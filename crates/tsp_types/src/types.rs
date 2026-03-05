//! TSP type representation.
//!
//! This module defines the type structures used to represent Python types
//! in the TSP protocol. These correspond to the TypeScript types in
//! `typeServerProtocol.ts`.

use lsp_types::{Range, Url};
use serde::{Deserialize, Serialize};

use crate::requests::Node;

/// A unique identifier for a type within a snapshot.
pub type TypeId = i64;

// ── Declaration types (matches TypeServerProtocol.Declaration) ──────────

/// Discriminator for the Declaration union type.
/// Matches `TypeServerProtocol.DeclarationKind` in TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeclarationKind {
    /// Declaration exists in source code with AST node.
    Regular = 0,
    /// Declaration created by type checker (no source node).
    Synthesized = 1,
}

impl Serialize for DeclarationKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DeclarationKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(DeclarationKind::Regular),
            1 => Ok(DeclarationKind::Synthesized),
            _ => Err(serde::de::Error::custom(format!(
                "invalid DeclarationKind: {value}"
            ))),
        }
    }
}

/// Represents the category of a declaration.
/// Matches `TypeServerProtocol.DeclarationCategory` in TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeclarationCategory {
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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DeclarationCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(DeclarationCategory::Intrinsic),
            1 => Ok(DeclarationCategory::Variable),
            2 => Ok(DeclarationCategory::Param),
            3 => Ok(DeclarationCategory::TypeParam),
            4 => Ok(DeclarationCategory::TypeAlias),
            5 => Ok(DeclarationCategory::Function),
            6 => Ok(DeclarationCategory::Class),
            7 => Ok(DeclarationCategory::Import),
            _ => Err(serde::de::Error::custom(format!(
                "invalid DeclarationCategory: {value}"
            ))),
        }
    }
}

/// A declaration in the type system.
/// Matches `TypeServerProtocol.Declaration` (= `RegularDeclaration` | `SynthesizedDeclaration`).
///
/// Uses internally-tagged serialization:
///   Regular   → `{ "kind": 0, "category": N, "node": {...}, "name": "..." }`
///   Synthesized → `{ "kind": 1, "uri": "..." }`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Regular {
        category: DeclarationCategory,
        node: Node,
        name: Option<String>,
    },
    Synthesized {
        uri: String,
    },
}

impl Serialize for Declaration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            Declaration::Regular {
                category,
                node,
                name,
            } => {
                let mut map = serializer.serialize_map(Some(if name.is_some() { 4 } else { 3 }))?;
                map.serialize_entry("kind", &DeclarationKind::Regular)?;
                map.serialize_entry("category", category)?;
                map.serialize_entry("node", node)?;
                if let Some(n) = name {
                    map.serialize_entry("name", n)?;
                }
                map.end()
            }
            Declaration::Synthesized { uri } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", &DeclarationKind::Synthesized)?;
                map.serialize_entry("uri", uri)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Declaration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
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
                Ok(Declaration::Regular {
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
                Ok(Declaration::Synthesized { uri })
            }
            _ => Err(serde::de::Error::custom(format!(
                "invalid DeclarationKind: {kind}"
            ))),
        }
    }
}

impl Declaration {
    /// Create a Regular declaration pointing to a source location.
    pub fn regular(
        category: DeclarationCategory,
        uri: Url,
        range: Range,
        name: Option<String>,
    ) -> Self {
        Declaration::Regular {
            category,
            node: Node { uri, range },
            name,
        }
    }

    /// Create a Synthesized declaration with just a URI.
    pub fn synthesized(uri: impl Into<String>) -> Self {
        Declaration::Synthesized { uri: uri.into() }
    }
}

