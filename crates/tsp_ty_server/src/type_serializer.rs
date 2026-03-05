//! Type serialization for TSP protocol.
//!
//! This module converts `ty_ide`'s `TypeInfo` (category + display string)
//! to the TSP protocol's `Type` format. It works entirely through `ty_ide`'s
//! public API — no direct dependency on ty's internal `Type<'db>`.
//!
//! The approach:
//! 1. Use `ty_ide::TypeCategory` to determine the TSP category
//! 2. Use the display string (from `TypeInfo.display`) for label/stub content
//! 3. Use the signature string (from `TypeInfo.signature`) when available
//!
//! This keeps all TSP-specific logic in `tsp_ty_server` and avoids changes
//! to the base ty crates.

use ty_ide::TypeCategory as TyCategory;

use tsp_types::types::{Declaration, Type, TypeCategory as TspCategory, TypeId};

use crate::stub_generator;

/// Convert a `ty_ide::TypeCategory` to a `tsp_types::TypeCategory`.
pub(crate) fn convert_category(category: TyCategory) -> TspCategory {
    match category {
        TyCategory::Unknown => TspCategory::Unknown,
        TyCategory::Any | TyCategory::Dynamic => TspCategory::Any,
        TyCategory::Never => TspCategory::Never,
        TyCategory::None => TspCategory::Instance,
        TyCategory::Instance | TyCategory::Property | TyCategory::TypeGuard => {
            TspCategory::Instance
        }
        TyCategory::Class | TyCategory::SubclassOf => TspCategory::Class,
        TyCategory::Union => TspCategory::Union,
        TyCategory::Intersection => TspCategory::Unknown,
        TyCategory::Function | TyCategory::BoundMethod | TyCategory::Callable => {
            TspCategory::Function
        }
        TyCategory::OverloadedFunction => TspCategory::OverloadedFunction,
        TyCategory::Module => TspCategory::Module,
        TyCategory::TypeVar => TspCategory::TypeVar,
        TyCategory::Literal => TspCategory::Literal,
        TyCategory::Tuple => TspCategory::Instance,
        TyCategory::TypedDict => TspCategory::TypedDict,
        TyCategory::TypeAlias => TspCategory::TypeAlias,
        TyCategory::SpecialForm => TspCategory::Instance,
        TyCategory::NewType => TspCategory::Instance,
    }
}

/// Information about the resolved declaration for a type, to be passed
/// from the handler to the serializer.
pub(crate) struct ResolvedDeclaration {
    /// The TSP declaration (Regular with URI + range pointing to the definition).
    pub declaration: Declaration,
    /// The name of the declared symbol (e.g., "list", "int", "`MyClass`").
    #[allow(dead_code)]
    pub name: Option<String>,
}

