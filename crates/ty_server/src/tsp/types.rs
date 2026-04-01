//! TSP type representations.
//!
//! These types correspond to the TypeScript protocol defined in
//! `typeServerProtocol.ts` and are sent over the wire as JSON.

use lsp_types::{Range, Url};
use serde::{Deserialize, Serialize};

use crate::tsp::requests::Node;

/// A unique identifier for a type within a snapshot.
pub(crate) type TypeId = i64;

// -- Declaration types -------------------------------------------------------

/// Discriminator for the `Declaration` union.
/// Serializes as numeric `0` or `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum DeclarationKind {
    /// Declaration exists in source code with an AST node.
    Regular = 0,
    /// Declaration synthesized by the type checker (no source node).
    Synthesized = 1,
}

impl Serialize for DeclarationKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DeclarationKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Self::Regular),
            1 => Ok(Self::Synthesized),
            n => Err(serde::de::Error::custom(format!(
                "invalid DeclarationKind: {n}"
            ))),
        }
    }
}

/// The category of a declaration.
/// Serializes as numeric `0`–`7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum DeclarationCategory {
    Intrinsic = 0,
    Variable = 1,
    Param = 2,
    TypeParam = 3,
    TypeAlias = 4,
    Function = 5,
    Class = 6,
    Import = 7,
}

impl Serialize for DeclarationCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DeclarationCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Self::Intrinsic),
            1 => Ok(Self::Variable),
            2 => Ok(Self::Param),
            3 => Ok(Self::TypeParam),
            4 => Ok(Self::TypeAlias),
            5 => Ok(Self::Function),
            6 => Ok(Self::Class),
            7 => Ok(Self::Import),
            n => Err(serde::de::Error::custom(format!(
                "invalid DeclarationCategory: {n}"
            ))),
        }
    }
}

/// A declaration in the type system.
///
/// Discriminated by the `kind` field:
/// - `Regular` → `{ kind: 0, category, node, name? }`
/// - `Synthesized` → `{ kind: 1, uri }`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Declaration {
    Regular {
        category: DeclarationCategory,
        node: Node,
        name: Option<String>,
    },
    Synthesized {
        uri: String,
    },
}

impl Declaration {
    /// Create a regular declaration pointing to a source location.
    pub(crate) fn regular(
        category: DeclarationCategory,
        uri: Url,
        range: Range,
        name: Option<String>,
    ) -> Self {
        Self::Regular {
            category,
            node: Node { uri, range },
            name,
        }
    }

    /// Create a synthesized declaration with just a URI.
    pub(crate) fn synthesized(uri: impl Into<String>) -> Self {
        Self::Synthesized { uri: uri.into() }
    }
}

impl Serialize for Declaration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Regular {
                category,
                node,
                name,
            } => {
                let field_count = if name.is_some() { 4 } else { 3 };
                let mut map = serializer.serialize_map(Some(field_count))?;
                map.serialize_entry("kind", &DeclarationKind::Regular)?;
                map.serialize_entry("category", category)?;
                map.serialize_entry("node", node)?;
                if let Some(n) = name {
                    map.serialize_entry("name", n)?;
                }
                map.end()
            }
            Self::Synthesized { uri } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", &DeclarationKind::Synthesized)?;
                map.serialize_entry("uri", uri)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Declaration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| serde::de::Error::missing_field("kind"))?;
        match kind {
            0 => {
                let category: DeclarationCategory =
                    serde_json::from_value(value.get("category").cloned().unwrap_or_default())
                        .map_err(serde::de::Error::custom)?;
                let node: Node = serde_json::from_value(
                    value
                        .get("node")
                        .cloned()
                        .ok_or_else(|| serde::de::Error::missing_field("node"))?,
                )
                .map_err(serde::de::Error::custom)?;
                let name = value.get("name").and_then(|v| v.as_str()).map(String::from);
                Ok(Self::Regular {
                    category,
                    node,
                    name,
                })
            }
            1 => {
                let uri = value
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| serde::de::Error::missing_field("uri"))?
                    .to_string();
                Ok(Self::Synthesized { uri })
            }
            n => Err(serde::de::Error::custom(format!(
                "invalid DeclarationKind: {n}"
            ))),
        }
    }
}

// -- Type kind ---------------------------------------------------------------

/// The kind of a type (discriminator for the Type union).
/// Serializes as `0`–`9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum TypeKind {
    BuiltIn = 0,
    Declared = 1,
    Function = 2,
    Class = 3,
    Union = 4,
    Module = 5,
    TypeVar = 6,
    Overloaded = 7,
    Synthesized = 8,
    TypeReference = 9,
}

