//! Decode Soroban metadata from WASM custom sections.
//!
//! Soroban contracts compiled with the official SDK embed two well-known custom
//! sections in the WASM binary:
//!
//! | Section name        | Contents                                              |
//! |---------------------|-------------------------------------------------------|
//! | `contractspecv0`    | XDR-encoded `SCSpecEntry` vector — the ABI           |
//! | `contractmetav0`    | XDR-encoded `SCMetaEntry` vector — build metadata    |
//!
//! Both sections are identified by their UTF-8 name (WASM custom section id = 0)
//! and contain raw XDR bytes with **no length prefix** — just the concatenation
//! of the XDR-serialised entries.
//!
//! # Example
//!
//! ```
//! use sky_moon_scope_core::soroban_meta_decoder::SorobanMetaDecoder;
//!
//! let wasm: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
//! let decoded = SorobanMetaDecoder::decode(wasm);
//! println!("{:?}", decoded.contract_meta);
//! ```

use serde::{Deserialize, Serialize};
use soroban_sdk::xdr::{
    Limits, ReadXdr, ScMetaEntry, ScMetaV0, ScSpecEntry,
};
use thiserror::Error;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can arise while decoding Soroban WASM custom sections.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum MetaDecodeError {
    /// The binary does not start with the WASM magic number.
    #[error("not a valid WASM module: missing magic number")]
    NotWasm,

    /// A custom section name claims more bytes than the section contains.
    #[error("truncated custom section at offset {offset}")]
    TruncatedSection { offset: usize },

    /// XDR deserialisation of a section payload failed.
    #[error("XDR decode error in section `{section}`: {detail}")]
    XdrError { section: String, detail: String },
}

// ── Output types ──────────────────────────────────────────────────────────────

/// One decoded entry from the `contractmetav0` custom section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractMetaEntry {
    /// UTF-8 key identifying the metadata field (e.g. `"rssdkver"`).
    pub key: String,
    /// UTF-8 value for this field (e.g. `"22.0.0"`).
    pub value: String,
}

/// Argument descriptor extracted from a `contractspecv0` function entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFunctionArg {
    /// Argument name as declared in the contract source.
    pub name: String,
    /// XDR debug representation of the `ScSpecTypeDef`.
    pub type_def: String,
}

/// A decoded function entry from the `contractspecv0` ABI section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFunctionEntry {
    /// Contract entry point name — matches the exported WASM function name.
    pub name: String,
    /// Human-readable documentation comment, if present.
    pub doc: String,
    /// Ordered list of input arguments.
    pub inputs: Vec<SpecFunctionArg>,
    /// XDR debug representations of the return type(s).
    pub outputs: Vec<String>,
}

/// A decoded type (struct, enum, union, or error enum) from `contractspecv0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecTypeEntry {
    /// `"struct"` | `"union"` | `"enum"` | `"error_enum"`
    pub kind: String,
    /// Name of the type as declared in the contract source.
    pub name: String,
    /// Human-readable documentation comment, if present.
    pub doc: String,
}

/// The full decoded result for both Soroban custom sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SorobanMetaReport {
    /// Build metadata decoded from `contractmetav0`.
    pub contract_meta: Vec<ContractMetaEntry>,
    /// Function ABI entries decoded from `contractspecv0`.
    pub spec_functions: Vec<SpecFunctionEntry>,
    /// Custom type definitions decoded from `contractspecv0`.
    pub spec_types: Vec<SpecTypeEntry>,
    /// Non-fatal warnings raised during decoding.
    pub warnings: Vec<String>,
    /// Errors encountered while decoding.  A partial result is still returned.
    pub errors: Vec<MetaDecodeError>,
}