/// Convert a `ty_ide::TypeInfo` to a TSP `Type`.
///
/// This is the main entry point for converting type information from `ty_ide`
/// into the TSP protocol format.
///
/// When a `resolved_declaration` is provided (meaning the type has a known
/// source declaration, like a class in `builtins.pyi`), we return a declared
/// `ClassType` or `FunctionType` pointing to that location.
///
/// When no declaration is available, we fall back to generating a synthesized
/// stub type.
///
/// # Arguments
///
/// * `id` - The TSP type ID to assign.
/// * `category` - The type category from `ty_ide`.
/// * `display` - The display string from `ty_ide` (TypeInfo.display).
/// * `signature` - Optional signature string (TypeInfo.signature), used when available.
/// * `resolved_declaration` - Optional declaration info resolved by the handler.
/// * `union_members` - Optional pre-converted inline Type objects for union types.
///   When the category is Union and this is Some, a union type with inline subTypes is produced.
#[allow(clippy::too_many_arguments)]
pub(crate) fn convert_type_info(
    id: TypeId,
    category: TyCategory,
    display: &str,
    signature: Option<&str>,
    resolved_declaration: Option<ResolvedDeclaration>,
    union_members: Option<Vec<Type>>,
    overload_members: Option<Vec<Type>>,
    implementation_member: Option<Box<Type>>,
) -> Type {
    let tsp_category = convert_category(category);

    // Use signature if available, otherwise use display
    let label = signature.unwrap_or(display).to_string();

    // If we have a declaration, return a declared type instead of synthesizing.
    // This covers types defined in real .pyi files (builtins, typeshed, etc.)
    if let Some(resolved) = resolved_declaration {
        let flags = tsp_category.to_flags();

        match tsp_category {
            TspCategory::Instance | TspCategory::Class => {
                return Type::declared_class(id, label, resolved.declaration, None, flags);
            }
            TspCategory::Function => {
                return Type::declared_function(id, label, resolved.declaration, flags);
            }
            _ => {
                // For other categories (Module, TypeVar, etc.), we still use from_category.
                // In the future we could extend declared types to cover these.
            }
        }
    }

    // For union types with pre-converted member types, build a union with inline subTypes.
    if tsp_category == TspCategory::Union {
        if let Some(members) = union_members {
            return Type::union(id, members, label);
        }
    }

    // For overloaded function types with pre-converted overload members,
    // build an overloaded type with inline overloads and optional implementation.
    if tsp_category == TspCategory::OverloadedFunction {
        if let Some(overloads) = overload_members {
            return Type::overloaded(id, overloads, implementation_member.map(|b| *b), label);
        }
    }

    // Try to generate stub content for synthesized types
    if let Some((stub_content, module_parts, offset)) =
        stub_generator::generate_stub(category, display)
    {
        return Type::synthesized(
            id,
            Some(label),
            stub_content,
            module_parts,
            offset,
            tsp_category.to_flags(),
        );
    }

    // Otherwise, use the regular category-based representation
    Type::from_category(id, tsp_category, Some(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsp_types::types::TypeFlags;

    #[test]
    fn test_convert_category_instance() {
        assert_eq!(
            convert_category(TyCategory::Instance),
            TspCategory::Instance
        );
    }

    #[test]
    fn test_convert_category_class() {
        assert_eq!(convert_category(TyCategory::Class), TspCategory::Class);
    }

    #[test]
    fn test_convert_category_function() {
        assert_eq!(
            convert_category(TyCategory::Function),
            TspCategory::Function
        );
    }

    #[test]
    fn test_convert_type_info_synthesized_instance() {
        // Instance category without declaration should produce a synthesized type with stub content
        let ty = convert_type_info(
            1,
            TyCategory::Instance,
            "MyClass",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(ty.id, 1);
        // Should be synthesized (has stubContent)
        assert!(ty.details.is_some());
    }

    #[test]
    fn test_convert_type_info_unknown() {
        // Unknown category should produce a bare type (no stub)
        let ty = convert_type_info(
            1,
            TyCategory::Unknown,
            "Unknown",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(ty.id, 1);
        assert!(ty.details.is_none());
    }

    #[test]
    fn test_convert_type_info_declared_instance() {
        use lsp_types::{Position, Range, Url};
        use tsp_types::types::{DeclarationCategory, TypeKind};

        // Instance category with declaration should return ClassType, not synthesized
        let decl = Declaration::regular(
            DeclarationCategory::Class,
            Url::parse("file:///typeshed/stdlib/builtins.pyi").unwrap(),
            Range {
                start: Position {
                    line: 100,
                    character: 0,
                },
                end: Position {
                    line: 100,
                    character: 3,
                },
            },
            Some("int".to_string()),
        );
        let resolved = ResolvedDeclaration {
            declaration: decl,
            name: Some("int".to_string()),
        };
        let ty = convert_type_info(
            1,
            TyCategory::Instance,
            "int",
            None,
            Some(resolved),
            None,
            None,
            None,
        );
        assert_eq!(ty.id, 1);
        assert_eq!(ty.kind, TypeKind::Class);
        assert_eq!(ty.flags, TypeFlags::INSTANCE);
        // Should have a declaration, not stub details
        assert!(ty.declaration.is_some());
        assert!(ty.details.is_none());
    }

    #[test]
    fn test_convert_type_info_declared_function() {
        use lsp_types::{Position, Range, Url};
        use tsp_types::types::{DeclarationCategory, TypeKind};

        let decl = Declaration::regular(
            DeclarationCategory::Function,
            Url::parse("file:///test.pyi").unwrap(),
            Range {
                start: Position {
                    line: 5,
                    character: 4,
                },
                end: Position {
                    line: 5,
                    character: 10,
                },
            },
            Some("my_func".to_string()),
        );
        let resolved = ResolvedDeclaration {
            declaration: decl,
            name: Some("my_func".to_string()),
        };
        let ty = convert_type_info(
            1,
            TyCategory::Function,
            "def my_func(...)",
            None,
            Some(resolved),
            None,
            None,
            None,
        );
        assert_eq!(ty.id, 1);
        assert_eq!(ty.kind, TypeKind::Function);
        assert!(ty.declaration.is_some());
        assert!(ty.details.is_none());
    }

    #[test]
    fn test_convert_type_info_union_with_members() {
        use tsp_types::types::{TypeDetails, TypeKind};

        // Union category with pre-converted member types should produce a union with subTypes
        let member_int = Type::instance(1, "builtins.int");
        let member_str = Type::instance(2, "builtins.str");
        let members = vec![member_int, member_str];

        let ty = convert_type_info(
            3,
            TyCategory::Union,
            "int | str",
            None,
            None,
            Some(members),
            None,
            None,
        );
        assert_eq!(ty.id, 3);
        assert_eq!(ty.kind, TypeKind::Union);
        assert_eq!(ty.display, Some("int | str".to_string()));

        // Should have union details with inline member types
        match &ty.details {
            Some(TypeDetails::Union(union_details)) => {
                assert_eq!(union_details.members.len(), 2);
                assert_eq!(union_details.members[0].id, 1);
                assert_eq!(union_details.members[1].id, 2);
            }
            _ => panic!("Expected Union details"),
        }
    }

    #[test]
    fn test_convert_type_info_union_without_members() {
        use tsp_types::types::TypeKind;

        // Union category without member info falls back to bare type
        let ty = convert_type_info(
            1,
            TyCategory::Union,
            "int | str",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(ty.id, 1);
        assert_eq!(ty.kind, TypeKind::Union);
        // Without union_members, should not have union details
        assert!(ty.details.is_none());
    }

    #[test]
    fn test_convert_type_info_overloaded_with_members() {
        use tsp_types::types::{TypeDetails, TypeKind};

        // Overloaded function with pre-converted overload arms and implementation
        let overload1 = Type::function(1, Some("process".to_string()), None);
        let overload2 = Type::function(2, Some("process".to_string()), None);
        let implementation = Type::function(3, Some("process".to_string()), None);

        let ty = convert_type_info(
            4,
            TyCategory::OverloadedFunction,
            "Overloaded(def process(value: int) -> str, def process(value: str) -> int)",
            None,
            None,
            None,
            Some(vec![overload1, overload2]),
            Some(Box::new(implementation)),
        );
        assert_eq!(ty.id, 4);
        assert_eq!(ty.kind, TypeKind::Overloaded);
        assert_eq!(ty.flags, TypeFlags::CALLABLE);

        // Should have overloaded details with inline overload types
        match &ty.details {
            Some(TypeDetails::Overloaded(overloaded_details)) => {
                assert_eq!(overloaded_details.overloads.len(), 2);
                assert_eq!(overloaded_details.overloads[0].id, 1);
                assert_eq!(overloaded_details.overloads[1].id, 2);
                assert!(overloaded_details.implementation.is_some());
                assert_eq!(overloaded_details.implementation.as_ref().unwrap().id, 3);
            }
            _ => panic!("Expected Overloaded details"),
        }
    }

    #[test]
    fn test_convert_type_info_overloaded_without_implementation() {
        use tsp_types::types::{TypeDetails, TypeKind};

        // Overloaded function without implementation (e.g., Protocol stubs)
        let overload1 = Type::function(1, Some("f".to_string()), None);
        let overload2 = Type::function(2, Some("f".to_string()), None);

        let ty = convert_type_info(
            3,
            TyCategory::OverloadedFunction,
            "Overloaded(def f(x: int) -> str, def f(x: str) -> int)",
            None,
            None,
            None,
            Some(vec![overload1, overload2]),
            None,
        );
        assert_eq!(ty.id, 3);
        assert_eq!(ty.kind, TypeKind::Overloaded);

        match &ty.details {
            Some(TypeDetails::Overloaded(overloaded_details)) => {
                assert_eq!(overloaded_details.overloads.len(), 2);
                assert!(overloaded_details.implementation.is_none());
            }
            _ => panic!("Expected Overloaded details"),
        }
    }

    #[test]
    fn test_convert_category_overloaded() {
        assert_eq!(
            convert_category(TyCategory::OverloadedFunction),
            TspCategory::OverloadedFunction
        );
    }
}
