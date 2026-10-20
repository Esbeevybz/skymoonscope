/**
 * Parse a WASM binary and return the sorted names of all publicly exported
 * functions (names starting with `_` are excluded as internal).
 *
 * Returns `[]` when the buffer is not valid WASM or has no function exports.
 */
export declare function extractFunctionNamesFromWasm(buffer: ArrayBuffer): string[];