impl SorobanMetaReport {
    /// Returns `true` when both sections decoded without hard errors.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

// ── Decoder ───────────────────────────────────────────────────────────────────

/// Decoder for Soroban contract metadata stored in WASM custom sections.
pub struct SorobanMetaDecoder;

const SECTION_CONTRACT_META: &str = "contractmetav0";
const SECTION_CONTRACT_SPEC: &str = "contractspecv0";
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

impl SorobanMetaDecoder {
    /// Decode Soroban metadata from `wasm_bytes`.  Never panics.
    pub fn decode(wasm_bytes: &[u8]) -> SorobanMetaReport {
        let mut report = SorobanMetaReport {
            contract_meta: Vec::new(),
            spec_functions: Vec::new(),
            spec_types: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        if wasm_bytes.len() < 8 || wasm_bytes[..4] != WASM_MAGIC {
            report.errors.push(MetaDecodeError::NotWasm);
            return report;
        }

        let mut cursor = 8usize;
        while cursor < wasm_bytes.len() {
            let section_id = wasm_bytes[cursor];
            cursor += 1;

            let (section_size, leb_len) = match read_leb128_u32(wasm_bytes, cursor) {
                Some(v) => v,
                None => {
                    report.warnings.push(format!(
                        "truncated LEB128 at offset {cursor}; stopping section walk"
                    ));
                    break;
                }
            };
            cursor += leb_len;

            let section_end = cursor + section_size as usize;
            if section_end > wasm_bytes.len() {
                report.warnings.push(format!(
                    "section at offset {cursor} claims {section_size} bytes but only {} remain; stopping",
                    wasm_bytes.len() - cursor
                ));
                break;
            }

            if section_id == 0 {
                let section_bytes = &wasm_bytes[cursor..section_end];
                match read_custom_section_name(section_bytes) {
                    Err(e) => report.errors.push(e),
                    Ok((name, payload_start)) => {
                        let payload = &section_bytes[payload_start..];
                        match name.as_str() {
                            SECTION_CONTRACT_META => Self::decode_meta_section(payload, &mut report),
                            SECTION_CONTRACT_SPEC => Self::decode_spec_section(payload, &mut report),
                            _ => {}
                        }
                    }
                }
            }

            cursor = section_end;
        }

        report
    }

    fn decode_meta_section(payload: &[u8], report: &mut SorobanMetaReport) {
        let mut cursor = std::io::Cursor::new(payload);
        loop {
            if cursor.position() as usize >= payload.len() {
                break;
            }
            match ScMetaEntry::read_xdr(&mut cursor, Limits::none()) {
                Ok(entry) => {
                    if let ScMetaEntry::ScMetaV0(ScMetaV0 { key, val }) = entry {
                        report.contract_meta.push(ContractMetaEntry {
                            key: String::from_utf8_lossy(key.as_ref()).into_owned(),
                            value: String::from_utf8_lossy(val.as_ref()).into_owned(),
                        });
                    } else {
                        report.warnings.push(
                            "contractmetav0: unknown ScMetaEntry variant".to_string(),
                        );
                    }
                }
                Err(e) => {
                    report.errors.push(MetaDecodeError::XdrError {
                        section: SECTION_CONTRACT_META.to_string(),
                        detail: e.to_string(),
                    });
                    break;
                }
            }
        }
    }

    fn decode_spec_section(payload: &[u8], report: &mut SorobanMetaReport) {
        let mut cursor = std::io::Cursor::new(payload);
        loop {
            if cursor.position() as usize >= payload.len() {
                break;
            }
            match ScSpecEntry::read_xdr(&mut cursor, Limits::none()) {
                Ok(entry) => Self::dispatch_spec_entry(entry, report),
                Err(e) => {
                    report.errors.push(MetaDecodeError::XdrError {
                        section: SECTION_CONTRACT_SPEC.to_string(),
                        detail: e.to_string(),
                    });
                    break;
                }
            }
        }
    }

    fn dispatch_spec_entry(entry: ScSpecEntry, report: &mut SorobanMetaReport) {
        use soroban_sdk::xdr::ScSpecEntry::*;
        match entry {
            FunctionV0(f) => {
                report.spec_functions.push(SpecFunctionEntry {
                    name: String::from_utf8_lossy(f.name.as_ref()).into_owned(),
                    doc: String::from_utf8_lossy(f.doc.as_ref()).into_owned(),
                    inputs: f
                        .inputs
                        .iter()
                        .map(|a| SpecFunctionArg {
                            name: String::from_utf8_lossy(a.name.as_ref()).into_owned(),
                            type_def: format!("{:?}", a.type_),
                        })
                        .collect(),
                    outputs: f.outputs.iter().map(|t| format!("{t:?}")).collect(),
                });
            }
            UdtStructV0(s) => {
                report.spec_types.push(SpecTypeEntry {
                    kind: "struct".to_string(),
                    name: String::from_utf8_lossy(s.name.as_ref()).into_owned(),
                    doc: String::from_utf8_lossy(s.doc.as_ref()).into_owned(),
                });
            }
            UdtUnionV0(u) => {
                report.spec_types.push(SpecTypeEntry {
                    kind: "union".to_string(),
                    name: String::from_utf8_lossy(u.name.as_ref()).into_owned(),
                    doc: String::from_utf8_lossy(u.doc.as_ref()).into_owned(),
                });
            }
            UdtEnumV0(e) => {
                report.spec_types.push(SpecTypeEntry {
                    kind: "enum".to_string(),
                    name: String::from_utf8_lossy(e.name.as_ref()).into_owned(),
                    doc: String::from_utf8_lossy(e.doc.as_ref()).into_owned(),
                });
            }
            UdtErrorEnumV0(e) => {
                report.spec_types.push(SpecTypeEntry {
                    kind: "error_enum".to_string(),
                    name: String::from_utf8_lossy(e.name.as_ref()).into_owned(),
                    doc: String::from_utf8_lossy(e.doc.as_ref()).into_owned(),
                });
            }
        }
    }
}

// ── LEB128 helpers ────────────────────────────────────────────────────────────

/// Decode an unsigned 32-bit LEB128 integer at `bytes[offset..]`.
/// Returns `(value, bytes_consumed)` or `None` on truncation / overflow.
pub fn read_leb128_u32(bytes: &[u8], mut offset: usize) -> Option<(u32, usize)> {
    let start = offset;
    let mut result = 0u32;
    let mut shift = 0u32;

    loop {
        if offset >= bytes.len() {
            return None;
        }
        let byte = bytes[offset];
        offset += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }

    Some((result, offset - start))
}

/// Extract the name from the front of a custom section payload.
/// Layout: `[name_len: LEB128][name_bytes: utf8][payload...]`
/// Returns `(name, payload_start_offset_within_section_bytes)`.
pub fn read_custom_section_name(
    section_bytes: &[u8],
) -> Result<(String, usize), MetaDecodeError> {
    let (name_len, leb_len) = read_leb128_u32(section_bytes, 0)
        .ok_or(MetaDecodeError::TruncatedSection { offset: 0 })?;

    let name_start = leb_len;
    let name_end = name_start + name_len as usize;

    if name_end > section_bytes.len() {
        return Err(MetaDecodeError::TruncatedSection {
            offset: name_start,
        });
    }

    let name =
        String::from_utf8_lossy(&section_bytes[name_start..name_end]).into_owned();
    Ok((name, name_end))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::xdr::{
        Limits, ScMetaEntry, ScMetaV0, ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0,
        ScSpecTypeDef, StringM, VecM, WriteXdr,
    };

    fn leb128_encode(mut value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 { byte |= 0x80; }
            out.push(byte);
            if value == 0 { break; }
        }
        out
    }

