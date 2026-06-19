// fusa:req REQ-CG-001
// fusa:req REQ-CG-002
// fusa:req REQ-CG-003
// fusa:req REQ-CG-004

//! Stub-code generator — emits Rust type definitions from a JSON schema.
//!
//! Used in CI to keep generated zone-controller stubs in sync with the spec.

use std::collections::HashMap;

// ── Schema types ──────────────────────────────────────────────────────────────

/// Simple field types supported by the code generator.
// fusa:req REQ-CG-001
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    U8,
    U16,
    U32,
    U64,
    Bool,
    String,
    Bytes,
}

impl FieldType {
    pub fn rust_type(&self) -> &'static str {
        match self {
            FieldType::U8 => "u8",
            FieldType::U16 => "u16",
            FieldType::U32 => "u32",
            FieldType::U64 => "u64",
            FieldType::Bool => "bool",
            FieldType::String => "String",
            FieldType::Bytes => "Vec<u8>",
        }
    }
}

/// A named field in a generated struct.
// fusa:req REQ-CG-002
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ftype: FieldType,
    pub optional: bool,
}

/// A schema for a generated struct.
// fusa:req REQ-CG-002
#[derive(Debug, Clone)]
pub struct StructSchema {
    pub name: String,
    pub fields: Vec<Field>,
}

// ── Code generator ────────────────────────────────────────────────────────────

/// Generates Rust struct definitions from a list of schemas.
// fusa:req REQ-CG-003
pub fn generate_structs(schemas: &[StructSchema]) -> String {
    let mut out = String::new();
    for schema in schemas {
        out.push_str(&format!(
            "#[derive(Debug, Clone, Default)]\npub struct {} {{\n",
            schema.name
        ));
        for field in &schema.fields {
            let ty = field.ftype.rust_type();
            if field.optional {
                out.push_str(&format!("    pub {}: Option<{}>,\n", field.name, ty));
            } else {
                out.push_str(&format!("    pub {}: {},\n", field.name, ty));
            }
        }
        out.push_str("}\n\n");
    }
    out
}

/// Parse a simple JSON-like schema definition map into [`StructSchema`] list.
///
/// Accepts: `{"StructName": {"field": "type", "opt_field?": "type"}}`
// fusa:req REQ-CG-004
pub fn parse_schema(map: &HashMap<String, HashMap<String, String>>) -> Vec<StructSchema> {
    let mut schemas = Vec::new();
    let mut names: Vec<&String> = map.keys().collect();
    names.sort();
    for name in names {
        let fields_map = &map[name];
        let mut field_names: Vec<&String> = fields_map.keys().collect();
        field_names.sort();
        let fields = field_names
            .iter()
            .map(|fname| {
                let optional = fname.ends_with('?');
                let clean_name = if optional {
                    &fname[..fname.len() - 1]
                } else {
                    fname.as_str()
                };
                let ftype = match fields_map[*fname].as_str() {
                    "u8" => FieldType::U8,
                    "u16" => FieldType::U16,
                    "u32" => FieldType::U32,
                    "u64" => FieldType::U64,
                    "bool" => FieldType::Bool,
                    "bytes" => FieldType::Bytes,
                    _ => FieldType::String,
                };
                Field {
                    name: clean_name.to_string(),
                    ftype,
                    optional,
                }
            })
            .collect();
        schemas.push(StructSchema {
            name: name.clone(),
            fields,
        });
    }
    schemas
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    // fusa:test REQ-CG-001
    fn field_type_rust_names() {
        assert_eq!(FieldType::U8.rust_type(), "u8");
        assert_eq!(FieldType::Bool.rust_type(), "bool");
        assert_eq!(FieldType::Bytes.rust_type(), "Vec<u8>");
    }

    #[test]
    // fusa:test REQ-CG-003
    fn generate_empty_struct() {
        let schema = StructSchema {
            name: "Empty".into(),
            fields: vec![],
        };
        let src = generate_structs(&[schema]);
        assert!(src.contains("pub struct Empty {"));
    }

    #[test]
    // fusa:test REQ-CG-002
    // fusa:test REQ-CG-003
    fn generate_struct_with_optional_field() {
        let schema = StructSchema {
            name: "Cmd".into(),
            fields: vec![
                Field {
                    name: "id".into(),
                    ftype: FieldType::U32,
                    optional: false,
                },
                Field {
                    name: "payload".into(),
                    ftype: FieldType::Bytes,
                    optional: true,
                },
            ],
        };
        let src = generate_structs(&[schema]);
        assert!(src.contains("pub id: u32"));
        assert!(src.contains("pub payload: Option<Vec<u8>>"));
    }

    #[test]
    // fusa:test REQ-CG-004
    fn parse_schema_roundtrip() {
        let mut map = HashMap::new();
        let mut fields = HashMap::new();
        fields.insert("zone".to_string(), "u8".to_string());
        fields.insert("name?".to_string(), "string".to_string());
        map.insert("ZoneConfig".to_string(), fields);
        let schemas = parse_schema(&map);
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "ZoneConfig");
        let opt = schemas[0].fields.iter().find(|f| f.name == "name").unwrap();
        assert!(opt.optional);
        let src = generate_structs(&schemas);
        assert!(src.contains("pub zone: u8"));
        assert!(src.contains("pub name: Option<String>"));
    }
}
