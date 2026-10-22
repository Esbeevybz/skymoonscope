export declare const WASM_MAGIC: number[];
export declare const WASM_VERSION: number;
export declare const SECTION_NAMES: Record<number, string>;

export declare class WasmDecodeError extends Error {
  offset: number;
  constructor(message: string, offset: number);
}

export interface WasmSectionSummary {
  id: number;
  name: string;
  size: number;
}

export interface WasmExportEntry {
  name: string;
  kind: string;
  index: number;
}

export interface WasmImportEntry {
  module: string;
  name: string;
  kind: string;
}

export interface WasmDataSegment {
  /** Zero-based index within the data section. */
  index: number;
  /** "active" | "passive" | "unknown(n)" */
  kind: string;
  /** Target linear memory index; null for passive segments. */
  memoryIndex: number | null;
  /**
   * Decoded i32.const offset for active segments with a simple constant
   * expression; null when the offset uses a more complex expression (e.g.,
   * global.get) or when the segment is passive.
   */
  offsetValue: number | null;
  /** Byte length of the initialisation data. */
  byteLength: number;
}

export interface WasmValidationReport {
  valid: boolean;
  byteLength: number;
  version: number | null;
  sections: WasmSectionSummary[];
  exports: WasmExportEntry[];
  imports: WasmImportEntry[];
  dataSegments: WasmDataSegment[];
  sorobanMeta?: import('./sorobanMeta').SorobanMetaDecodeResult;
  errors: string[];
  warnings: string[];
  durationMs: number;
}

export interface WasmValidationOptions {
  /** Reject modules larger than this many bytes. */
  maxBytes?: number;
}

export declare function hasWasmMagic(input: ArrayBuffer | Uint8Array | number[]): boolean;

export declare function readVarUint32(
  bytes: Uint8Array,
  offset: number,
): { value: number; next: number };

export declare function parseWasmSections(
  input: ArrayBuffer | Uint8Array,
): Array<WasmSectionSummary & { start: number; end: number }>;

export declare function parseDataSection(
  bytes: Uint8Array,
  start: number,
  end: number,
): WasmDataSegment[];

export declare function validateWasmModule(
  input: ArrayBuffer | Uint8Array | number[],
  options?: WasmValidationOptions,
): WasmValidationReport;

export declare function extractContractFunctions(report: WasmValidationReport): string[];