    fn module_with_custom_section(name: &str, payload: &[u8]) -> Vec<u8> {
        let name_bytes = name.as_bytes();
        let name_leb = leb128_encode(name_bytes.len() as u32);
        let section_body: Vec<u8> = [name_leb.as_slice(), name_bytes, payload].concat();
        let size_leb = leb128_encode(section_body.len() as u32);
        let mut wasm = vec![0x00u8, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        wasm.push(0x00);
        wasm.extend_from_slice(&size_leb);
        wasm.extend_from_slice(&section_body);
        wasm
    }

    fn encode_meta_entries(entries: &[ScMetaEntry]) -> Vec<u8> {
        entries
            .iter()
            .flat_map(|e| e.to_xdr(Limits::none()).unwrap())
            .collect()
    }

    fn encode_spec_entries(entries: &[ScSpecEntry]) -> Vec<u8> {
        entries
            .iter()
            .flat_map(|e| e.to_xdr(Limits::none()).unwrap())
            .collect()
    }

    fn string_m(s: &str) -> StringM {
        s.as_bytes().to_vec().try_into().expect("string too long")
    }

    #[test]
    fn decode_rejects_non_wasm_input() {
        let report = SorobanMetaDecoder::decode(b"this is not wasm");
        assert!(!report.errors.is_empty());
        assert!(matches!(report.errors[0], MetaDecodeError::NotWasm));
    }

    #[test]
    fn decode_empty_module_returns_clean_report() {
        let wasm = [0x00u8, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.contract_meta.is_empty());
        assert!(report.spec_functions.is_empty());
        assert!(report.spec_types.is_empty());
        assert!(report.is_clean());
    }

    #[test]
    fn decode_contractmetav0_single_entry() {
        let payload = encode_meta_entries(&[ScMetaEntry::ScMetaV0(ScMetaV0 {
            key: string_m("rssdkver"),
            val: string_m("22.0.0"),
        })]);
        let wasm = module_with_custom_section("contractmetav0", &payload);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.contract_meta.len(), 1);
        assert_eq!(report.contract_meta[0].key, "rssdkver");
        assert_eq!(report.contract_meta[0].value, "22.0.0");
    }

    #[test]
    fn decode_contractmetav0_multiple_entries() {
        let payload = encode_meta_entries(&[
            ScMetaEntry::ScMetaV0(ScMetaV0 { key: string_m("rsver"),    val: string_m("1.78.0") }),
            ScMetaEntry::ScMetaV0(ScMetaV0 { key: string_m("rssdkver"), val: string_m("22.0.0") }),
        ]);
        let wasm = module_with_custom_section("contractmetav0", &payload);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.contract_meta.len(), 2);
        assert_eq!(report.contract_meta[0].key, "rsver");
        assert_eq!(report.contract_meta[1].key, "rssdkver");
    }

