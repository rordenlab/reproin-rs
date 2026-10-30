//! BIDS / ReproIn constants shared across passes. Update with BIDS releases.

/// `_BIDS_EXTS`: companions that move together as one series family.
pub const BIDS_EXTS: [&str; 7] = [
    ".nii.gz", ".nii", ".json", ".bval", ".bvec", ".tsv", ".tsv.gz",
];

/// `_BIDS_DATATYPES`. "ct" is deliberately absent until BEP-024 is in the
/// validator (CT stays in Unknown/ under -f %H).
pub const BIDS_DATATYPES: [&str; 14] = [
    "anat", "func", "dwi", "fmap", "perf", "pet", "mrs", "meg", "eeg", "ieeg", "beh", "micr",
    "nirs", "motion",
];

pub fn is_datatype(s: &str) -> bool {
    BIDS_DATATYPES.contains(&s)
}

/// Entities that follow `run` in canonical filename order.
pub const POST_RUN_ENTITIES: [&str; 9] = [
    "echo",
    "flip",
    "inv",
    "mt",
    "part",
    "proc",
    "mod",
    "recording",
    "chunk",
];

pub const FMAP_EPI_ENTITY_ORDER: [&str; 6] = ["sub", "ses", "acq", "ce", "dir", "run"];

/// func suffixes that take a `part` entity (noRF: Siemens RF-off noise).
pub const PART_FUNC_SUFFIXES: [&str; 3] = ["bold", "sbref", "noRF"];

/// heudiconv `MATCHING_PARAMETERS`.
pub const MATCHING_PARAMETERS: [&str; 2] = ["ImagingVolume", "Shims"];

pub const PROVENANCE_TSV: &str = ".reproin_provenance.tsv";

pub const DEFAULT_MIN_VOLUMES: i64 = 5;

// Scaffolding copied verbatim from heudiconv's reproin output; keep in sync
// with upstream, do not polish the TODO wording.
pub const CHANGES_TEMPLATE: &str = "0.0.1  Initial data acquired\nTODOs:\n\t- verify and possibly extend information in participants.tsv (see for example http://datasets.datalad.org/?dir=/openfmri/ds000208)\n\t- fill out dataset_description.json, README, sourcedata/README (if present)\n\t- provide _events.tsv file for each _bold.nii.gz with onsets of events (see  '8.5 Task events'  of BIDS specification)";

pub const README_TEMPLATE: &str = "TODO: Provide description for the dataset -- basic details about the study, possibly pointing to pre-registration (if public or embargoed)";

pub const DATASET_DESCRIPTION: &str = r#"{
  "Acknowledgements": "We thank Terry Sacket and the rest of the DBIC (Dartmouth Brain Imaging Center) personnel for assistance in data collection, and Yaroslav O. Halchenko for preparing BIDS dataset. TODO: adjust to your case.",
  "Authors": ["TODO:", "First1 Last1", "First2 Last2", "..."],
  "BIDSVersion": "1.8.0",
  "DatasetDOI": "TODO: eventually a DOI for the dataset",
  "Funding": ["TODO", "GRANT #1", "GRANT #2"],
  "HowToAcknowledge": "TODO: describe how to acknowledge -- either cite a corresponding paper, or just in acknowledgement section",
  "License": "TODO: choose a license, e.g. PDDL (http://opendatacommons.org/licenses/pddl/)",
  "Name": "TODO: name of the dataset",
  "ReferencesAndLinks": ["TODO", "List of papers or websites"]
}"#;

pub const PARTICIPANTS_JSON: &str = r#"{
  "participant_id": {"Description": "Participant identifier"},
  "age": {"Description": "Age in years (TODO - verify) as in the initial session, might not be correct for other sessions"},
  "sex": {"Description": "self-rated by participant, M for male/F for female (TODO: verify)"},
  "group": {"Description": "(TODO: adjust - by default everyone is in control group)"}
}"#;

pub const SCANS_JSON: &str = r#"{
  "filename": {"Description": "Name of the nifti file"},
  "acq_time": {"LongName": "Acquisition time", "Description": "Acquisition time of the particular scan"},
  "operator": {"Description": "Name of the operator"},
  "randstr": {"LongName": "Random string", "Description": "md5 hash of UIDs"}
}"#;

pub const DCM2NIIX_DDESC_NAME: &str = "dcm2niix dummy dataset";
