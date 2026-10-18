/**
 * Pure WASM binary decoding + validation helpers.
 *
 * Served from `public/` so the classic Web Worker in `wasmWorker.js` can pull it
 * in with `importScripts()` without any bundler involvement, while the app and
 * `node --test` load the very same file through `lib/wasmValidation.js`.
 *
 * Hence the UMD-style wrapper: one implementation, three consumers.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  } else {
    root.SkyMoonScopeWasmValidation = api;
  }
})(typeof self !== 'undefined' ? self : globalThis, function () {

  const WASM_MAGIC = [0x00, 0x61, 0x73, 0x6d];
  const WASM_VERSION = 1;

  const SECTION_NAMES = {
    0: 'custom',
    1: 'type',
    2: 'import',
    3: 'function',
    4: 'table',
    5: 'memory',
    6: 'global',
    7: 'export',
    8: 'start',
    9: 'element',
    10: 'code',
    11: 'data',
    12: 'data count',
  };

  const EXTERNAL_KINDS = {
    0: 'function',
    1: 'table',
    2: 'memory',
    3: 'global',
  };

  class WasmDecodeError extends Error {
    constructor(message, offset) {
      super(message);
      this.name = 'WasmDecodeError';
      this.offset = offset;
    }
  }

  function toBytes(input) {
    if (input instanceof Uint8Array) return input;
    if (input instanceof ArrayBuffer) return new Uint8Array(input);
    if (ArrayBuffer.isView(input)) {
      return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
    }
    if (Array.isArray(input)) return Uint8Array.from(input);
    throw new WasmDecodeError('Unsupported input: expected ArrayBuffer or Uint8Array', 0);
  }

  /** Decode an unsigned LEB128 integer. Returns `{ value, next }`. */
  function readVarUint32(bytes, offset) {
    let result = 0;
    let shift = 0;
    let cursor = offset;

    while (true) {
      if (cursor >= bytes.length) {
        throw new WasmDecodeError('Unexpected end of buffer while reading LEB128 integer', cursor);
      }
      const byte = bytes[cursor];
      cursor += 1;
      result += (byte & 0x7f) * Math.pow(2, shift);
      if ((byte & 0x80) === 0) break;
      shift += 7;
      if (shift > 35) {
        throw new WasmDecodeError('LEB128 integer is too large for u32', cursor);
      }
    }

    return { value: result, next: cursor };
  }

  function readName(bytes, offset) {
    const { value: length, next } = readVarUint32(bytes, offset);
    const end = next + length;
    if (end > bytes.length) {
      throw new WasmDecodeError('Name length exceeds buffer size', next);
    }
    const slice = bytes.subarray(next, end);
    const decoded =
      typeof TextDecoder !== 'undefined'
        ? new TextDecoder('utf-8').decode(slice)
        : Array.from(slice, (b) => String.fromCharCode(b)).join('');
    return { value: decoded, next: end };
  }

  /** True when the buffer starts with the `\0asm` magic number. */
  function hasWasmMagic(input) {
    let bytes;
    try {
      bytes = toBytes(input);
    } catch {
      return false;
    }
    if (bytes.length < WASM_MAGIC.length) return false;
    return WASM_MAGIC.every((byte, index) => bytes[index] === byte);
  }

  function readVersion(bytes) {
    return bytes[4] | (bytes[5] << 8) | (bytes[6] << 16) | (bytes[7] << 24);
  }

  function parseExportSection(bytes, start, end) {
    const exports = [];
    let cursor = start;
    const { value: count, next } = readVarUint32(bytes, cursor);
    cursor = next;

    for (let i = 0; i < count && cursor < end; i += 1) {
      const name = readName(bytes, cursor);
      cursor = name.next;
      if (cursor >= end) {
        throw new WasmDecodeError('Truncated export entry', cursor);
      }
      const kindByte = bytes[cursor];
      cursor += 1;
      const index = readVarUint32(bytes, cursor);
      cursor = index.next;
      exports.push({
        name: name.value,
        kind: EXTERNAL_KINDS[kindByte] || `unknown(${kindByte})`,
        index: index.value,
      });
    }

    return exports;
  }

  /**
   * Skip the descriptor bytes following an import kind byte.
   *
   * Each import kind has a different descriptor shape in the binary format:
   *   0 (function): one LEB128 type index
   *   1 (table):    reftype byte + limits (flag byte + min LEB128 [+ max LEB128])
   *   2 (memory):   limits (flag byte + min LEB128 [+ max LEB128])
   *   3 (global):   valtype byte + mutability byte
   *
   * We only need the kind and names for the summary panel, so we skip all of
   * the descriptor bytes without interpreting them.
   */
  function skipImportDescriptor(bytes, cursor, kindByte, end) {
    if (kindByte === 0) {
      // function: type index (LEB128)
      const r = readVarUint32(bytes, cursor);
      return r.next;
    }
    if (kindByte === 1) {
      // table: reftype (1 byte) + limits
      cursor += 1; // skip reftype
      return skipLimits(bytes, cursor, end);
    }
    if (kindByte === 2) {
      // memory: limits only
      return skipLimits(bytes, cursor, end);
    }
    if (kindByte === 3) {
      // global: valtype (1 byte) + mutability (1 byte)
      cursor += 2;
      return cursor;
    }
    // Unknown kind — cannot reliably skip. Stop parsing further imports.
    return null;
  }

  /**
   * Skip a WASM limits block: flag byte + min [+ max].
   * flag 0x00 = no max, flag 0x01 = with max.
   */
  function skipLimits(bytes, cursor, end) {
    if (cursor >= end) {
      throw new WasmDecodeError('Truncated limits block', cursor);
    }
    const flag = bytes[cursor];
    cursor += 1;
    const min = readVarUint32(bytes, cursor);
    cursor = min.next;
    if (flag === 0x01) {
      const max = readVarUint32(bytes, cursor);
      cursor = max.next;
    }
    return cursor;
  }

  function parseImportSection(bytes, start, end) {
    const imports = [];
    let cursor = start;
    const { value: count, next } = readVarUint32(bytes, cursor);
    cursor = next;

    for (let i = 0; i < count && cursor < end; i += 1) {
      const moduleName = readName(bytes, cursor);
      cursor = moduleName.next;
      const fieldName = readName(bytes, cursor);
      cursor = fieldName.next;
      if (cursor >= end) {
        throw new WasmDecodeError('Truncated import entry', cursor);
      }
      const kindByte = bytes[cursor];
      cursor += 1;

      imports.push({
        module: moduleName.value,
        name: fieldName.value,
        kind: EXTERNAL_KINDS[kindByte] || `unknown(${kindByte})`,
      });

      // Skip the descriptor bytes so we can continue to the next import entry.
      const nextCursor = skipImportDescriptor(bytes, cursor, kindByte, end);
      if (nextCursor === null) {
        // Unknown descriptor kind; we cannot determine how many bytes to skip.
        break;
      }
      cursor = nextCursor;
    }

    return imports;
  }

  /**
   * Parse the data section (id = 11).
   *
   * The WASM binary format has two data segment variants relevant here:
   *   - Active (kind = 0): memory index 0 (implicit), offset expression, init bytes
   *   - Active (kind = 2): explicit memory index, offset expression, init bytes
   *   - Passive (kind = 1): no memory reference, just init bytes
   *
   * We record the index, kind string, byte length of the initialiser data, and
   * the offset constant when the segment is active with a simple i32.const.
   * We intentionally do not store the raw bytes — that would balloon the JSON
   * report that crosses the worker message channel for large modules.
   */
  function parseDataSection(bytes, start, end) {
    const segments = [];
    let cursor = start;

    const { value: count, next } = readVarUint32(bytes, cursor);
    cursor = next;

    for (let i = 0; i < count && cursor < end; i += 1) {
      const { value: segmentKind, next: afterKind } = readVarUint32(bytes, cursor);
      cursor = afterKind;

      let memoryIndex = 0;
      let offsetValue = null;

      if (segmentKind === 0) {
        // Active, implicit memory 0. Offset expression follows.
        const offset = readConstExprI32(bytes, cursor, end);
        offsetValue = offset.value;
        cursor = offset.next;
      } else if (segmentKind === 2) {
        // Active, explicit memory index. Memory index then offset expression.
        const memIdx = readVarUint32(bytes, cursor);
        memoryIndex = memIdx.value;
        cursor = memIdx.next;
        const offset = readConstExprI32(bytes, cursor, end);
        offsetValue = offset.value;
        cursor = offset.next;
      } else if (segmentKind === 1) {
        // Passive — no memory reference or offset.
        memoryIndex = null;
      } else {
        // Unknown segment kind; record what we have and stop parsing further.
        segments.push({
          index: i,
          kind: `unknown(${segmentKind})`,
          memoryIndex: null,
          offsetValue: null,
          byteLength: 0,
        });
        break;
      }

      // Init vector: LEB128 length + that many bytes.
      const { value: dataLen, next: afterLen } = readVarUint32(bytes, cursor);
      cursor = afterLen;

      if (cursor + dataLen > end) {
        throw new WasmDecodeError(
          `Data segment ${i} claims ${dataLen} bytes but only ${end - cursor} remain`,
          cursor,
        );
      }
      cursor += dataLen;

      segments.push({
        index: i,
        kind: segmentKind === 1 ? 'passive' : 'active',
        memoryIndex: segmentKind === 1 ? null : memoryIndex,
        offsetValue,
        byteLength: dataLen,
      });
    }

    return segments;
  }

  /**
   * Read a simple constant expression of the form `i32.const <imm> end`.
   *
   * WASM constant expressions in practice for data segment offsets are almost
   * always a single `i32.const` opcode (0x41) followed by a signed LEB128
   * immediate and an `end` opcode (0x0b). We handle that case and skip
   * gracefully otherwise.
   */
  function readConstExprI32(bytes, cursor, end) {
    if (cursor >= end) {
      throw new WasmDecodeError('Truncated constant expression', cursor);
    }
    const opcode = bytes[cursor];
    cursor += 1;

    let value = null;
    if (opcode === 0x41) {
      // i32.const: signed LEB128
      const r = readVarUint32(bytes, cursor);
      value = r.value;
      cursor = r.next;
    } else {
      // Non-i32.const opcode (e.g., global.get). We do not decode the value but
      // still need to advance past the expression up to the `end` (0x0b) byte.
      while (cursor < end && bytes[cursor] !== 0x0b) {
        cursor += 1;
      }
    }

    // Expect the `end` (0x0b) opcode that closes every const expression.
    if (cursor < end && bytes[cursor] === 0x0b) {
      cursor += 1;
    }

    return { value, next: cursor };
  }

  /**
   * Walk every top-level section of a WASM module.
   *
   * This is the expensive part of validation for large contracts, which is why it
   * is offloaded to a Web Worker in the browser.
   */
  function parseWasmSections(input) {
    const bytes = toBytes(input);
    const sections = [];
    let cursor = 8; // skip magic + version

    while (cursor < bytes.length) {
      const id = bytes[cursor];
      cursor += 1;
      const { value: size, next } = readVarUint32(bytes, cursor);
      cursor = next;
      const end = cursor + size;

      if (end > bytes.length) {
        throw new WasmDecodeError(
          `Section "${SECTION_NAMES[id] || id}" claims ${size} bytes but only ${
            bytes.length - cursor
          } remain`,
          cursor,
        );
      }

      sections.push({
        id,
        name: SECTION_NAMES[id] || `unknown(${id})`,
        size,
        start: cursor,
        end,
      });

      cursor = end;
    }

    return sections;
  }

  /**
   * Decode and validate a WASM module, returning a structured report.
   *
   * Never throws: decode failures are reported through `errors` so the caller (or
   * the worker message channel) always receives a serialisable result.
   */
  function validateWasmModule(input, options = {}) {
    const started = Date.now();
    const maxBytes = typeof options.maxBytes === 'number' ? options.maxBytes : null;
    const errors = [];
    const warnings = [];

    let bytes;
    try {
      bytes = toBytes(input);
    } catch (error) {
    return {
      valid: false,
      byteLength: 0,
      version: null,
      sections: [],
      exports: [],
      imports: [],
      dataSegments: [],
      errors: [error.message],
      warnings,
      durationMs: Date.now() - started,
    };
  }

  const base = {
    byteLength: bytes.length,
    version: null,
    sections: [],
    exports: [],
    imports: [],
    dataSegments: [],
    warnings,
    durationMs: 0,
  };

  if (bytes.length === 0) {
    errors.push('File is empty');
  } else if (bytes.length < 8) {
    errors.push('File is too small to be a WASM module (needs at least 8 bytes)');
  } else if (!hasWasmMagic(bytes)) {
    errors.push('Missing WASM magic number (\\0asm) — file is not WebAssembly bytecode');
  }

  if (maxBytes !== null && bytes.length > maxBytes) {
    errors.push(`Module is ${bytes.length} bytes which exceeds the ${maxBytes} byte limit`);
  }

  if (errors.length > 0) {
    return { ...base, valid: false, errors, durationMs: Date.now() - started };
  }

  const version = readVersion(bytes);
  if (version !== WASM_VERSION) {
    errors.push(`Unsupported WASM binary version ${version} (expected ${WASM_VERSION})`);
    return { ...base, valid: false, version, errors, durationMs: Date.now() - started };
  }

  let sections = [];
  let exports = [];
  let imports = [];
  let dataSegments = [];

  try {
    sections = parseWasmSections(bytes);
    const exportSection = sections.find((section) => section.id === 7);
    if (exportSection) {
      exports = parseExportSection(bytes, exportSection.start, exportSection.end);
    }
    const importSection = sections.find((section) => section.id === 2);
    if (importSection) {
      imports = parseImportSection(bytes, importSection.start, importSection.end);
    }
    const dataSection = sections.find((section) => section.id === 11);
    if (dataSection) {
      dataSegments = parseDataSection(bytes, dataSection.start, dataSection.end);
    }
  } catch (error) {
    errors.push(error.message);
    return {
      ...base,
      valid: false,
      version,
      sections,
      exports,
      imports,
      dataSegments,
      errors,
      durationMs: Date.now() - started,
    };
  }

  if (exports.length === 0) {
    warnings.push('Module exports nothing — Soroban contracts must export their entry points');
  }
  if (!sections.some((section) => section.id === 10)) {
    warnings.push('Module has no code section');
  }

  let sorobanMeta = undefined;
  try {
    const sorobanMetaApi =
      typeof require !== 'undefined'
        ? (function () {
            try {
              return require('./sorobanMeta');
            } catch {
              return null;
            }
          })()
        : typeof SkyMoonScopeSorobanMeta !== 'undefined'
        ? SkyMoonScopeSorobanMeta
        : null;
    if (sorobanMetaApi && typeof sorobanMetaApi.decodeSorobanMeta === 'function') {
      const decoded = sorobanMetaApi.decodeSorobanMeta(bytes);
      if (
        decoded.contractMeta.length > 0 ||
        decoded.specFunctions.length > 0 ||
        decoded.specTypes.length > 0
      ) {
        sorobanMeta = decoded;
      }
    }
  } catch {
    // non-fatal
  }

  return {
    valid: true,
    byteLength: bytes.length,
    version,
    sections: sections.map(({ id, name, size }) => ({ id, name, size })),
    exports,
    imports,
    dataSegments,
    sorobanMeta,
    errors,
    warnings,
    durationMs: Date.now() - started,
  };
}

/** Contract-facing summary: the exported functions a user can invoke. */
function extractContractFunctions(report) {
  if (!report || !Array.isArray(report.exports)) return [];
  return report.exports
    .filter((entry) => entry.kind === 'function')
    .map((entry) => entry.name)
    .filter((name) => name.length > 0 && !name.startsWith('_'))
    .sort((a, b) => a.localeCompare(b));
}

  return {
    WASM_MAGIC,
    WASM_VERSION,
    SECTION_NAMES,
    WasmDecodeError,
    hasWasmMagic,
    readVarUint32,
    parseWasmSections,
    parseDataSection,
    validateWasmModule,
    extractContractFunctions,
  };
});
