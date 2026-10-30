//! Minimal NIfTI-1 header reader and volume remover (ports reproinx.py's
//! stdlib struct code). NIfTI-2 and other formats are reported as `None`.

use std::io::{Read, Write};

/// Bytes a host must supply for header-only reads (`_read_nifti_header`).
pub const HEADER_BYTES: usize = 348;

pub struct Header {
    pub big_endian: bool,
    pub dim: [i16; 8],
    pub bitpix: i16,
    pub pixdim: [f32; 8],
    pub vox_offset: f32,
    qform_code: i16,
    sform_code: i16,
    quatern: [f32; 6],
    srow: [[f32; 4]; 3],
}

impl Header {
    pub fn parse(h: &[u8]) -> Option<Header> {
        if h.len() < HEADER_BYTES {
            return None;
        }
        let be = if i32::from_le_bytes(h[0..4].try_into().unwrap()) == 348 {
            false
        } else if i32::from_be_bytes(h[0..4].try_into().unwrap()) == 348 {
            true
        } else {
            return None;
        };
        let i16_at = |o: usize| {
            let b = [h[o], h[o + 1]];
            if be {
                i16::from_be_bytes(b)
            } else {
                i16::from_le_bytes(b)
            }
        };
        let f32_at = |o: usize| {
            let b: [u8; 4] = h[o..o + 4].try_into().unwrap();
            if be {
                f32::from_be_bytes(b)
            } else {
                f32::from_le_bytes(b)
            }
        };
        Some(Header {
            big_endian: be,
            dim: std::array::from_fn(|i| i16_at(40 + 2 * i)),
            bitpix: i16_at(72),
            pixdim: std::array::from_fn(|i| f32_at(76 + 4 * i)),
            vox_offset: f32_at(108),
            qform_code: i16_at(252),
            sform_code: i16_at(254),
            quatern: std::array::from_fn(|i| f32_at(256 + 4 * i)),
            srow: std::array::from_fn(|r| std::array::from_fn(|c| f32_at(280 + 16 * r + 4 * c))),
        })
    }

    /// `_nifti_ndim`: volume count, 1 for 3D.
    pub fn ndim(&self) -> i64 {
        if self.dim[0] < 4 {
            return 1;
        }
        if self.dim[4] > 0 {
            self.dim[4] as i64
        } else {
            1
        }
    }

    pub fn shape3(&self) -> [i64; 3] {
        [self.dim[1] as i64, self.dim[2] as i64, self.dim[3] as i64]
    }

    /// nibabel `get_best_affine` order: sform, then qform, else None.
    pub fn affine(&self) -> Option<[[f64; 4]; 4]> {
        let last = [0.0, 0.0, 0.0, 1.0];
        if self.sform_code > 0 {
            let r = |i: usize| self.srow[i].map(|x| x as f64);
            return Some([r(0), r(1), r(2), last]);
        }
        if self.qform_code <= 0 {
            return None;
        }
        let [b, c, d, ox, oy, oz] = self.quatern.map(|x| x as f64);
        let p0 = self.pixdim[0] as f64;
        let qfac = if p0 == -1.0 || p0 == 1.0 { p0 } else { 1.0 };
        let a = (1.0 - b * b - c * c - d * d).max(0.0).sqrt();
        let (sx, sy, sz) = (
            self.pixdim[1] as f64,
            self.pixdim[2] as f64,
            qfac * self.pixdim[3] as f64,
        );
        Some([
            [
                (a * a + b * b - c * c - d * d) * sx,
                (2.0 * b * c - 2.0 * a * d) * sy,
                (2.0 * b * d + 2.0 * a * c) * sz,
                ox,
            ],
            [
                (2.0 * b * c + 2.0 * a * d) * sx,
                (a * a + c * c - b * b - d * d) * sy,
                (2.0 * c * d - 2.0 * a * b) * sz,
                oy,
            ],
            [
                (2.0 * b * d - 2.0 * a * c) * sx,
                (2.0 * c * d + 2.0 * a * b) * sy,
                (a * a + d * d - b * b - c * c) * sz,
                oz,
            ],
            last,
        ])
    }