/// The kind of a type (discriminator for the Type union).
/// Matches `TypeServerProtocol.TypeKind` in TypeScript.
/// Serializes as a number (0-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TypeKind {
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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for TypeKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(TypeKind::BuiltIn),
            1 => Ok(TypeKind::Declared),
            2 => Ok(TypeKind::Function),
            3 => Ok(TypeKind::Class),
            4 => Ok(TypeKind::Union),
            5 => Ok(TypeKind::Module),
            6 => Ok(TypeKind::TypeVar),
            7 => Ok(TypeKind::Overloaded),
            8 => Ok(TypeKind::Synthesized),
            9 => Ok(TypeKind::TypeReference),
            _ => Err(serde::de::Error::custom(format!(
                "invalid TypeKind: {value}"
            ))),
        }
    }
}

/// Type category enum for internal use (maps to `TypeKind` for serialization).
/// This is used by the handlers to categorize types before converting to TSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCategory {
    /// Unknown type.
    Unknown,
    /// Any type.
    Any,
    /// Never type (bottom type).
    Never,
    /// None type.
    None,
    /// A class type (the class itself, not an instance).
    Class,
    /// An instance of a class.
    Instance,
    /// A union of types.
    Union,
    /// A function type.
    Function,
    /// An overloaded function.
    OverloadedFunction,
    /// A module type.
    Module,
    /// A type variable.
    TypeVar,
    /// A `ParamSpec`.
    ParamSpec,
    /// A `TypeVarTuple`.
    TypeVarTuple,
    /// A literal type.
    Literal,
    /// A tuple type.
    Tuple,
    /// A `TypedDict` type.
    TypedDict,
    /// A type alias.
    TypeAlias,
    /// A reference to another type (for cycle breaking).
    TypeReference,
}

impl TypeCategory {
    /// Convert to `TypeKind` for TSP protocol serialization.
    pub fn to_type_kind(self) -> TypeKind {
        match self {
            TypeCategory::Unknown
            | TypeCategory::Any
            | TypeCategory::Never
            | TypeCategory::None => TypeKind::BuiltIn,
            TypeCategory::Class
            | TypeCategory::Instance
            | TypeCategory::Literal
            | TypeCategory::Tuple
            | TypeCategory::TypedDict => TypeKind::Class,
            TypeCategory::Union => TypeKind::Union,
            TypeCategory::Function => TypeKind::Function,
            TypeCategory::OverloadedFunction => TypeKind::Overloaded,
            TypeCategory::Module => TypeKind::Module,
            TypeCategory::TypeVar | TypeCategory::ParamSpec | TypeCategory::TypeVarTuple => {
                TypeKind::TypeVar
            }
            TypeCategory::TypeAlias => TypeKind::Declared,
            TypeCategory::TypeReference => TypeKind::TypeReference,
        }
    }

    /// Convert to `TypeFlags` for TSP protocol serialization.
    pub fn to_flags(self) -> u32 {
        match self {
            TypeCategory::Instance
            | TypeCategory::Literal
            | TypeCategory::Tuple
            | TypeCategory::TypedDict => TypeFlags::INSTANCE,
            TypeCategory::Class => TypeFlags::INSTANTIABLE,
            TypeCategory::Function | TypeCategory::OverloadedFunction => TypeFlags::CALLABLE,
            _ => TypeFlags::NONE,
        }
    }
}

/// Bitfield flags that describe characteristics of a type.
/// Matches `TypeServerProtocol.TypeFlags` in TypeScript.
#[allow(non_snake_case)]
pub mod TypeFlags {
    /// No flags set.
    pub const NONE: u32 = 0;
    /// Indicates if the type can be instantiated (e.g., a class itself).
    pub const INSTANTIABLE: u32 = 1 << 0;
    /// Indicates if the type represents an instance (as opposed to a class).
    pub const INSTANCE: u32 = 1 << 1;
    /// Indicates if an instance of the type can be called like a function.
    pub const CALLABLE: u32 = 1 << 2;
}