impl Serialize for TypeKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for TypeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Self::BuiltIn),
            1 => Ok(Self::Declared),
            2 => Ok(Self::Function),
            3 => Ok(Self::Class),
            4 => Ok(Self::Union),
            5 => Ok(Self::Module),
            6 => Ok(Self::TypeVar),
            7 => Ok(Self::Overloaded),
            8 => Ok(Self::Synthesized),
            9 => Ok(Self::TypeReference),
            n => Err(serde::de::Error::custom(format!("invalid TypeKind: {n}"))),
        }
    }
}

// -- Type flags --------------------------------------------------------------

/// Bitfield flags describing characteristics of a type.
pub(crate) mod type_flags {
    pub(crate) const NONE: u32 = 0;
    /// The type can be instantiated (e.g., a class object itself).
    pub(crate) const INSTANTIABLE: u32 = 1 << 0;
    /// The type represents an instance (as opposed to a class).
    pub(crate) const INSTANCE: u32 = 1 << 1;
    /// An instance of the type can be called like a function.
    pub(crate) const CALLABLE: u32 = 1 << 2;
}

// -- The main Type struct ----------------------------------------------------

/// A type in the TSP protocol.
///
/// `details` is flattened into the parent JSON object during serialization
/// to match the TypeScript protocol's flat type union.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::struct_field_names)]
pub(crate) struct Type {
    /// Unique ID of this type within the snapshot.
    pub id: TypeId,
    /// Discriminator kind.
    pub kind: TypeKind,
    /// Human-readable display string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Bitfield of [`type_flags`].
    #[serde(default)]
    pub flags: u32,
    /// Source declaration for declared types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration: Option<Declaration>,
    /// Type arguments for generic types (inline Type objects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_args: Option<Vec<Type>>,
    /// Kind-specific details (flattened in JSON).
    #[serde(default)]
    pub details: Option<TypeDetails>,
}

impl Serialize for Type {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        // Count required + optional fields
        let mut n = 3; // id, kind, flags
        if self.display.is_some() {
            n += 1;
        }
        if self.declaration.is_some() {
            n += 1;
        }
        if self.type_args.is_some() {
            n += 1;
        }
        n += detail_field_count(self.details.as_ref());

        let mut map = serializer.serialize_map(Some(n))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("kind", &self.kind)?;
        if let Some(ref display) = self.display {
            map.serialize_entry("display", display)?;
        }
        map.serialize_entry("flags", &self.flags)?;
        if let Some(ref decl) = self.declaration {
            map.serialize_entry("declaration", decl)?;
        }
        if let Some(ref args) = self.type_args {
            map.serialize_entry("typeArgs", args)?;
        }

        // Flatten details into the top-level map
        serialize_details(&mut map, self.details.as_ref())?;

        map.end()
    }
}

/// Count how many JSON fields a `TypeDetails` variant contributes.
fn detail_field_count(details: Option<&TypeDetails>) -> usize {
    match details {
        Some(TypeDetails::Synthesized(_)) => 2,
        Some(TypeDetails::Union(_)) => 1,
        Some(TypeDetails::Literal(_)) => 2,
        Some(TypeDetails::Tuple(_)) => 2,
        Some(TypeDetails::TypeReference(_)) => 1,
        Some(TypeDetails::Module(_)) => 1,
        Some(TypeDetails::Class(_)) => 3,
        Some(TypeDetails::Overloaded(_)) => 2,
        Some(TypeDetails::Function(_)) => 3,
        None => 0,
    }
}

