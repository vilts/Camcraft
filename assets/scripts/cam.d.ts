// CamCraft job-script API - the `cam` global of a batch run.
//
// GENERATED SHAPE: the verb lines below are the VERBS table in
// crates/app/src/script/mod.rs, and the law test in
// crates/app/tests/scripting.rs fails if the two drift. The types
// they name are this repo's own views: the operation table, the tool
// list, the post gate. Provenance: clean-room, specced from ROADMAP
// M5.3 F ("Typescript bindings for batch ops").
//
// There is no `fs`, no `fetch`, no `console` in a job - every side
// effect is a cam verb, so every world write rides the command
// history and nothing happens the batch report cannot show.

type OpStatus = "ok" | "stale" | "never";

interface OperationInfo {
  name: string;
  kind: string;
  /** null for the implicit first setup (the document's own law). */
  setup: string | null;
  status: OpStatus;
  /** Generation warnings carried by the cached path, if any. */
  warnings?: string[];
}

/** A DXF profile placed on the stock of a setup (not a model). */
interface DxfProfile {
  profile: string;
  /** null for the implicit first setup. */
  setup: string | null;
  /** stated: the file's own unit (null: none stated); used: the unit the drawing was read in. */
  units: { stated: string | null; used: string; from: "file" | "caller"; mm_per_unit: number };
  layers: string[];
  primitives: number;
  join_tol: number;
  chains: { closed: number; open: number; rejected: number };
  /** Skipped DXF entities, counted by kind (TEXT, DIMENSION, ...). */
  skipped: { [kind: string]: number };
  skipped_total: number;
  bounds: { min: number[]; max: number[] } | null;
  /** On the top face of the setup's stock, from the stock origin: mm and degrees. */
  placement: { x: number; y: number; angle: number };
  origin: number[] | null;
  rotation: number[] | null;
  summary?: string;
}

/** A text engraving profile placed on the stock of a setup. */
interface TextProfile extends DxfProfile {
  text: string;
  height: number;
}

interface DocumentInfo {
  path: string;
  operations: OperationInfo[];
}

interface VerifyReport {
  /** The post's whole verdict: every op in scope fresh, the gate passed,
   *  and no op in scope holding the post with its own warnings. */
  ok: boolean;
  operations: OperationInfo[];
  /** The program-level gate; skipped (with reason) while anything is stale. */
  gate: { ok: boolean; problems: string[]; skipped?: string };
  /** Ops in scope whose own cached warnings refuse the post (#722). */
  held_by: string[];
}

interface PostOptions {
  /** The .nc (or .cnc) file to write. Required. */
  to: string;
  /** A built-in post name - "Fanuc", "Haas", "Siemens 840D",
   *  "Heidenhain", "Mazak", "Tormach PathPilot", "grblHAL", "TinyG", "LinuxCNC" - or the
   *  path of a .cps file.
   *  Omitted means the document's own post. */
  cps?: string;
  /** Also return the program text in the result. */
  program?: boolean;
  /** Per-post declared option overrides for this run. */
  options?: Record<string, boolean | number | string>;
}

interface PostResult {
  path: string;
  ops: number;
  lines: number;
  warning?: string;
  program?: string;
  /** Setups on different machines or posts, or per-op output: every file written. */
  files?: string[];
  mounts?: PostedMountInfo[];
  /** One file per operation (`per_op` enabled): per-operation summaries when multiple files are written. */
  operations?: PostedOpInfo[];
}

interface PostedMountInfo {
  path: string;
  setups: string[];
  machine: string;
  post: string;
  ops: number;
  lines: number;
  program?: string;
}

interface PostedOpInfo {
  path: string;
  operation: string;
  lines: number;
  program?: string;
}

interface ToolRow {
  tool: string;
  ops: string[];
  cut_mm: number;
  rapid_mm: number;
  minutes: number;
  changes: number;
}

interface JobReport {
  document: string | null;
  posted: string | null;
  operations: OperationInfo[];
  tools: ToolRow[];
  minutes: number;
  cut_mm: number;
  rapid_mm: number;
}

interface CamApi {
  load(path: string): DocumentInfo;  // arity 1
  save(path?: string | null, options?: { overwrite?: boolean }): { path: string };  // variadic
  import(path: string, name?: string | null, facetBudget?: number | null, units?: "mm" | "in"): { model: string; facets: number; facet_budget?: number; notes?: string[] };  // variadic
  importDxf(path: string, options?: { setup?: string; units?: "mm" | "in"; join_tol?: number; name?: string }): DxfProfile;  // variadic
  importSvg(path: string, options?: { setup?: string; units?: "mm" | "in"; join_tol?: number; name?: string }): DxfProfile;  // variadic
  insertText(text: string, options: { height: number; position?: [number, number]; angle?: number; letter_spacing?: number; setup?: string; name?: string }): TextProfile;  // variadic
  exportTools(path: string, format?: "auto" | "camtools" | "pathpilot" | "hurco", units?: "mm" | "inch"): { path: string; tools: number; format: string };  // variadic
  importTools(path: string, mode?: "add" | "update" | "replace", holders?: boolean, format?: "auto" | "camtools" | "pathpilot" | "hurco", units?: "mm" | "inch"): { path: string; format: string; mode: string; added: string[]; renamed: { from: string; to: string }[]; skipped: string[]; updated: string[]; replaced: string[]; removed: string[]; holders: number; holders_left_out: number; summary: string };  // variadic
  operations(): OperationInfo[];  // arity 0
  generate(...names: string[]): OperationInfo[];  // variadic
  verify(...names: string[]): VerifyReport;  // variadic
  post(options: PostOptions): PostResult;  // arity 1
  report(): JobReport;  // arity 0
  check(): VerifyReport;  // arity 0
  result(): unknown;  // arity 0
  log(...values: unknown[]): null;  // variadic
  call(tool: string, args?: Record<string, unknown>): unknown;  // variadic
}

declare const cam: CamApi;