/// A type in the TSP protocol.
/// This is a simplified representation that matches the TypeScript protocol's Type union.
///
/// Serialization: `details` is flattened into the parent JSON object to match
/// the TypeScript protocol's flat type union (e.g., `stubContent` appears at
/// the top level, not nested under a `details` key).
///
/// Note: We derive `Deserialize` normally (with `details` as a nested field
/// via `#[serde(default)]`), but implement `Serialize` manually for flattening.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Type {
    /// The unique ID of this type within the snapshot.
    pub id: TypeId,
    /// The kind of this type (discriminator).
    pub kind: TypeKind,
    /// Human-readable string representation (used for display purposes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Bitfield of `TypeFlags` that describe characteristics of the type.
    #[serde(default)]
    pub flags: u32,
    /// Declaration information for declared types (Class, Function, Declared kinds).
    /// Points to the source location where this type is defined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration: Option<Declaration>,
    /// Type arguments for generic class types (e.g., `int` in `list[int]`).
    /// Each element is a full inline Type object.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "typeArgs")]
    pub type_args: Option<Vec<Type>>,
    /// Additional details based on kind.
    #[serde(default)]
    pub details: Option<TypeDetails>,
}

impl Serialize for Type {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // Count fields: id + kind + flags are always present.
        // display, declaration, typeArgs, and details fields are optional.
        let mut field_count = 3; // id, kind, flags
        if self.display.is_some() {
            field_count += 1;
        }
        if self.declaration.is_some() {
            field_count += 1;
        }
        if self.type_args.is_some() {
            field_count += 1;
        }
        // Details fields are flattened, so count the inner fields.
        let detail_field_count = match &self.details {
            Some(TypeDetails::Synthesized(_)) => 2, // stubContent, metadata
            Some(TypeDetails::Union(_)) => 1,       // subTypes
            Some(TypeDetails::Literal(_)) => 2,     // value, literalKind
            Some(TypeDetails::Tuple(_)) => 2,       // elements, isUnbounded (max)
            Some(TypeDetails::TypeReference(_)) => 1, // referencedTypeId
            Some(TypeDetails::Module(_)) => 1,      // moduleName
            Some(TypeDetails::Class(_)) => 3,       // qualifiedName, module, typeArguments (max)
            Some(TypeDetails::Overloaded(_)) => 2,  // overloads, implementation (max)
            Some(TypeDetails::Function(_)) => 3,    // name, parameters, returnType (max)
            None => 0,
        };
        field_count += detail_field_count;

        let mut map = serializer.serialize_map(Some(field_count))?;
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
        match &self.details {
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

        map.end()
    }
}

/// Type-specific details.
///
/// IMPORTANT: Variant order matters for untagged deserialization!
/// Variants with required fields MUST come before variants with all optional fields.
/// Otherwise, a variant like Function (all optional) will match everything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeDetails {
    /// Details for synthesized types (required: stubContent, metadata).
    Synthesized(SynthesizedDetails),
    /// Details for union types (serialized as subTypes in JSON).
    Union(UnionDetails),
    /// Details for literal types (required: value, literalKind).
    Literal(LiteralDetails),
    /// Details for tuple types (required: elements).
    Tuple(TupleDetails),
    /// Details for type references (required: referencedTypeId).
    TypeReference(TypeReferenceDetails),
    /// Details for module types (required: moduleName).
    Module(ModuleDetails),
    /// Details for class/instance types (required: qualifiedName).
    Class(ClassDetails),
    /// Details for overloaded function types (required: overloads).
    Overloaded(OverloadedDetails),
    /// Details for function types (all optional - MUST be last!).
    Function(FunctionDetails),
}

/// Details for a class or instance type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDetails {
    /// The fully qualified name of the class.
    pub qualified_name: String,
    /// The module where the class is defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Type arguments (for generic classes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_arguments: Option<Vec<TypeId>>,
}

/// Details for a union type.
/// The `members` field contains inline Type objects and is serialized as
/// `subTypes` in JSON to match the TypeScript protocol's `UnionType.subTypes: Type[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnionDetails {
    /// The inline member types of the union.
    pub members: Vec<Type>,
}

