/**
 * sorobanMeta.js — decode Soroban-specific WASM custom sections.
 *
 * Parses the two well-known Soroban custom sections from a WASM binary:
 *
 *   contractmetav0  — XDR ScMetaEntry list (build metadata: SDK version, etc.)
 *   contractspecv0  — XDR ScSpecEntry list (ABI: functions, structs, enums)
 *
 * XDR layout for both sections is a **raw concatenation** of fixed-format XDR
 * records — no framing length prefix — immediately following the custom section
 * name in the WASM binary.
 *
 * This module is intentionally dependency-free and uses the same UMD pattern as
 * wasmValidation.js so the Web Worker can pull it in with importScripts().
 *
 * References:
 *   https://developers.stellar.org/docs/learn/smart-contract-internals/contract-metadata
 *   stellar/rs-soroban-sdk  (contractspecv0 / contractmetav0 XDR definitions)
 */
(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  } else {
    root.SkyMoonScopeSorobanMeta = api;
  }
})(typeof self !== 'undefined' ? self : globalThis, function () {
  'use strict';

  // ── Constants ───────────────────────────────────────────────────────────────

  const WASM_MAGIC = [0x00, 0x61, 0x73, 0x6d];

  /**
   * XDR discriminant constants for ScMetaEntry and ScSpecEntry.
   *
   * These match the Stellar XDR definitions; the discriminant is a big-endian
   * 4-byte signed integer at the front of each XDR union.
   */
  const XDR_META_V0 = 0; // ScMetaEntry::ScMetaV0

  const XDR_SPEC_FUNCTION_V0   = 0; // ScSpecEntry::FunctionV0
  const XDR_SPEC_UDT_STRUCT_V0 = 1; // ScSpecEntry::UdtStructV0
  const XDR_SPEC_UDT_UNION_V0  = 2; // ScSpecEntry::UdtUnionV0
  const XDR_SPEC_UDT_ENUM_V0   = 3; // ScSpecEntry::UdtEnumV0
  const XDR_SPEC_UDT_ERROR_V0  = 4; // ScSpecEntry::UdtErrorEnumV0

  /**
   * ScSpecTypeDef discriminant → human-readable name.
   *
   * Source: Stellar XDR (ScSpecType enum).
   */
  const SPEC_TYPE_NAMES = {
    0:  'Val',
    1:  'Bool',
    2:  'Void',
    3:  'Error',
    4:  'U32',
    5:  'I32',
    6:  'U64',
    7:  'I64',
    8:  'Timepoint',
    9:  'Duration',
    10: 'U128',
    11: 'I128',
    12: 'U256',
    13: 'I256',
    14: 'Bytes',
    15: 'String',
    16: 'Symbol',
    17: 'Address',
    1000: 'Option',
    1001: 'Result',
    1002: 'Vec',
    1003: 'Map',
    1004: 'Tuple',
    1005: 'BytesN',
    2000: 'Udt',
  };

  // ── XDR primitives ─────────────────────────────────────────────────────────

  /**
   * Read a big-endian 4-byte signed integer from `bytes` at `offset`.
   * Returns { value, next }.
   */
  function readInt32BE(bytes, offset) {
    if (offset + 4 > bytes.length) {
      throw new Error(`readInt32BE: truncated at offset ${offset}`);
    }
    const value =
      ((bytes[offset]     << 24) |
       (bytes[offset + 1] << 16) |
       (bytes[offset + 2] <<  8) |
        bytes[offset + 3]) | 0;
    return { value, next: offset + 4 };
  }

  /**
   * Read a big-endian 4-byte unsigned integer.
   */
  function readUint32BE(bytes, offset) {
    const { value, next } = readInt32BE(bytes, offset);
    return { value: value >>> 0, next };
  }

  /**
   * Read an XDR opaque<> / string<> field: 4-byte big-endian length, then that
   * many bytes, then 0-3 padding bytes to align to a 4-byte boundary.
   * Returns { value: Uint8Array, next }.
   */
  function readXdrBytes(bytes, offset) {
    const { value: length, next: dataStart } = readUint32BE(bytes, offset);
    const dataEnd = dataStart + length;
    if (dataEnd > bytes.length) {
      throw new Error(
        `readXdrBytes: claims ${length} bytes at offset ${dataStart} but only ${bytes.length - dataStart} remain`,
      );
    }
    const value = bytes.subarray(dataStart, dataEnd);
    // XDR pads to 4-byte boundary.
    const paddedEnd = dataEnd + ((4 - (length & 3)) & 3);
    return { value, next: Math.min(paddedEnd, bytes.length) };
  }

  /**
   * Read an XDR string<> and decode it as UTF-8.
   */
  function readXdrString(bytes, offset) {
    const { value: raw, next } = readXdrBytes(bytes, offset);
    const text =
      typeof TextDecoder !== 'undefined'
        ? new TextDecoder('utf-8').decode(raw)
        : String.fromCharCode(...raw);
    return { value: text, next };
  }

  // ── WASM section walker ────────────────────────────────────────────────────

  /** Read an unsigned LEB128 integer.  Returns { value, next } or null. */
  function readLEB128(bytes, offset) {
    let result = 0;
    let shift = 0;
    let cursor = offset;
    while (cursor < bytes.length) {
      const byte = bytes[cursor++];
      result |= (byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) return { value: result >>> 0, next: cursor };
      shift += 7;
      if (shift > 28) return null; // overflow
    }
    return null; // truncated
  }

  /**
   * Walk WASM custom sections (section id = 0) and yield entries for those
   * whose name matches the predicate.
   *
   * @param {Uint8Array} bytes
   * @param {(name: string) => boolean} namePredicate
   * @returns {{ name: string, payload: Uint8Array }[]}
   */
  function findCustomSections(bytes, namePredicate) {
    const found = [];
    if (bytes.length < 8) return found;
    // Verify magic.
    if (
      bytes[0] !== 0x00 || bytes[1] !== 0x61 ||
      bytes[2] !== 0x73 || bytes[3] !== 0x6d
    ) {
      return found;
    }

    let cursor = 8; // skip 4-byte magic + 4-byte version
    while (cursor < bytes.length) {
      const sectionId = bytes[cursor++];
      const sizeResult = readLEB128(bytes, cursor);
      if (!sizeResult) break;
      cursor = sizeResult.next;
      const sectionEnd = cursor + sizeResult.value;
      if (sectionEnd > bytes.length) break;

      if (sectionId === 0) {
        // Custom section: read name
        const nameLenResult = readLEB128(bytes, cursor);
        if (nameLenResult) {
          const nameStart = nameLenResult.next;
          const nameEnd   = nameStart + nameLenResult.value;
          if (nameEnd <= sectionEnd) {
            const nameBytes = bytes.subarray(nameStart, nameEnd);
            const name =
              typeof TextDecoder !== 'undefined'
                ? new TextDecoder('utf-8').decode(nameBytes)
                : String.fromCharCode(...nameBytes);
            if (namePredicate(name)) {
              found.push({ name, payload: bytes.subarray(nameEnd, sectionEnd) });
            }
          }
        }
      }

      cursor = sectionEnd;
    }

    return found;
  }

  // ── contractmetav0 decoder ─────────────────────────────────────────────────

  /**
   * Decode a single `ScMetaEntry` (contractmetav0) from `bytes` at `offset`.
   * Returns { entry: { key, value }, next } or throws.
   */
  function decodeMetaEntry(bytes, offset) {
    // Discriminant: 4-byte big-endian int
    const { value: discriminant, next: afterDisc } = readInt32BE(bytes, offset);
    if (discriminant !== XDR_META_V0) {
      throw new Error(`Unknown ScMetaEntry discriminant: ${discriminant}`);
    }
    const { value: key,   next: afterKey }   = readXdrString(bytes, afterDisc);
    const { value: value, next: afterValue }  = readXdrString(bytes, afterKey);
    return { entry: { key, value }, next: afterValue };
  }

  /**
   * Decode all `ScMetaEntry` records from raw payload bytes.
   *
   * @param {Uint8Array} payload
   * @returns {{ entries: { key: string, value: string }[], errors: string[] }}
   */
  function decodeMetaSection(payload) {
    const entries = [];
    const errors  = [];
    let offset = 0;

    while (offset < payload.length) {
      try {
        const { entry, next } = decodeMetaEntry(payload, offset);
        entries.push(entry);
        offset = next;
      } catch (e) {
        errors.push(`contractmetav0 at offset ${offset}: ${e.message}`);
        break;
      }
    }

    return { entries, errors };
  }

  // ── contractspecv0 decoder ─────────────────────────────────────────────────

  /**
   * Decode a `ScSpecTypeDef` from `bytes` at `offset`.
   * Returns { typeName: string, next: number }.
   *
   * Complex types (Option, Vec, Map, Tuple, BytesN, Result, Udt) carry a nested
   * payload; we decode enough to form a readable label without fully recursing.
   */
  function decodeSpecTypeDef(bytes, offset) {
    const { value: typeCode, next } = readUint32BE(bytes, offset);
    const baseName = SPEC_TYPE_NAMES[typeCode] || `unknown(${typeCode})`;

    // Simple (non-compound) types carry no additional bytes.
    if (typeCode < 1000) {
      return { typeName: baseName, next };
    }

    // Compound types — skip their inner fields; produce a readable label.
    switch (typeCode) {
      case 1000: { // Option<T>
        const inner = decodeSpecTypeDef(bytes, next);
        return { typeName: `Option<${inner.typeName}>`, next: inner.next };
      }
      case 1001: { // Result<T, E>
        const ok  = decodeSpecTypeDef(bytes, next);
        const err = decodeSpecTypeDef(bytes, ok.next);
        return { typeName: `Result<${ok.typeName}, ${err.typeName}>`, next: err.next };
      }
      case 1002: { // Vec<T>
        const inner = decodeSpecTypeDef(bytes, next);
        return { typeName: `Vec<${inner.typeName}>`, next: inner.next };
      }
      case 1003: { // Map<K, V>
        const k = decodeSpecTypeDef(bytes, next);
        const v = decodeSpecTypeDef(bytes, k.next);
        return { typeName: `Map<${k.typeName}, ${v.typeName}>`, next: v.next };
      }
      case 1004: { // Tuple<T...>
        const { value: count, next: afterCount } = readUint32BE(bytes, next);
        let cur = afterCount;
        const parts = [];
        for (let i = 0; i < count && cur < bytes.length; i++) {
          const t = decodeSpecTypeDef(bytes, cur);
          parts.push(t.typeName);
          cur = t.next;
        }
        return { typeName: `Tuple<${parts.join(', ')}>`, next: cur };
      }
      case 1005: { // BytesN<N>
        const { value: n, next: afterN } = readUint32BE(bytes, next);
        return { typeName: `BytesN<${n}>`, next: afterN };
      }
      case 2000: { // Udt (user-defined type) — carries a string name
        const { value: udtName, next: afterName } = readXdrString(bytes, next);
        return { typeName: udtName, next: afterName };
      }
      default:
        return { typeName: baseName, next };
    }
  }

  /**
   * Decode a `ScSpecFunctionInputV0` from `bytes` at `offset`.
   * XDR layout: doc:string, name:string, type:ScSpecTypeDef
   */
  function decodeFunctionInput(bytes, offset) {
    const { value: doc,  next: afterDoc  } = readXdrString(bytes, offset);
    const { value: name, next: afterName } = readXdrString(bytes, afterDoc);
    const { typeName,    next: afterType } = decodeSpecTypeDef(bytes, afterName);
    return { input: { doc, name, type: typeName }, next: afterType };
  }

  /**
   * Decode a `ScSpecFunctionV0` from `bytes` at `offset`.
   * XDR layout: doc:string, name:string, inputs:array<FunctionInputV0>, outputs:array<TypeDef>
   */
  function decodeFunctionV0(bytes, offset) {
    const { value: doc,  next: afterDoc  } = readXdrString(bytes, offset);
    const { value: name, next: afterName } = readXdrString(bytes, afterDoc);

    // inputs array: 4-byte count, then count * FunctionInputV0
    const { value: inputCount, next: afterInputCount } = readUint32BE(bytes, afterName);
    let cur = afterInputCount;
    const inputs = [];
    for (let i = 0; i < inputCount && cur < bytes.length; i++) {
      const { input, next } = decodeFunctionInput(bytes, cur);
      inputs.push(input);
      cur = next;
    }

    // outputs array: 4-byte count, then count * ScSpecTypeDef
    const { value: outputCount, next: afterOutputCount } = readUint32BE(bytes, cur);
    cur = afterOutputCount;
    const outputs = [];
    for (let i = 0; i < outputCount && cur < bytes.length; i++) {
      const { typeName, next } = decodeSpecTypeDef(bytes, cur);
      outputs.push(typeName);
      cur = next;
    }

    return { entry: { kind: 'function', name, doc, inputs, outputs }, next: cur };
  }

  /**
   * Decode the name/doc header shared by struct / union / enum / errorEnum types.
   * XDR layout: doc:string, lib:string, name:string  (structs also have fields after)
   */
  function decodeUdtHeader(bytes, offset) {
    const { value: doc,  next: afterDoc  } = readXdrString(bytes, offset);
    const { value: lib,  next: afterLib  } = readXdrString(bytes, afterDoc);   // lib field
    const { value: name, next: afterName } = readXdrString(bytes, afterLib);
    return { doc, lib, name, next: afterName };
  }

  /**
   * Skip a struct field array so we can correctly position after the entry.
   * ScSpecUdtStructFieldV0: doc:string, name:string, type:ScSpecTypeDef
   */
  function skipStructFields(bytes, offset) {
    const { value: count, next: afterCount } = readUint32BE(bytes, offset);
    let cur = afterCount;
    for (let i = 0; i < count && cur < bytes.length; i++) {
      const afterDoc  = readXdrString(bytes, cur);
      const afterName = readXdrString(bytes, afterDoc.next);
      const afterType = decodeSpecTypeDef(bytes, afterName.next);
      cur = afterType.next;
    }
    return cur;
  }

  /**
   * Skip union case array entries.
   * ScSpecUdtUnionCaseTupleV0: doc, name, types[]
   * ScSpecUdtUnionCaseVoidV0:  doc, name
   * Each wrapped in a 4-byte discriminant (0=void, 1=tuple).
   */
  function skipUnionCases(bytes, offset) {
    const { value: count, next: afterCount } = readUint32BE(bytes, offset);
    let cur = afterCount;
    for (let i = 0; i < count && cur < bytes.length; i++) {
      const { value: caseKind, next: afterKind } = readUint32BE(bytes, cur);
      cur = afterKind;
      if (caseKind === 0) {
        // Void case: doc:string, name:string
        cur = readXdrString(bytes, cur).next;
        cur = readXdrString(bytes, cur).next;
      } else if (caseKind === 1) {
        // Tuple case: doc:string, name:string, types[]
        cur = readXdrString(bytes, cur).next;
        cur = readXdrString(bytes, cur).next;
        const { value: typeCount, next: afterTypeCount } = readUint32BE(bytes, cur);
        cur = afterTypeCount;
        for (let j = 0; j < typeCount && cur < bytes.length; j++) {
          cur = decodeSpecTypeDef(bytes, cur).next;
        }
      } else {
        // Unknown case — bail out of the union entirely.
        return bytes.length;
      }
    }
    return cur;
  }

  /**
   * Skip enum cases: ScSpecUdtEnumCaseV0: doc:string, name:string, value:u32
   */
  function skipEnumCases(bytes, offset) {
    const { value: count, next: afterCount } = readUint32BE(bytes, offset);
    let cur = afterCount;
    for (let i = 0; i < count && cur < bytes.length; i++) {
      cur = readXdrString(bytes, cur).next;  // doc
      cur = readXdrString(bytes, cur).next;  // name
      const { next: afterVal } = readUint32BE(bytes, cur);
      cur = afterVal;
    }
    return cur;
  }

  /**
   * Skip error-enum cases: ScSpecUdtErrorEnumCaseV0: doc:string, name:string, value:u32
   * (same layout as regular enum cases)
   */
  const skipErrorEnumCases = skipEnumCases;

  /**
   * Decode a single `ScSpecEntry` record from `bytes` at `offset`.
   * Returns { entry, next } or throws.
   */
  function decodeSpecEntry(bytes, offset) {
    const { value: discriminant, next: afterDisc } = readInt32BE(bytes, offset);

    switch (discriminant) {
      case XDR_SPEC_FUNCTION_V0: {
        return decodeFunctionV0(bytes, afterDisc);
      }
      case XDR_SPEC_UDT_STRUCT_V0: {
        const hdr = decodeUdtHeader(bytes, afterDisc);
        const afterFields = skipStructFields(bytes, hdr.next);
        return {
          entry: { kind: 'struct', name: hdr.name, doc: hdr.doc },
          next: afterFields,
        };
      }
      case XDR_SPEC_UDT_UNION_V0: {
        const hdr = decodeUdtHeader(bytes, afterDisc);
        const afterCases = skipUnionCases(bytes, hdr.next);
        return {
          entry: { kind: 'union', name: hdr.name, doc: hdr.doc },
          next: afterCases,
        };
      }
      case XDR_SPEC_UDT_ENUM_V0: {
        const hdr = decodeUdtHeader(bytes, afterDisc);
        const afterCases = skipEnumCases(bytes, hdr.next);
        return {
          entry: { kind: 'enum', name: hdr.name, doc: hdr.doc },
          next: afterCases,
        };
      }
      case XDR_SPEC_UDT_ERROR_V0: {
        const hdr = decodeUdtHeader(bytes, afterDisc);
        const afterCases = skipErrorEnumCases(bytes, hdr.next);
        return {
          entry: { kind: 'error_enum', name: hdr.name, doc: hdr.doc },
          next: afterCases,
        };
      }
      default:
        throw new Error(`Unknown ScSpecEntry discriminant: ${discriminant}`);
    }
  }

  /**
   * Decode all `ScSpecEntry` records from raw payload bytes.
   *
   * @param {Uint8Array} payload
   * @returns {{ functions: object[], types: object[], errors: string[] }}
   */
  function decodeSpecSection(payload) {
    const functions = [];
    const types     = [];
    const errors    = [];
    let offset = 0;

    while (offset < payload.length) {
      try {
        const { entry, next } = decodeSpecEntry(payload, offset);
        if (next <= offset) {
          errors.push(`contractspecv0: decoder made no progress at offset ${offset}; stopping`);
          break;
        }
        if (entry.kind === 'function') {
          functions.push(entry);
        } else {
          types.push(entry);
        }
        offset = next;
      } catch (e) {
        errors.push(`contractspecv0 at offset ${offset}: ${e.message}`);
        break;
      }
    }

    return { functions, types, errors };
  }

  // ── Public API ─────────────────────────────────────────────────────────────

  /**
   * Decode Soroban metadata from a WASM binary.
   *
   * @param {ArrayBuffer|Uint8Array} input  - Raw WASM bytes
   * @returns {SorobanMetaDecodeResult}
   *
   * @typedef {Object} SorobanMetaDecodeResult
   * @property {Array<{key:string, value:string}>} contractMeta   - Build metadata entries
   * @property {Array<object>}                     specFunctions  - ABI function entries
   * @property {Array<object>}                     specTypes      - ABI type definitions
   * @property {string[]}                          errors         - Decode errors (non-fatal)
   * @property {string[]}                          warnings       - Decode warnings
   */
  function decodeSorobanMeta(input) {
    const result = {
      contractMeta:  [],
      specFunctions: [],
      specTypes:     [],
      errors:        [],
      warnings:      [],
    };

    let bytes;
    try {
      bytes =
        input instanceof Uint8Array
          ? input
          : input instanceof ArrayBuffer
          ? new Uint8Array(input)
          : new Uint8Array(input);
    } catch (e) {
      result.errors.push(`Failed to read input: ${e.message}`);
      return result;
    }

    if (
      bytes.length < 8 ||
      bytes[0] !== 0x00 || bytes[1] !== 0x61 ||
      bytes[2] !== 0x73 || bytes[3] !== 0x6d
    ) {
      result.errors.push('Not a valid WASM module (missing magic number)');
      return result;
    }

    const metaSections = findCustomSections(bytes, (n) => n === 'contractmetav0');
    const specSections = findCustomSections(bytes, (n) => n === 'contractspecv0');

    if (metaSections.length === 0 && specSections.length === 0) {
      result.warnings.push(
        'No Soroban metadata sections found. This WASM may not be a Soroban contract, ' +
        'or it was compiled without metadata (try soroban contract build --release).',
      );
    }

    for (const section of metaSections) {
      const { entries, errors } = decodeMetaSection(section.payload);
      result.contractMeta.push(...entries);
      result.errors.push(...errors);
    }

    for (const section of specSections) {
      const { functions, types, errors } = decodeSpecSection(section.payload);
      result.specFunctions.push(...functions);
      result.specTypes.push(...types);
      result.errors.push(...errors);
    }

    return result;
  }

  return {
    SPEC_TYPE_NAMES,
    findCustomSections,
    decodeMetaSection,
    decodeSpecSection,
    decodeSorobanMeta,
    // Expose lower-level helpers for unit tests.
    _internal: {
      readInt32BE,
      readUint32BE,
      readXdrBytes,
      readXdrString,
      readLEB128,
      decodeSpecTypeDef,
    },
  };
});