/// Serialize `TypeDetails` fields directly into an open map.
fn serialize_details<S: serde::ser::SerializeMap>(
    map: &mut S,
    details: Option<&TypeDetails>,
) -> Result<(), S::Error> {
    match details {
        Some(TypeDetails::Synthesized(d)) => {
            map.serialize_entry("stubContent", &d.stub_content)?;
            map.serialize_entry("metadata", &d.metadata)?;
        }
        Some(TypeDetails::Union(d)) => {
            map.serialize_entry("subTypes", &d.members)?;
        }
        Some(TypeDetails::Literal(d)) => {
            map.serialize_entry("value", &d.value)?;
            map.serialize_entry("literalKind", &d.literal_kind)?;
        }
        Some(TypeDetails::Tuple(d)) => {
            map.serialize_entry("elements", &d.elements)?;
            if let Some(ref unbounded) = d.is_unbounded {
                map.serialize_entry("isUnbounded", unbounded)?;
            }
        }
        Some(TypeDetails::TypeReference(d)) => {
            map.serialize_entry("referencedTypeId", &d.referenced_type_id)?;
        }
        Some(TypeDetails::Module(d)) => {
            map.serialize_entry("moduleName", &d.module_name)?;
        }
        Some(TypeDetails::Class(d)) => {
            map.serialize_entry("qualifiedName", &d.qualified_name)?;
            if let Some(ref module) = d.module {
                map.serialize_entry("module", module)?;
            }
            if let Some(ref args) = d.type_arguments {
                map.serialize_entry("typeArguments", args)?;
            }
        }
        Some(TypeDetails::Overloaded(d)) => {
            map.serialize_entry("overloads", &d.overloads)?;
            if let Some(ref impl_type) = d.implementation {
                map.serialize_entry("implementation", impl_type)?;
            }
        }
        Some(TypeDetails::Function(d)) => {
            if let Some(ref name) = d.name {
                map.serialize_entry("name", name)?;
            }
            if let Some(ref params) = d.parameters {
                map.serialize_entry("parameters", params)?;
            }
            if let Some(ref ret) = d.return_type {
                map.serialize_entry("returnType", ret)?;
            }
        }
        None => {}
    }
    Ok(())
}