    #[test]
    fn decode_contractmetav0_empty_payload_is_clean() {
        let wasm = module_with_custom_section("contractmetav0", &[]);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty());
        assert!(report.contract_meta.is_empty());
    }

    #[test]
    fn decode_contractspecv0_function_entry() {
        let inputs: VecM<ScSpecFunctionInputV0> = vec![ScSpecFunctionInputV0 {
            doc: string_m(""),
            name: string_m("amount"),
            type_: ScSpecTypeDef::U64,
        }]
        .try_into()
        .unwrap();
        let outputs: VecM<ScSpecTypeDef> = vec![ScSpecTypeDef::Bool].try_into().unwrap();
        let payload = encode_spec_entries(&[ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
            doc: string_m("Transfer tokens"),
            name: string_m("transfer"),
            inputs,
            outputs,
        })]);
        let wasm = module_with_custom_section("contractspecv0", &payload);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.spec_functions.len(), 1);
        let f = &report.spec_functions[0];
        assert_eq!(f.name, "transfer");
        assert_eq!(f.doc, "Transfer tokens");
        assert_eq!(f.inputs.len(), 1);
        assert_eq!(f.inputs[0].name, "amount");
        assert!(f.inputs[0].type_def.contains("U64"));
        assert_eq!(f.outputs.len(), 1);
        assert!(f.outputs[0].contains("Bool"));
    }

    #[test]
    fn decode_contractspecv0_struct_type_entry() {
        use soroban_sdk::xdr::{ScSpecUdtStructFieldV0, ScSpecUdtStructV0};
        let fields: VecM<ScSpecUdtStructFieldV0> = vec![].try_into().unwrap();
        let payload = encode_spec_entries(&[ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            doc: string_m("A config struct"),
            lib: string_m(""),
            name: string_m("Config"),
            fields,
        })]);
        let wasm = module_with_custom_section("contractspecv0", &payload);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.spec_types.len(), 1);
        assert_eq!(report.spec_types[0].kind, "struct");
        assert_eq!(report.spec_types[0].name, "Config");
        assert_eq!(report.spec_types[0].doc, "A config struct");
    }

    #[test]
    fn decode_contractspecv0_empty_payload_is_clean() {
        let wasm = module_with_custom_section("contractspecv0", &[]);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty());
        assert!(report.spec_functions.is_empty());
        assert!(report.spec_types.is_empty());
    }

    #[test]
    fn decode_contractspecv0_corrupt_payload_records_error() {
        let wasm = module_with_custom_section("contractspecv0", &[0xDE, 0xAD, 0xBE, 0xEF]);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(
            &report.errors[0],
            MetaDecodeError::XdrError { section, .. } if section == "contractspecv0"
        ));
    }

    #[test]
    fn decode_ignores_unknown_custom_sections() {
        let wasm = module_with_custom_section("name", &[0x00]);
        let report = SorobanMetaDecoder::decode(&wasm);
        assert!(report.errors.is_empty());
        assert!(report.contract_meta.is_empty());
        assert!(report.spec_functions.is_empty());
    }

    #[test]
    fn soroban_meta_report_serialises_to_json() {
        let wasm = [0x00u8, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let report = SorobanMetaDecoder::decode(&wasm);
        let json = serde_json::to_string(&report).expect("report must serialise");
        assert!(json.contains("contract_meta"));
        assert!(json.contains("spec_functions"));
    }

    #[test]
    fn leb128_decodes_single_byte() {
        assert_eq!(read_leb128_u32(&[0x00], 0), Some((0, 1)));
        assert_eq!(read_leb128_u32(&[0x7f], 0), Some((127, 1)));
    }

    #[test]
    fn leb128_decodes_multi_byte() {
        assert_eq!(read_leb128_u32(&[0x80, 0x01], 0), Some((128, 2)));
        assert_eq!(read_leb128_u32(&[0xe5, 0x8e, 0x26], 0), Some((624485, 3)));
    }

    #[test]
    fn leb128_returns_none_on_truncated_buffer() {
        assert_eq!(read_leb128_u32(&[0x80], 0), None);
        assert_eq!(read_leb128_u32(&[], 0), None);
    }

    #[test]
    fn custom_section_name_extraction() {
        let section = b"\x04name\xff\xfe";
        let (name, offset) = read_custom_section_name(section).unwrap();
        assert_eq!(name, "name");
        assert_eq!(offset, 5);
        assert_eq!(section[offset], 0xff);
    }

    #[test]
    fn custom_section_name_truncated_returns_error() {
        let section = &[0x0a, b'x', b'y', b'z'];
        assert!(read_custom_section_name(section).is_err());
    }
}