/// Details for an overloaded function type.
/// Matches the TypeScript protocol's `OverloadedType`:
/// ```typescript
/// interface OverloadedType extends TypeBase<TypeKind.Overloaded> {
///     overloads: Type[];
///     implementation?: Type;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverloadedDetails {
    /// The `@overload` decorated signatures.
    pub overloads: Vec<Type>,
    /// The implementation signature (if present — the non-`@overload` `def`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation: Option<Box<Type>>,
}

/// Details for a function type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDetails {
    /// The name of the function.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The parameters of the function.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<FunctionParameter>>,
    /// The return type ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<TypeId>,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionParameter {
    /// The name of the parameter.
    pub name: String,
    /// The type ID of the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<TypeId>,
    /// Whether the parameter has a default value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_default: Option<bool>,
    /// The kind of parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ParameterKind>,
}

/// The kind of a function parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterKind {
    /// A positional-only parameter.
    PositionalOnly,
    /// A positional-or-keyword parameter.
    PositionalOrKeyword,
    /// A *args parameter.
    VarPositional,
    /// A keyword-only parameter.
    KeywordOnly,
    /// A **kwargs parameter.
    VarKeyword,
}

/// Details for a literal type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiteralDetails {
    /// The literal value as a string.
    pub value: String,
    /// The kind of literal.
    pub literal_kind: LiteralKind,
}

/// The kind of a literal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiteralKind {
    /// An integer literal.
    Int,
    /// A boolean literal.
    Bool,
    /// A string literal.
    Str,
    /// A bytes literal.
    Bytes,
    /// An enum member.
    EnumMember,
}

/// Details for a tuple type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TupleDetails {
    /// The element types.
    pub elements: Vec<TypeId>,
    /// Whether this is a variable-length tuple.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_unbounded: Option<bool>,
}

/// Details for a type reference (for cycle breaking).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeReferenceDetails {
    /// The ID of the referenced type.
    pub referenced_type_id: TypeId,
}

/// Details for a module type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDetails {
    /// The fully qualified module name.
    pub module_name: String,
}

/// Details for a synthesized type (type without source code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizedDetails {
    /// Python stub file content (.pyi format) generated for this type.
    pub stub_content: String,
    /// Additional metadata about the synthesized type.
    pub metadata: SynthesizedMetadata,
}

/// Metadata for a synthesized type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizedMetadata {
    /// Module where the synthesized type is defined.
    pub module: ModuleName,
    /// Character offset into the stubContent where the primary definition starts.
    pub primary_definition_offset: usize,
}

/// Represents a module name in the form of "segments.separated.by.dots".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleName {
    /// The segments of the module name (e.g., `["typing", "Protocol"]` for `typing.Protocol`).
    pub name_parts: Vec<String>,
}