    /// Preconditions `_nifti_drop_volumes` checks before touching voxels.
    pub fn can_drop(&self) -> bool {
        self.volume_bytes().is_some()
            && self.dim[4] > 1
            && self.dim[1..4].iter().all(|&d| d > 0)
            && self.bitpix > 0
            && self.bitpix % 8 == 0
            && (352.0..2_147_483_648.0).contains(&self.vox_offset)
    }

    /// Bytes of one volume; None on non-positive dims or usize overflow
    /// (wasm32 is 32-bit).
    pub fn volume_bytes(&self) -> Option<usize> {
        if self.bitpix <= 0 || self.dim[1..4].iter().any(|&d| d <= 0) {
            return None;
        }
        self.dim[1..4]
            .iter()
            .try_fold(self.bitpix as usize / 8, |n, &d| n.checked_mul(d as usize))
    }
}

pub fn set_dim4(h: &mut [u8], big_endian: bool, n: i16) {
    let b = if big_endian {
        n.to_be_bytes()
    } else {
        n.to_le_bytes()
    };
    h[48..50].copy_from_slice(&b);
}

/// `_nifti_drop_volumes` as a push filter over uncompressed NIfTI-1 bytes:
/// copies the header (dim[4] updated) and kept volumes, buffering at most one
/// volume. `push` fails when the image cannot be trimmed (the reference then
/// leaves image and sidecar unchanged); `finish` fails on a short body. Either
/// way the caller must discard partial output.
pub struct VolumeFilter {
    drop: Vec<usize>,
    buf: Vec<u8>,
    /// None while reading the header; then (volume size, volumes, next volume).
    body: Option<(usize, usize, usize)>,
}

impl VolumeFilter {
    pub fn new(drop: &[usize]) -> Self {
        VolumeFilter {
            drop: drop.to_vec(),
            buf: Vec::new(),
            body: None,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        if self.body.is_none() {
            if self.buf.len() < 352 {
                return Ok(out);
            }
            let h = Header::parse(&self.buf).ok_or("not a NIfTI-1 image")?;
            if !h.can_drop() {
                return Err(
                    "image cannot be trimmed (needs 4D, whole-byte voxels, vox_offset >= 352)"
                        .into(),
                );
            }
            let vox = h.vox_offset as usize;
            if self.buf.len() < vox {
                return Ok(out);
            }
            let nt = h.dim[4] as usize;
            let keep = (0..nt).filter(|i| !self.drop.contains(i)).count();
            let mut hdr: Vec<u8> = self.buf.drain(..vox).collect();
            set_dim4(&mut hdr, h.big_endian, keep as i16);
            out.extend(hdr);
            self.body = Some((h.volume_bytes().unwrap(), nt, 0));
        }
        let (volsz, nt, ref mut i) = *self.body.as_mut().unwrap();
        let mut at = 0;
        while *i < nt && self.buf.len() - at >= volsz {
            if !self.drop.contains(i) {
                out.extend_from_slice(&self.buf[at..at + volsz]);
            }
            at += volsz;
            *i += 1;
        }
        self.buf.drain(..at);
        Ok(out)
    }

    pub fn finish(&self) -> Result<(), String> {
        match self.body {
            Some((_, nt, i)) if i == nt => Ok(()),
            _ => Err("image is truncated".into()),
        }
    }
}

/// Stream `r` through a `VolumeFilter` into `w`.
pub fn drop_volumes<R: Read, W: Write>(mut r: R, mut w: W, drop: &[usize]) -> std::io::Result<()> {
    let mut f = VolumeFilter::new(drop);
    let mut chunk = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        w.write_all(&f.push(&chunk[..n]).map_err(std::io::Error::other)?)?;
    }
    f.finish().map_err(std::io::Error::other)?;
    w.flush()
}

/// First `n` decompressed bytes (header reads without inflating the image).
pub fn gunzip_prefix(b: impl Read, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = flate2::read::MultiGzDecoder::new(b)
        .take(n as u64)
        .read_to_end(&mut out);
    out
}

