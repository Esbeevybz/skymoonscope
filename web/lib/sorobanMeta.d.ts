/**
 * TypeScript declarations for lib/sorobanMeta.js
 *
 * Covers the Soroban-specific WASM custom-section decoder
 * (contractmetav0 + contractspecv0).
 */

/** One key/value pair from the `contractmetav0` custom section. */
export interface SorobanMetaKV {
  key: string;
  value: string;
}

/** A single function-argument descriptor decoded from `contractspecv0`. */
export interface SorobanSpecFunctionArg {
  /** Argument documentation comment (may be empty). */
  doc: string;
  /** Argument name as declared in the contract source. */
  name: string;
  /** Human-readable type name, e.g. "U64", "Address", "Vec<Symbol>". */
  type: string;
}

/**
 * A decoded function entry from `contractspecv0`.
 * `kind` is always `"function"`.
 */
export interface SorobanSpecFunction {
  kind: 'function';
  /** Contract entry point name (matches the WASM export). */
  name: string;
  /** Doc comment from the contract source. */
  doc: string;
  inputs: SorobanSpecFunctionArg[];
  /** Return type names, in declaration order. Empty for `void` functions. */
  outputs: string[];
}

/**
 * A decoded user-defined type from `contractspecv0`.
 * `kind` is one of `"struct"` | `"union"` | `"enum"` | `"error_enum"`.
 */
export interface SorobanSpecType {
  kind: 'struct' | 'union' | 'enum' | 'error_enum';
  name: string;
  doc: string;
}

/** Full decoded result returned by `decodeSorobanMeta()`. */
export interface SorobanMetaDecodeResult {
  /** Entries from `contractmetav0` (build metadata). */
  contractMeta: SorobanMetaKV[];
  /** Function ABI entries from `contractspecv0`. */
  specFunctions: SorobanSpecFunction[];
  /** Type-definition entries from `contractspecv0`. */
  specTypes: SorobanSpecType[];
  /** Decode errors (non-fatal; partial results still returned). */
  errors: string[];
  /** Informational warnings (e.g. no Soroban sections found). */
  warnings: string[];
}

/** Map from ScSpecType discriminant integer to its name. */
export declare const SPEC_TYPE_NAMES: Record<number, string>;

/**
 * Find and return named WASM custom sections from a binary.
 * @param bytes     - Raw WASM bytes
 * @param predicate - Return `true` for sections you want to collect
 */
export declare function findCustomSections(
  bytes: Uint8Array,
  predicate: (name: string) => boolean,
): Array<{ name: string; payload: Uint8Array }>;

/**
 * Decode raw `contractmetav0` payload bytes into key/value entries.
 */
export declare function decodeMetaSection(payload: Uint8Array): {
  entries: SorobanMetaKV[];
  errors: string[];
};

/**
 * Decode raw `contractspecv0` payload bytes into functions and type definitions.
 */
export declare function decodeSpecSection(payload: Uint8Array): {
  functions: SorobanSpecFunction[];
  types: SorobanSpecType[];
  errors: string[];
};

/**
 * Decode Soroban metadata from a complete WASM binary.
 *
 * Never throws.  Decode failures are reported in the `errors` field of the
 * returned object; a partial result is always returned.
 */
export declare function decodeSorobanMeta(
  input: ArrayBuffer | Uint8Array,
): SorobanMetaDecodeResult;