impl Type {
    /// Create a new Unknown type (`BuiltIn` kind).
    pub fn unknown(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Unknown".to_string()),
            flags: TypeFlags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a new Any type (`BuiltIn` kind).
    pub fn any(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Any".to_string()),
            flags: TypeFlags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a new Never type (`BuiltIn` kind).
    pub fn never(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("Never".to_string()),
            flags: TypeFlags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a new None type (`BuiltIn` kind).
    pub fn none(id: TypeId) -> Self {
        Self {
            id,
            kind: TypeKind::BuiltIn,
            display: Some("None".to_string()),
            flags: TypeFlags::NONE,
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a new class type.
    pub fn class(id: TypeId, qualified_name: impl Into<String>) -> Self {
        let name = qualified_name.into();
        Self {
            id,
            kind: TypeKind::Class,
            display: Some(format!("type[{}]", &name)),
            flags: TypeFlags::INSTANTIABLE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: name,
                module: None,
                type_arguments: None,
            })),
        }
    }

    /// Create a new instance type (Class kind with instance semantics).
    pub fn instance(id: TypeId, qualified_name: impl Into<String>) -> Self {
        let name = qualified_name.into();
        Self {
            id,
            kind: TypeKind::Class,
            display: Some(name.clone()),
            flags: TypeFlags::INSTANCE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: name,
                module: None,
                type_arguments: None,
            })),
        }
    }

    /// Create a new union type with inline member Type objects.
    pub fn union(id: TypeId, members: Vec<Type>, display: impl Into<String>) -> Self {
        Self {
            id,
            kind: TypeKind::Union,
            display: Some(display.into()),
            flags: TypeFlags::NONE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Union(UnionDetails { members })),
        }
    }

    /// Create a new overloaded function type with inline overload Type objects.
    pub fn overloaded(
        id: TypeId,
        overloads: Vec<Type>,
        implementation: Option<Type>,
        display: impl Into<String>,
    ) -> Self {
        Self {
            id,
            kind: TypeKind::Overloaded,
            display: Some(display.into()),
            flags: TypeFlags::CALLABLE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Overloaded(OverloadedDetails {
                overloads,
                implementation: implementation.map(Box::new),
            })),
        }
    }

    /// Create a new function type.
    pub fn function(id: TypeId, name: Option<String>, return_type: Option<TypeId>) -> Self {
        Self {
            id,
            kind: TypeKind::Function,
            display: name.as_ref().map(|n| format!("def {n}(...)")),
            flags: TypeFlags::CALLABLE,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Function(FunctionDetails {
                name,
                parameters: None,
                return_type,
            })),
        }
    }

    /// Create a Type from a `TypeCategory` (for handler use).
    pub fn from_category(id: TypeId, category: TypeCategory, display: Option<String>) -> Self {
        Self {
            id,
            kind: category.to_type_kind(),
            display,
            flags: category.to_flags(),
            declaration: None,
            type_args: None,
            details: None,
        }
    }

    /// Create a declared class type with a source declaration.
    ///
    /// Returns a `ClassType` (kind=3) with a Regular declaration pointing to
    /// the class definition in source code (e.g., `builtins.pyi`).
    pub fn declared_class(
        id: TypeId,
        display: impl Into<String>,
        declaration: Declaration,
        type_args: Option<Vec<Type>>,
        flags: u32,
    ) -> Self {
        Self {
            id,
            kind: TypeKind::Class,
            display: Some(display.into()),
            flags,
            declaration: Some(declaration),
            type_args,
            details: None,
        }
    }

    /// Create a declared function type with a source declaration.
    ///
    /// Returns a `FunctionType` (kind=2) with a Regular declaration pointing to
    /// the function definition in source code.
    pub fn declared_function(
        id: TypeId,
        display: impl Into<String>,
        declaration: Declaration,
        flags: u32,
    ) -> Self {
        Self {
            id,
            kind: TypeKind::Function,
            display: Some(display.into()),
            flags,
            declaration: Some(declaration),
            type_args: None,
            details: None,
        }
    }

    /// Create a new synthesized type.
    pub fn synthesized(
        id: TypeId,
        display: Option<String>,
        stub_content: impl Into<String>,
        module_parts: Vec<String>,
        primary_definition_offset: usize,
        flags: u32,
    ) -> Self {
        Self {
            id,
            kind: TypeKind::Synthesized,
            display,
            flags,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Synthesized(SynthesizedDetails {
                stub_content: stub_content.into(),
                metadata: SynthesizedMetadata {
                    module: ModuleName {
                        name_parts: module_parts,
                    },
                    primary_definition_offset,
                },
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_kind_serialization() {
        // TypeKind uses numeric repr
        assert_eq!(serde_json::to_string(&TypeKind::BuiltIn).unwrap(), "0");
        assert_eq!(serde_json::to_string(&TypeKind::Class).unwrap(), "3");
        assert_eq!(serde_json::to_string(&TypeKind::Union).unwrap(), "4");
        assert_eq!(serde_json::to_string(&TypeKind::Function).unwrap(), "2");
    }

    /// Helper: verify that a Type serializes to valid JSON with expected core fields.
    fn assert_type_serializes(ty: &Type) {
        let json = serde_json::to_string(ty).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["id"], ty.id);
        assert_eq!(value["kind"], ty.kind as u8);
        assert_eq!(value["flags"], ty.flags);
        if let Some(ref display) = ty.display {
            assert_eq!(value["display"], display.as_str());
        }
        // Details fields should be flattened (at top level, not nested under "details")
        assert!(
            value.get("details").is_none(),
            "details should be flattened into top-level fields"
        );
    }

    #[test]
    fn test_unknown_type() {
        let ty = Type::unknown(1);
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::BuiltIn);
    }

    #[test]
    fn test_instance_type() {
        let ty = Type::instance(1, "builtins.int");
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Class);
        assert_eq!(ty.display, Some("builtins.int".to_string()));
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""qualifiedName":"builtins.int""#));
    }

    #[test]
    fn test_class_type() {
        let ty = Type::class(1, "builtins.int");
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Class);
        assert_eq!(ty.display, Some("type[builtins.int]".to_string()));
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""qualifiedName":"builtins.int""#));
    }

    #[test]
    fn test_union_type() {
        let members = vec![
            Type::instance(1, "builtins.int"),
            Type::instance(2, "builtins.str"),
        ];
        let ty = Type::union(3, members, "int | str");
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Union);
        let json = serde_json::to_string(&ty).unwrap();
        // subTypes should be an array of inline Type objects
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sub_types = value.get("subTypes").expect("should have subTypes");
        assert!(sub_types.is_array());
        let arr = sub_types.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[1]["id"], 2);
    }

    #[test]
    fn test_function_type() {
        let ty = Type::function(1, Some("foo".to_string()), Some(2));
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Function);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""name":"foo""#));
        assert!(json.contains(r#""returnType":2"#));
    }

    #[test]
    fn test_literal_details() {
        let details = LiteralDetails {
            value: "42".to_string(),
            literal_kind: LiteralKind::Int,
        };
        let json = serde_json::to_string(&details).unwrap();
        assert!(json.contains("\"literalKind\":\"int\""));
    }

    #[test]
    fn test_parameter_kind_serialization() {
        assert_eq!(
            serde_json::to_string(&ParameterKind::PositionalOnly).unwrap(),
            r#""positionalOnly""#
        );
        assert_eq!(
            serde_json::to_string(&ParameterKind::VarPositional).unwrap(),
            r#""varPositional""#
        );
    }

    #[test]
    fn test_type_category_to_kind() {
        assert_eq!(TypeCategory::Unknown.to_type_kind(), TypeKind::BuiltIn);
        assert_eq!(TypeCategory::Any.to_type_kind(), TypeKind::BuiltIn);
        assert_eq!(TypeCategory::Class.to_type_kind(), TypeKind::Class);
        assert_eq!(TypeCategory::Instance.to_type_kind(), TypeKind::Class);
        assert_eq!(TypeCategory::Union.to_type_kind(), TypeKind::Union);
        assert_eq!(TypeCategory::Function.to_type_kind(), TypeKind::Function);
        assert_eq!(TypeCategory::Module.to_type_kind(), TypeKind::Module);
        assert_eq!(TypeCategory::TypeVar.to_type_kind(), TypeKind::TypeVar);
    }

    // ========== Additional comprehensive round trip tests ==========

    #[test]
    fn test_any_type_roundtrip() {
        let ty = Type::any(1);
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::BuiltIn);
        assert_eq!(ty.display, Some("Any".to_string()));
    }

    #[test]
    fn test_never_type_roundtrip() {
        let ty = Type::never(1);
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::BuiltIn);
        assert_eq!(ty.display, Some("Never".to_string()));
    }

    #[test]
    fn test_none_type_roundtrip() {
        let ty = Type::none(1);
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::BuiltIn);
        assert_eq!(ty.display, Some("None".to_string()));
    }

    #[test]
    fn test_literal_int_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("Literal[42]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "42".to_string(),
                literal_kind: LiteralKind::Int,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""literalKind":"int""#));
    }

    #[test]
    fn test_literal_bool_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("Literal[True]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "True".to_string(),
                literal_kind: LiteralKind::Bool,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""literalKind":"bool""#));
    }

    #[test]
    fn test_literal_str_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("Literal['hello']".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "hello".to_string(),
                literal_kind: LiteralKind::Str,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""literalKind":"str""#));
    }

    #[test]
    fn test_literal_bytes_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("Literal[b'data']".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "data".to_string(),
                literal_kind: LiteralKind::Bytes,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""literalKind":"bytes""#));
    }

    #[test]
    fn test_literal_enum_member_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("Color.RED".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Literal(LiteralDetails {
                value: "RED".to_string(),
                literal_kind: LiteralKind::EnumMember,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""literalKind":"enumMember""#));
    }

    #[test]
    fn test_tuple_fixed_length_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("tuple[int, str, bool]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Tuple(TupleDetails {
                elements: vec![2, 3, 4],
                is_unbounded: None,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""elements":[2,3,4]"#));
    }

    #[test]
    fn test_tuple_unbounded_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("tuple[int, ...]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Tuple(TupleDetails {
                elements: vec![2],
                is_unbounded: Some(true),
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""isUnbounded":true"#));
    }

    #[test]
    fn test_module_type_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Module,
            display: Some("module 'os.path'".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Module(ModuleDetails {
                module_name: "os.path".to_string(),
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""moduleName":"os.path""#));
    }

    #[test]
    fn test_type_reference_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::TypeReference,
            display: Some("Node (recursive)".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::TypeReference(TypeReferenceDetails {
                referenced_type_id: 2,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""referencedTypeId":2"#));
    }

    #[test]
    fn test_synthesized_type_roundtrip() {
        let ty = Type::synthesized(
            1,
            Some("MyDataclass".to_string()),
            "class MyDataclass:\n    def __init__(self, x: int) -> None: ...",
            vec!["mymodule".to_string()],
            0,
            TypeFlags::INSTANCE,
        );
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Synthesized);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""stubContent":"#));
        assert!(json.contains(r#""primaryDefinitionOffset":0"#));
        assert!(json.contains(r#""nameParts":["mymodule"]"#));
    }

    #[test]
    fn test_class_with_module_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("type[MyClass]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: "mypackage.MyClass".to_string(),
                module: Some("mypackage".to_string()),
                type_arguments: None,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""module":"mypackage""#));
    }

    #[test]
    fn test_class_with_type_arguments_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("list[int]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: "builtins.list".to_string(),
                module: Some("builtins".to_string()),
                type_arguments: Some(vec![2]),
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""typeArguments":[2]"#));
    }

    #[test]
    fn test_function_with_full_parameters_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Function,
            display: Some(
                "def foo(a: int, b: str = 'default', *args: Any, **kwargs: Any) -> bool"
                    .to_string(),
            ),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Function(FunctionDetails {
                name: Some("foo".to_string()),
                parameters: Some(vec![
                    FunctionParameter {
                        name: "a".to_string(),
                        type_id: Some(2),
                        has_default: Some(false),
                        kind: Some(ParameterKind::PositionalOrKeyword),
                    },
                    FunctionParameter {
                        name: "b".to_string(),
                        type_id: Some(3),
                        has_default: Some(true),
                        kind: Some(ParameterKind::PositionalOrKeyword),
                    },
                    FunctionParameter {
                        name: "args".to_string(),
                        type_id: Some(4),
                        has_default: None,
                        kind: Some(ParameterKind::VarPositional),
                    },
                    FunctionParameter {
                        name: "kwargs".to_string(),
                        type_id: Some(4),
                        has_default: None,
                        kind: Some(ParameterKind::VarKeyword),
                    },
                ]),
                return_type: Some(5),
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""kind":"positionalOrKeyword""#));
        assert!(json.contains(r#""kind":"varPositional""#));
        assert!(json.contains(r#""kind":"varKeyword""#));
    }

    #[test]
    fn test_function_with_positional_only_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Function,
            display: Some("def foo(a, /, b)".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Function(FunctionDetails {
                name: Some("foo".to_string()),
                parameters: Some(vec![
                    FunctionParameter {
                        name: "a".to_string(),
                        type_id: None,
                        has_default: None,
                        kind: Some(ParameterKind::PositionalOnly),
                    },
                    FunctionParameter {
                        name: "b".to_string(),
                        type_id: None,
                        has_default: None,
                        kind: Some(ParameterKind::PositionalOrKeyword),
                    },
                ]),
                return_type: None,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""kind":"positionalOnly""#));
    }

    #[test]
    fn test_function_with_keyword_only_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Function,
            display: Some("def foo(*, a: int, b: str)".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Function(FunctionDetails {
                name: Some("foo".to_string()),
                parameters: Some(vec![
                    FunctionParameter {
                        name: "a".to_string(),
                        type_id: Some(2),
                        has_default: None,
                        kind: Some(ParameterKind::KeywordOnly),
                    },
                    FunctionParameter {
                        name: "b".to_string(),
                        type_id: Some(3),
                        has_default: None,
                        kind: Some(ParameterKind::KeywordOnly),
                    },
                ]),
                return_type: None,
            })),
        };
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        assert!(json.contains(r#""kind":"keywordOnly""#));
    }

    #[test]
    fn test_overloaded_function_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::Overloaded,
            display: Some("Overload[def foo(x: int), def foo(x: str)]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: None,
        };
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::Overloaded);
    }

    #[test]
    fn test_typevar_roundtrip() {
        let ty = Type {
            id: 1,
            kind: TypeKind::TypeVar,
            display: Some("T".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: None,
        };
        assert_type_serializes(&ty);
        assert_eq!(ty.kind, TypeKind::TypeVar);
    }

    #[test]
    fn test_complex_union_roundtrip() {
        let members = vec![
            Type::instance(2, "builtins.int"),
            Type::instance(3, "builtins.str"),
            Type::from_category(4, TypeCategory::Instance, Some("None".to_string())),
            Type::instance(5, "builtins.list"),
        ];
        let ty = Type::union(1, members, "int | str | None | list[Any]");
        assert_type_serializes(&ty);
        let json = serde_json::to_string(&ty).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sub_types = value.get("subTypes").expect("should have subTypes");
        let arr = sub_types.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["id"], 2);
        assert_eq!(arr[1]["id"], 3);
        assert_eq!(arr[2]["id"], 4);
        assert_eq!(arr[3]["id"], 5);
    }

    #[test]
    fn test_json_field_names_camelcase() {
        // Verify that field names are properly camelCased in JSON
        let ty = Type {
            id: 1,
            kind: TypeKind::Class,
            display: Some("list[int]".to_string()),
            flags: 0,
            declaration: None,
            type_args: None,
            details: Some(TypeDetails::Class(ClassDetails {
                qualified_name: "builtins.list".to_string(),
                module: Some("builtins".to_string()),
                type_arguments: Some(vec![2]),
            })),
        };
        let json = serde_json::to_string(&ty).unwrap();

        // Should have camelCase field names
        assert!(json.contains(r#""qualifiedName""#));
        assert!(json.contains(r#""typeArguments""#));

        // Should NOT have snake_case field names
        assert!(!json.contains(r#""qualified_name""#));
        assert!(!json.contains(r#""type_arguments""#));
    }

    #[test]
    fn test_optional_fields_omitted() {
        // Verify that None optional fields are omitted from JSON
        let ty = Type::unknown(1);
        let json = serde_json::to_string(&ty).unwrap();

        // Should NOT contain the "details" field since it's None
        assert!(!json.contains(r#""details""#));

        // Function without parameters
        let func = Type::function(1, Some("foo".to_string()), None);
        let json = serde_json::to_string(&func).unwrap();

        // Should NOT contain returnType since it's None
        assert!(!json.contains(r#""returnType""#));
    }
}