impl Type {
    /// Create an Unknown built-in type.
    pub(crate) fn unknown(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Unknown".to_string()),
            flags: type_flags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create an Any built-in type.
    pub(crate) fn any(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Any".to_string()),
            flags: type_flags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a Never built-in type.
    pub(crate) fn never(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Never".to_string()),
            flags: type_flags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a None built-in type.
    pub(crate) fn none(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("None".to_string()),
            flags: type_flags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }
}

// -- TypeDetails variants ----------------------------------------------------

/// Kind-specific details for a `Type`.
///
/// Variant ordering matters for untagged deserialization — variants with
/// required fields must come before variants with all-optional fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum TypeDetails {
    Synthesized(SynthesizedDetails),
    Union(UnionDetails),
    Literal(LiteralDetails),
    Tuple(TupleDetails),
    TypeReference(TypeReferenceDetails),
    Module(ModuleDetails),
    Class(ClassDetails),
    Overloaded(OverloadedDetails),
    /// All fields optional — must be last for untagged deserialization.
    Function(FunctionDetails),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClassDetails {
    /// Fully qualified class name.
    pub qualified_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_arguments: Option<Vec<TypeId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnionDetails {
    /// Inline member types.
    pub members: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverloadedDetails {
    /// The `@overload` signatures.
    pub overloads: Vec<Type>,
    /// The implementation signature (non-`@overload` def).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation: Option<Box<Type>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FunctionDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<FunctionParameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FunctionParameter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<TypeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ParameterKind>,
}

/// The kind of a function parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ParameterKind {
    PositionalOnly,
    PositionalOrKeyword,
    VarPositional,
    KeywordOnly,
    VarKeyword,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiteralDetails {
    pub value: String,
    pub literal_kind: LiteralKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LiteralKind {
    Int,
    Bool,
    Str,
    Bytes,
    EnumMember,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TupleDetails {
    pub elements: Vec<TypeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_unbounded: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TypeReferenceDetails {
    pub referenced_type_id: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleDetails {
    pub module_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SynthesizedDetails {
    /// Python stub content for a type without source code.
    pub stub_content: String,
    pub metadata: SynthesizedMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SynthesizedMetadata {
    /// Module where the synthesized type is defined.
    pub module: ModuleName,
    /// Byte offset into `stub_content` where the primary definition starts.
    pub primary_definition_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleName {
    pub name_parts: Vec<String>,
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    #[test]
    fn type_kind_serializes_as_number() {
        assert_eq!(serde_json::to_value(TypeKind::BuiltIn).unwrap(), 0);
        assert_eq!(serde_json::to_value(TypeKind::Class).unwrap(), 3);
        assert_eq!(serde_json::to_value(TypeKind::TypeReference).unwrap(), 9);
    }

    #[test]
    fn type_kind_roundtrips() {
        for (expected_num, expected_kind) in [
            (0, TypeKind::BuiltIn),
            (1, TypeKind::Declared),
            (2, TypeKind::Function),
            (3, TypeKind::Class),
            (4, TypeKind::Union),
            (5, TypeKind::Module),
            (6, TypeKind::TypeVar),
            (7, TypeKind::Overloaded),
            (8, TypeKind::Synthesized),
            (9, TypeKind::TypeReference),
        ] {
            let json = serde_json::to_value(expected_kind).unwrap();
            assert_eq!(json, expected_num);
            let back: TypeKind = serde_json::from_value(json).unwrap();
            assert_eq!(back, expected_kind);
        }
    }

    #[test]
    fn declaration_kind_serializes_as_number() {
        assert_eq!(serde_json::to_value(DeclarationKind::Regular).unwrap(), 0);
        assert_eq!(
            serde_json::to_value(DeclarationKind::Synthesized).unwrap(),
            1
        );
    }

    #[test]
    fn declaration_category_roundtrips() {
        for (n, cat) in [
            (0, DeclarationCategory::Intrinsic),
            (1, DeclarationCategory::Variable),
            (2, DeclarationCategory::Param),
            (3, DeclarationCategory::TypeParam),
            (4, DeclarationCategory::TypeAlias),
            (5, DeclarationCategory::Function),
            (6, DeclarationCategory::Class),
            (7, DeclarationCategory::Import),
        ] {
            let json = serde_json::to_value(cat).unwrap();
            assert_eq!(json, n);
            let back: DeclarationCategory = serde_json::from_value(json).unwrap();
            assert_eq!(back, cat);
        }
    }

    #[test]
    fn type_flags_values() {
        assert_eq!(type_flags::NONE, 0);
        assert_eq!(type_flags::INSTANTIABLE, 1);
        assert_eq!(type_flags::INSTANCE, 2);
        assert_eq!(type_flags::CALLABLE, 4);
        // Flags should be combinable
        let combined = type_flags::INSTANCE | type_flags::CALLABLE;
        assert_eq!(combined, 6);
    }

    #[test]
    fn regular_declaration_serialization() {
        let decl = Declaration::regular(
            DeclarationCategory::Class,
            Url::parse("file:///test.py").unwrap(),
            Range {
                start: Position::new(0, 0),
                end: Position::new(0, 5),
            },
            Some("MyClass".to_string()),
        );
        let json = serde_json::to_value(&decl).unwrap();
        assert_eq!(json["kind"], 0);
        assert_eq!(json["category"], 6);
        assert_eq!(json["name"], "MyClass");
        assert_eq!(json["node"]["uri"], "file:///test.py");
    }

    #[test]
    fn synthesized_declaration_serialization() {
        let decl = Declaration::synthesized("file:///builtins.pyi");
        let json = serde_json::to_value(&decl).unwrap();
        assert_eq!(json["kind"], 1);
        assert_eq!(json["uri"], "file:///builtins.pyi");
        assert!(json.get("node").is_none());
    }

    #[test]
    fn declaration_roundtrip_regular() {
        let decl = Declaration::regular(
            DeclarationCategory::Function,
            Url::parse("file:///test.py").unwrap(),
            Range {
                start: Position::new(1, 4),
                end: Position::new(1, 11),
            },
            Some("my_func".to_string()),
        );
        let json = serde_json::to_string(&decl).unwrap();
        let back: Declaration = serde_json::from_str(&json).unwrap();
        assert_eq!(decl, back);
    }

    #[test]
    fn declaration_roundtrip_synthesized() {
        let decl = Declaration::synthesized("file:///stub.pyi");
        let json = serde_json::to_string(&decl).unwrap();
        let back: Declaration = serde_json::from_str(&json).unwrap();
        assert_eq!(decl, back);
    }

    #[test]
    fn builtin_type_serialization() {
        let ty = Type::unknown(1);
        let json = serde_json::to_value(&ty).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["kind"], 0);
        assert_eq!(json["display"], "Unknown");
        assert_eq!(json["flags"], 0);
        // No details fields should be present
        assert!(json.get("subTypes").is_none());
        assert!(json.get("qualifiedName").is_none());
    }

    #[test]
    fn type_with_union_details_flattened() {
        let ty = Type {
            id: 10,
            kind: TypeKind::Union,
            display: Some("int | str".to_string()),
            flags: type_flags::NONE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Union(UnionDetails {
                members: vec![
                    Type {
                        id: 11,
                        kind: TypeKind::Class,
                        display: Some("int".to_string()),
                        flags: type_flags::INSTANCE,
                        declaration: None,
                        type_args: None,
                        details: None,
                    },
                    Type {
                        id: 12,
                        kind: TypeKind::Class,
                        display: Some("str".to_string()),
                        flags: type_flags::INSTANCE,
                        declaration: None,
                        type_args: None,
                        details: None,
                    },
                ],
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        // Union members should be flattened as "subTypes"
        let sub_types = json["subTypes"].as_array().unwrap();
        assert_eq!(sub_types.len(), 2);
        assert_eq!(sub_types[0]["display"], "int");
        assert_eq!(sub_types[1]["display"], "str");
    }

    #[test]
    fn type_with_class_details_flattened() {
        let ty = Type {
            id: 5,
            kind: TypeKind::Class,
            display: Some("list[int]".to_string()),
            flags: type_flags::INSTANCE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: "builtins.list".to_string(),
                module: Some("builtins".to_string()),
                type_arguments: Some(vec![6]),
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        assert_eq!(json["qualifiedName"], "builtins.list");
        assert_eq!(json["module"], "builtins");
        assert_eq!(json["typeArguments"], serde_json::json!([6]));
    }

    #[test]
    fn type_with_literal_details_flattened() {
        let ty = Type {
            id: 20,
            kind: TypeKind::Class,
            display: Some("Literal[42]".to_string()),
            flags: type_flags::INSTANCE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "42".to_string(),
                literal_kind: LiteralKind::Int,
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        assert_eq!(json["value"], "42");
        assert_eq!(json["literalKind"], "int");
    }

    #[test]
    fn type_camel_case_field_names() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: None,
            flags: type_flags::NONE,
            declaration: None,
            type_args: Some(vec![Type::unknown(2)]),
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: "foo.Bar".to_string(),
                module: None,
                type_arguments: Some(vec![2]),
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        // Verify camelCase naming
        assert!(json.get("typeArgs").is_some());
        assert!(json.get("qualifiedName").is_some());
        assert!(json.get("typeArguments").is_some());
        // No snake_case
        assert!(json.get("type_args").is_none());
        assert!(json.get("qualified_name").is_none());
        assert!(json.get("type_arguments").is_none());
    }

    #[test]
    fn literal_kind_serializes_as_camel_case() {
        assert_eq!(
            serde_json::to_value(LiteralKind::Int).unwrap(),
            serde_json::json!("int")
        );
        assert_eq!(
            serde_json::to_value(LiteralKind::Bool).unwrap(),
            serde_json::json!("bool")
        );
        assert_eq!(
            serde_json::to_value(LiteralKind::EnumMember).unwrap(),
            serde_json::json!("enumMember")
        );
    }

    #[test]
    fn parameter_kind_serializes_as_camel_case() {
        assert_eq!(
            serde_json::to_value(ParameterKind::VarPositional).unwrap(),
            serde_json::json!("varPositional")
        );
        assert_eq!(
            serde_json::to_value(ParameterKind::KeywordOnly).unwrap(),
            serde_json::json!("keywordOnly")
        );
    }

    #[test]
    fn function_details_flattened() {
        let ty = Type {
            id: 30,
            kind: TypeKind::Function,
            display: Some("def foo(x: int) -> str".to_string()),
            flags: type_flags::CALLABLE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Function(FunctionDetails {
                name: Some("foo".to_string()),
                parameters: Some(vec![FunctionParameter {
                    name: "x".to_string(),
                    type_id: Some(1),
                    has_default: Some(false),
                    kind: Some(ParameterKind::PositionalOrKeyword),
                }]),
                return_type: Some(2),
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        assert_eq!(json["name"], "foo");
        assert_eq!(json["returnType"], 2);
        let params = json["parameters"].as_array().unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0]["name"], "x");
        assert_eq!(params[0]["typeId"], 1);
    }

    #[test]
    fn synthesized_details_flattened() {
        let ty = Type {
            id: 40,
            kind: TypeKind::Synthesized,
            display: Some("def bar() -> None".to_string()),
            flags: type_flags::CALLABLE,
            declaration: Some(Declaration::synthesized("tsp://stub/40")),
            type_args: None,
            details: Some(TypeDetails::Synthesized(SynthesizedDetails {
                stub_content: "def bar() -> None: ...".to_string(),
                metadata: SynthesizedMetadata {
                    module: ModuleName {
                        name_parts: vec!["mymodule".to_string()],
                    },
                    primary_definition_offset: 0,
                },
            })),
        };
        let json = serde_json::to_value(&ty).unwrap();
        assert_eq!(json["stubContent"], "def bar() -> None: ...");
        assert_eq!(
            json["metadata"]["module"]["nameParts"],
            serde_json::json!(["mymodule"])
        );
        assert_eq!(json["metadata"]["primaryDefinitionOffset"], 0);
    }
}