/// Apply a `DropVolumes` op from `src` to `dst` (`.nii` or `.nii.gz` by name);
/// gzip output is level 9 with mtime 0, like `gzip.compress(..., mtime=0)`.
pub fn drop_volumes_file<R: Read, W: Write>(
    path: &str,
    src: R,
    dst: W,
    drop: &[usize],
) -> std::io::Result<()> {
    if path.ends_with(".gz") {
        let mut e = flate2::GzBuilder::new()
            .mtime(0)
            .write(dst, flate2::Compression::best());
        drop_volumes(flate2::read::MultiGzDecoder::new(src), &mut e, drop)?;
        e.finish()?;
        Ok(())
    } else {
        drop_volumes(src, dst, drop)
    }
}

#[cfg(test)]
pub(crate) fn synth(dim: [i16; 8], bitpix: i16, sform: bool) -> Vec<u8> {
    let mut h = vec![0u8; 352];
    h[0..4].copy_from_slice(&348i32.to_le_bytes());
    for (i, d) in dim.iter().enumerate() {
        h[40 + 2 * i..42 + 2 * i].copy_from_slice(&d.to_le_bytes());
    }
    h[72..74].copy_from_slice(&bitpix.to_le_bytes());
    h[108..112].copy_from_slice(&352f32.to_le_bytes());
    for i in 0..4 {
        h[76 + 4 * i..80 + 4 * i].copy_from_slice(&1f32.to_le_bytes());
    }
    if sform {
        h[254..256].copy_from_slice(&1i16.to_le_bytes());
        for r in 0..3 {
            h[280 + 16 * r + 4 * r..284 + 16 * r + 4 * r].copy_from_slice(&2f32.to_le_bytes());
        }
    }
    let n: usize = dim[1..5]
        .iter()
        .map(|&d| d.max(1) as usize)
        .product::<usize>()
        * (bitpix as usize / 8);
    h.extend((0..n).map(|i| (i % 251) as u8));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_middle_volumes() {
        let raw = synth([4, 2, 2, 1, 5, 1, 1, 1], 16, true);
        let mut out = Vec::new();
        drop_volumes(&raw[..], &mut out, &[1, 2]).unwrap();
        let h = Header::parse(&out).unwrap();
        assert_eq!(h.dim[4], 3);
        let volsz = 2 * 2 * 2;
        assert_eq!(&out[352..352 + volsz], &raw[352..352 + volsz]);
        assert_eq!(
            &out[352 + volsz..352 + 2 * volsz],
            &raw[352 + 3 * volsz..352 + 4 * volsz]
        );
        assert_eq!(out.len(), 352 + 3 * volsz);
        let mut gz = Vec::new();
        let mut e = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        e.write_all(&raw).unwrap();
        e.finish().unwrap();
        let mut out_gz = Vec::new();
        drop_volumes_file("x.nii.gz", &gz[..], &mut out_gz, &[0]).unwrap();
        assert_eq!(
            Header::parse(&gunzip_prefix(&out_gz[..], 352)).unwrap().dim[4],
            4
        );
        assert!(drop_volumes(&raw[..380], &mut Vec::new(), &[0]).is_err()); // truncated body
        assert!(
            drop_volumes(
                &synth([3, 2, 2, 1, 1, 1, 1, 1], 16, true)[..],
                &mut Vec::new(),
                &[0]
            )
            .is_err()
        );
        // Byte-at-a-time pushes give the same output as one push.
        let mut f = VolumeFilter::new(&[1, 2]);
        let one: Vec<u8> = raw.iter().flat_map(|b| f.push(&[*b]).unwrap()).collect();
        f.finish().unwrap();
        assert_eq!(one, out);
        assert_eq!(h.affine().unwrap()[0][0], 2.0);
        // 1024^3 x 4 bytes overflows a 32-bit usize: refuse instead of wrapping.
        let mut big = Header::parse(&raw).unwrap();
        big.dim = [4, 1024, 1024, 1024, 2, 1, 1, 1];
        big.bitpix = 32;
        assert_eq!(big.volume_bytes().is_some(), usize::BITS > 32);
    }
}
