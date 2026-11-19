/**
 * Client-side WASM export section parser.
 *
 * Reads the binary WASM export section (section id 7) and returns the names
 * of all publicly exported functions. This is intentionally dependency-free
 * so it can run in both the main thread and a Web Worker without bundler
 * gymnastics, and so the pure logic can be unit tested directly with Node's
 * built-in test runner.
 *
 * The implementation mirrors the filter applied by the server-side
 * `extractContractFunctions` helper: names starting with `_` are treated as
 * private/internal exports and are excluded from the result.
 *
 * @param {ArrayBuffer} buffer  The raw bytes of a compiled `.wasm` file.
 * @returns {string[]}  Sorted list of public function export names, or `[]`
 *                      when the buffer is not valid WASM or has no exports.
 */
function extractFunctionNamesFromWasm(buffer) {
  try {
    const bytes = new Uint8Array(buffer);

    // Validate WASM magic (\0asm) + version (1, little-endian) — 8-byte header.
    if (
      bytes.length < 8 ||
      bytes[0] !== 0x00 || bytes[1] !== 0x61 ||
      bytes[2] !== 0x73 || bytes[3] !== 0x6d ||
      bytes[4] !== 0x01 || bytes[5] !== 0x00 ||
      bytes[6] !== 0x00 || bytes[7] !== 0x00
    ) {
      return [];
    }

    let offset = 8;

    /** Read a LEB128-encoded unsigned 32-bit integer, advancing `offset`. */
    function readVarUint32() {
      let result = 0;
      let shift = 0;
      while (offset < bytes.length) {
        const byte = bytes[offset++];
        result |= (byte & 0x7f) << shift;
        if ((byte & 0x80) === 0) break;
        shift += 7;
      }
      return result >>> 0;
    }

    /** Read a UTF-8 string of `len` bytes, advancing `offset`. */
    function readString(len) {
      const slice = bytes.subarray(offset, offset + len);
      offset += len;
      // TextDecoder is available in Node ≥ 11 and all modern browsers.
      return new TextDecoder().decode(slice);
    }

    const EXPORT_SECTION_ID = 7;
    const EXTERNAL_KIND_FUNCTION = 0;

    while (offset < bytes.length) {
      const sectionId = bytes[offset++];
      const sectionSize = readVarUint32();
      const sectionEnd = offset + sectionSize;

      if (sectionId === EXPORT_SECTION_ID) {
        const exportCount = readVarUint32();
        const names = [];

        for (let i = 0; i < exportCount; i++) {
          const nameLen = readVarUint32();
          const name = readString(nameLen);
          const kind = bytes[offset++]; // external kind byte (0=func,1=table,2=mem,3=global)
          readVarUint32(); // export index — not needed here

          // Keep only function exports whose names are non-empty and not
          // internal (prefixed with `_`).
          if (kind === EXTERNAL_KIND_FUNCTION && name.length > 0 && !name.startsWith('_')) {
            names.push(name);
          }
        }

        return names.sort((a, b) => a.localeCompare(b));
      }

      // This is not the export section — skip past it.
      offset = sectionEnd;
    }

    // No export section found (valid but empty contract).
    return [];
  } catch {
    // Malformed binary or unexpected encoding — return empty rather than throw.
    return [];
  }
}

// CommonJS export so the function is unit-testable from `.cjs` test files and
// importable by TypeScript consumers via the companion `.d.ts`.
if (typeof module !== 'undefined') {
  module.exports = { extractFunctionNamesFromWasm };
}
