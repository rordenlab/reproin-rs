export type Tree = Map<string, Blob>
export interface Options {
  strict?: boolean
  keepDerivatives?: boolean
  minVolumes?: number
  shiftDates?: boolean
  /** Name of the virtual output root (default "output"). */
  rootName?: string
}
export type Op =
  | { op: 'mkdir' | 'rmdir' | 'delete'; path: string }
  | { op: 'rename'; from: string; to: string }
  | { op: 'write'; path: string; text: string }
  | { op: 'dropVolumes'; path: string; indices: number[] }
export interface PlanResult {
  ops: Op[]
  log: string[]
  failures: string[]
  roots: string[]
  /** Set when a --strict or --shift-dates failure aborted the post-pass. */
  fatal: string | null
}
export interface ApplyOptions {
  onProgress?: (fraction: number) => void
  signal?: AbortSignal
  dirs?: Iterable<string>
}
export function load(source?: string | URL | BufferSource | WebAssembly.Module): Promise<unknown>
export function convertArgs(subject?: string, session?: string, anonymize?: boolean): string[]
export function plan(tree: Tree, options?: Options, dirs?: Iterable<string>): Promise<PlanResult>
export function apply(tree: Tree, ops: Op[], options?: ApplyOptions): Promise<{ files: Tree; dirs: Set<string> }>
export function postPass(tree: Tree, options?: Options & ApplyOptions): Promise<PlanResult & { files: Tree; dirs: Set<string> }>
export interface ConvertOptions extends Options, ApplyOptions {
  subject?: string
  session?: string
  anonymize?: boolean
  onLog?: (text: string, stream: 'stdout' | 'stderr') => void
  /** dcm2niix convert(); default: the bundled published @niivue/dcm2niix 1.4.20261029. */
  convert?: (files: ArrayLike<File>, o: { args: string[]; signal?: AbortSignal; onLog?: ConvertOptions['onLog'] }) => Promise<{ files: Array<File & { relativePath: string }>; exitCode: number; log: string[] }>
}
export function convertToBids(dicomFiles: ArrayLike<File>, options?: ConvertOptions): Promise<PlanResult & { files: Tree; dirs: Set<string>; exitCode: number; dcm2niixLog: string[] }>
/** Store-only ZIP (< 4 GiB). */
export function zip(tree: Tree, prefix?: string): Promise<Blob>
