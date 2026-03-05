//! External API for type information.
//!
//! This module provides a clean external interface for obtaining type information
//! at a specific position in a Python file. It's designed for use by external consumers
//! like language server protocol implementations (e.g., TSP) without exposing
//! internal ty type system details.

use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::{Db, NavigationTarget, RangedValue};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::types::ide_support::call_signature_details;
use ty_python_semantic::types::{Type, TypeDefinition};
use ty_python_semantic::{HasType, ImportAliasResolution, SemanticModel};

/// The category of a type, providing a high-level classification.
///
/// This enumeration mirrors common type categories without exposing
/// internal type system details.
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
    /// An intersection of types.
    Intersection,
    /// A function type.
    Function,
    /// A bound method.
    BoundMethod,
    /// A module type.
    Module,
    /// A type variable.
    TypeVar,
    /// A literal type (int, bool, string, bytes, enum).
    Literal,
    /// A tuple type.
    Tuple,
    /// A `TypedDict` type.
    TypedDict,
    /// A type alias.
    TypeAlias,
    /// A callable type.
    Callable,
    /// An overloaded function (has multiple @overload signatures).
    OverloadedFunction,
    /// A special form (typing constructs like Protocol, Final, etc.).
    SpecialForm,
    /// A property instance.
    Property,
    /// A `NewType` instance.
    NewType,
    /// A subclass-of type (type[C]).
    SubclassOf,
    /// A type guard (`TypeGuard` or `TypeIs`).
    TypeGuard,
    /// A dynamic type variant not covered by other categories.
    Dynamic,
}

/// Information about the declaration location of a type.
#[derive(Debug, Clone)]
pub struct DeclarationInfo {
    /// The navigation target for the declaration.
    pub target: NavigationTarget,
}

/// Structured information about a type at a specific position.
///
/// This struct provides all the information external consumers typically need
/// without exposing internal ty types.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// The high-level category of this type.
    pub category: TypeCategory,
    /// Human-readable display string for the type.
    pub display: String,
    /// Optional declaration location for the symbol at the cursor position.
    pub declaration: Option<DeclarationInfo>,
    /// Optional declaration location for the TYPE itself (e.g., class definition).
    /// For `x = A()`, `declaration` points to `x`, while `type_definition` points to `class A:`.
    pub type_definition: Option<DeclarationInfo>,
    /// For union types, the number of members.
    pub union_member_count: Option<usize>,
    /// For union types, the `TypeInfo` for each member type.
    /// This allows external consumers to decompose unions into their constituent types
    /// without needing access to ty's internal `Type<'db>` values.
    pub union_members: Option<Vec<TypeInfo>>,
    /// For overloaded functions, the `TypeInfo` for each `@overload` signature.
    pub overload_members: Option<Vec<TypeInfo>>,
    /// For overloaded functions, the `TypeInfo` for the implementation (non-`@overload` def).
    pub implementation_member: Option<Box<TypeInfo>>,
    /// For literal types, the literal value as a string.
    pub literal_value: Option<String>,
    /// For function types, the simplified signature if available.
    pub signature: Option<String>,
}

/// Get type information at a specific position in a file.
///
/// This is the main entry point for external consumers who need type information
/// at a cursor position.
///
/// # Arguments
///
/// * `db` - The database.
/// * `file` - The file to analyze.
/// * `offset` - The byte offset in the file.
///
/// # Returns
///
/// Returns `Some(RangedValue<TypeInfo>)` if type information is available at the position,
/// or `None` if no type information could be determined.
pub fn type_info(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;

    // Get the inferred type
    let ty = goto_target.inferred_type(&model)?;

    // Get declaration info if available
    let declaration = goto_target
        .get_definition_targets(&model, ImportAliasResolution::ResolveAliases)
        .and_then(|defs| get_declaration_from_definitions(db, defs, &ty, &model, &goto_target));

    // Try to get a simplified signature for callables
    let signature = goto_target.call_type_simplified_by_overloads(&model);

    // Extract type information
    let info = extract_type_info(db, ty, declaration, signature);

    Some(RangedValue {
        range: ruff_db::files::FileRange::new(file, goto_target.range()),
        value: info,
    })
}

/// Get the declared (annotated) type at a specific position in a file.
///
/// This returns the type from an explicit type annotation, if present.
/// For example:
/// - `x: int = 1` → returns `int` (not `Literal[1]`)
/// - `def foo(x: str)` → for parameter `x`, returns `str`
///
/// # Arguments
///
/// * `db` - The database.
/// * `file` - The file to analyze.
/// * `offset` - The byte offset in the file.
///
/// # Returns
///
/// Returns `Some(RangedValue<TypeInfo>)` if a declared type is available at the position,
/// or `None` if there is no explicit type annotation.
pub fn declared_type_info(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;

    // Get definitions for the target
    let definitions =
        goto_target.get_definition_targets(&model, ImportAliasResolution::ResolveAliases)?;

    // Try to get a declared type from the definitions
    let declared_ty = get_declared_type_from_definitions(db, &definitions)?;

    // Extract type information
    let info = extract_type_info(db, declared_ty, None, None);

    Some(RangedValue {
        range: ruff_db::files::FileRange::new(file, goto_target.range()),
        value: info,
    })
}

/// Get the declared (annotated) type from definitions.
///
/// TODO: Upstream removed `declaration_type_for_definition`; `declaration_type` is now `pub(crate)`.
/// Re-enable this when a public API for declared types becomes available.
fn get_declared_type_from_definitions<'db>(
    _db: &'db dyn Db,
    _definitions: &Definitions<'db>,
) -> Option<Type<'db>> {
    // Temporarily disabled: the upstream API we relied on
    // (declaration_type_for_definition) was removed.
    None
}

/// Get the expected/contextual type at a specific position in a file.
///
/// Returns the type that the expression at this position is expected to have,
/// based on its context. For example:
/// - `x: int = <expr>` → expected type for `<expr>` is `int`
/// - `foo(<expr>)` where `foo(x: int)` → expected type is `int`
/// - `return <expr>` in a function with `-> int` → expected type is `int`
///
/// # Arguments
///
/// * `db` - The database.
/// * `file` - The file to analyze.
/// * `offset` - The byte offset in the file.
///
/// # Returns
///
/// Returns `Some(RangedValue<TypeInfo>)` if an expected type can be determined,
/// or `None` if there is no contextual type expectation.
pub fn expected_type_info(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let tokens = parsed.tokens();

    // Find the token at the offset
    let token = tokens
        .at_offset(offset)
        .max_by_key(|t| i32::from(t.kind() == TokenKind::Name))?;

    // Get the covering node for the token
    let covering = covering_node(parsed.syntax().into(), token.range())
        .find_first(ruff_python_ast::AnyNodeRef::is_expression)
        .ok()?;

    let expr_node = covering.node();
    let expr_range = expr_node.range();

    // Walk up ancestors to find a context that provides an expected type
    let expected_ty = find_expected_type_from_context(db, &model, &covering)?;

    // Extract type information
    let info = extract_type_info(db, expected_ty, None, None);

    Some(RangedValue {
        range: ruff_db::files::FileRange::new(file, expr_range),
        value: info,
    })
}

/// Find the expected type from the AST context of an expression.
fn find_expected_type_from_context<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    covering: &ruff_python_ast::find_node::CoveringNode<'_>,
) -> Option<Type<'db>> {
    let expr_node = covering.node();

    // Walk up through ancestors to find a context that provides an expected type
    for ancestor in covering.ancestors() {
        match ancestor {
            // Case 1: Annotated assignment - `x: int = <expr>`
            AnyNodeRef::StmtAnnAssign(ann_assign) => {
                // Check if our expression is the value (RHS)
                if let Some(value) = &ann_assign.value {
                    if value.range().contains_range(expr_node.range()) {
                        // The expected type is the annotation
                        return ann_assign.annotation.inferred_type(model);
                    }
                }
            }

            // Case 2: Function call - `foo(<expr>)` where foo expects a certain type
            AnyNodeRef::ExprCall(call_expr) => {
                // Check if our expression is one of the arguments
                if let Some(expected_ty) =
                    find_expected_type_for_argument(db, model, call_expr, expr_node)
                {
                    return Some(expected_ty);
                }
            }

            // Case 3: Return statement - `return <expr>` in a function with return type
            AnyNodeRef::StmtReturn(return_stmt) => {
                if let Some(value) = &return_stmt.value {
                    if value.range().contains_range(expr_node.range()) {
                        // Find the enclosing function to get its return type
                        if let Some(return_ty) =
                            find_enclosing_function_return_type(model, covering)
                        {
                            return Some(return_ty);
                        }
                    }
                }
            }

            // Case 4: Assignment (without annotation but with type hint on target elsewhere)
            // This is less common - we'd need to find the original declaration of the target
            // For now, we skip this case
            _ => continue,
        }
    }

    None
}

/// Find the expected type for an argument in a function call.
fn find_expected_type_for_argument<'db>(
    _db: &'db dyn Db,
    model: &SemanticModel<'db>,
    call_expr: &ruff_python_ast::ExprCall,
    expr_node: AnyNodeRef<'_>,
) -> Option<Type<'db>> {
    // Find which argument position our expression is at
    let mut arg_index = None;

    for (i, arg) in call_expr.arguments.args.iter().enumerate() {
        if arg.range().contains_range(expr_node.range()) {
            arg_index = Some(i);
            break;
        }
    }

    let arg_index = arg_index?;

    // Get the call signature details
    let signature_details = call_signature_details(model, call_expr);
    let signature = signature_details.first()?;

    // Find the parameter that this argument maps to
    let arg_mapping = signature.argument_to_parameter_mapping.get(arg_index)?;

    if !arg_mapping.matched {
        return None;
    }

    // Get the first parameter index this argument maps to
    let param_index = arg_mapping.parameters.first()?;

    // Get the parameter type
    signature.parameter_types.get(*param_index).copied()
}

/// Find the return type of the enclosing function.
fn find_enclosing_function_return_type<'db>(
    model: &SemanticModel<'db>,
    covering: &ruff_python_ast::find_node::CoveringNode<'_>,
) -> Option<Type<'db>> {
    // Walk up ancestors to find an enclosing function definition
    for ancestor in covering.ancestors() {
        if let AnyNodeRef::StmtFunctionDef(func_def) = ancestor {
            // Get the return annotation if present
            if let Some(returns) = &func_def.returns {
                return returns.inferred_type(model);
            }
            // No return annotation on this function
            return None;
        }
    }

    None
}

/// Extract structured type information from a Type.
fn extract_type_info(
    db: &dyn Db,
    ty: Type<'_>,
    declaration: Option<DeclarationInfo>,
    signature: Option<String>,
) -> TypeInfo {
    use ty_python_semantic::DisplaySettings;

    let category = determine_category(&ty);
    let display = ty.display_with(db, DisplaySettings::default()).to_string();

    // Extract additional details based on type
    let (union_member_count, literal_value) = extract_type_details(db, &ty);

    // For union types, extract TypeInfo for each member so external consumers
    // can decompose the union without access to ty's internal types.
    let union_members = if let Type::Union(union) = &ty {
        let members: Vec<TypeInfo> = union
            .elements(db)
            .iter()
            .map(|member_ty| {
                // Each member gets its own TypeInfo with type_definition resolved.
                // We don't pass declaration or signature since these are sub-types,
                // not cursor-position symbols.
                extract_type_info(db, *member_ty, None, None)
            })
            .collect();
        Some(members)
    } else {
        None
    };

    // For overloaded functions, decompose into individual @overload arms
    // and an optional implementation entry.
    // TODO: Upstream removed `function_overload_info` / `FunctionOverloads`.
    // Re-enable when a public API for overload decomposition becomes available.
    let (overload_members, implementation_member): (Option<Vec<TypeInfo>>, Option<Box<TypeInfo>>) =
        (None, None);

    // Get the type's own definition (e.g., class A's definition for instances of A).
    // This is different from `declaration` which points to the symbol at the cursor.
    let type_definition = ty
        .definition(db)
        .and_then(|type_def| get_navigation_target_from_type_definition(db, &type_def))
        .map(|target| DeclarationInfo { target });

    TypeInfo {
        category,
        display,
        declaration,
        type_definition,
        union_member_count,
        union_members,
        overload_members,
        implementation_member,
        literal_value,
        signature,
    }
}

// TODO: Upstream removed `function_overload_info` / `FunctionOverloads`.
// The `extract_overload_members` helper has been temporarily removed.
// Re-enable when a public API for overload decomposition becomes available.

/// Determine the category of a type.
fn determine_category(ty: &Type<'_>) -> TypeCategory {
    match ty {
        Type::Dynamic(dyn_type) => {
            use ty_python_semantic::types::DynamicType;
            match dyn_type {
                DynamicType::Any => TypeCategory::Any,
                DynamicType::Unknown => TypeCategory::Unknown,
                _ => TypeCategory::Dynamic,
            }
        }
        Type::Never => TypeCategory::Never,
        Type::FunctionLiteral(_) => TypeCategory::Function,
        Type::BoundMethod(_) | Type::KnownBoundMethod(_) => TypeCategory::BoundMethod,
        Type::WrapperDescriptor(_) => TypeCategory::Function,
        Type::DataclassDecorator(_) | Type::DataclassTransformer(_) => TypeCategory::Callable,
        Type::Callable(_) => TypeCategory::Callable,
        Type::ModuleLiteral(_) => TypeCategory::Module,
        Type::ClassLiteral(_) | Type::GenericAlias(_) => TypeCategory::Class,
        Type::SubclassOf(_) => TypeCategory::SubclassOf,
        Type::NominalInstance(_) | Type::ProtocolInstance(_) => TypeCategory::Instance,
        Type::SpecialForm(_) | Type::KnownInstance(_) => TypeCategory::SpecialForm,
        Type::PropertyInstance(_) => TypeCategory::Property,
        Type::Union(_) => TypeCategory::Union,
        Type::Intersection(_) => TypeCategory::Intersection,
        Type::AlwaysTruthy | Type::AlwaysFalsy => TypeCategory::Instance,
        Type::LiteralValue(_) => TypeCategory::Literal,
        Type::TypeVar(_) => TypeCategory::TypeVar,
        Type::BoundSuper(_) => TypeCategory::Instance,
        Type::TypeIs(_) | Type::TypeGuard(_) => TypeCategory::TypeGuard,
        Type::TypedDict(_) => TypeCategory::TypedDict,
        Type::TypeAlias(_) => TypeCategory::TypeAlias,
        Type::NewTypeInstance(_) => TypeCategory::NewType,
    }
}

/// Extract additional type details for specific type categories.
fn extract_type_details(db: &dyn Db, ty: &Type<'_>) -> (Option<usize>, Option<String>) {
    match ty {
        Type::Union(union) => {
            let count = union.elements(db).len();
            (Some(count), None)
        }
        Type::LiteralValue(_) => {
            // Literal value details are included in the type's display string
            // e.g., Literal[42], Literal[True], Literal["hello"]
            (None, None)
        }
        _ => (None, None),
    }
}

/// Get declaration info from definitions.
fn get_declaration_from_definitions<'db>(
    db: &'db dyn Db,
    definitions: Definitions<'db>,
    ty: &Type<'db>,
    model: &SemanticModel<'db>,
    goto_target: &GotoTarget<'_>,
) -> Option<DeclarationInfo> {
    // First try to get navigation targets from the definitions
    if let Some(targets) = definitions.declaration_targets(model, goto_target) {
        if let Some(target) = targets.into_iter().next() {
            return Some(DeclarationInfo { target });
        }
    }

    // Fallback: try to get definition from the type itself
    if let Some(type_def) = ty.definition(db) {
        if let Some(target) = get_navigation_target_from_type_definition(db, &type_def) {
            return Some(DeclarationInfo { target });
        }
    }

    None
}

/// Convert a `TypeDefinition` to a `NavigationTarget`.
fn get_navigation_target_from_type_definition(
    db: &dyn Db,
    type_def: &TypeDefinition<'_>,
) -> Option<NavigationTarget> {
    use crate::HasNavigationTargets;
    let targets = type_def.navigation_targets(db);
    targets.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cursor_test;

    #[test]
    fn test_type_info_basic() {
        let test = cursor_test(
            r#"
        x = 10
        x<CURSOR>
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Literal);
        assert!(info.value.display.contains("10"));
    }

    #[test]
    fn test_type_info_function() {
        let test = cursor_test(
            r#"
        def foo(a: int) -> str:
            return ""

        fo<CURSOR>o
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Function);
    }

    #[test]
    fn test_type_info_class() {
        let test = cursor_test(
            r#"
        class MyClass:
            pass

        MyCla<CURSOR>ss
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Class);
    }

    #[test]
    fn test_type_info_union() {
        let test = cursor_test(
            r#"
        def foo(x: int | str):
            return x<CURSOR>
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Union);
        assert!(info.value.union_member_count.is_some());
    }

    #[test]
    fn test_type_info_instance() {
        let test = cursor_test(
            r#"
        x: int = 10
        x<CURSOR>
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // With explicit annotation, we get int instance, not literal
        assert!(
            info.value.category == TypeCategory::Instance
                || info.value.category == TypeCategory::Literal
        );
    }

    #[test]
    fn test_type_info_none() {
        let test = cursor_test(
            r#"
        x = None
        x<CURSOR>
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // None type or Instance(None)
        assert!(info.value.display.contains("None"));
    }

    #[test]
    fn test_type_info_no_type() {
        let test = cursor_test(
            r#"
        <CURSOR>
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_none());
    }

    #[test]
    fn test_declared_type_info_with_annotation() {
        let test = cursor_test(
            r#"
        x: int = 1
        x<CURSOR>
        "#,
        );

        let info = declared_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // Should return the declared type (int), not the inferred type (Literal[1])
        assert_eq!(info.value.category, TypeCategory::Instance);
        assert!(info.value.display.contains("int"));
    }

    #[test]
    fn test_declared_type_info_no_annotation() {
        let test = cursor_test(
            r#"
        x = 1
        x<CURSOR>
        "#,
        );

        // No explicit annotation, so declared_type_info should return None
        let info = declared_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_none());
    }

    #[test]
    fn test_declared_type_info_parameter() {
        let test = cursor_test(
            r#"
        def foo(x: str):
            x<CURSOR>
        "#,
        );

        let info = declared_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Instance);
        assert!(info.value.display.contains("str"));
    }

    #[test]
    fn test_expected_type_info_annotated_assignment() {
        let test = cursor_test(
            r#"
        x: int = 1<CURSOR>0
        "#,
        );

        let info = expected_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // The expected type should be `int` from the annotation
        assert_eq!(info.value.category, TypeCategory::Instance);
        assert!(info.value.display.contains("int"));
    }

    #[test]
    fn test_expected_type_info_function_argument() {
        let test = cursor_test(
            r#"
        def foo(x: str):
            pass

        foo("hel<CURSOR>lo")
        "#,
        );

        let info = expected_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // The expected type should be `str` from the parameter annotation
        assert_eq!(info.value.category, TypeCategory::Instance);
        assert!(info.value.display.contains("str"));
    }

    #[test]
    fn test_expected_type_info_return_statement() {
        let test = cursor_test(
            r#"
        def foo() -> int:
            return 4<CURSOR>2
        "#,
        );

        let info = expected_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        // The expected type should be `int` from the return type annotation
        assert_eq!(info.value.category, TypeCategory::Instance);
        assert!(info.value.display.contains("int"));
    }

    #[test]
    fn test_expected_type_info_no_context() {
        let test = cursor_test(
            r#"
        x = 1<CURSOR>0
        "#,
        );

        // No type annotation, no contextual expectation
        let info = expected_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_none());
    }

    #[test]
    fn test_type_definition_for_class_instance() {
        let test = cursor_test(
            r#"
        class MyClass:
            pass

        x = MyClass()
        <CURSOR>x
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Instance);
        // type_definition should point to class MyClass, not variable x
        assert!(
            info.value.type_definition.is_some(),
            "type_definition should be populated for class instances"
        );
    }

    #[test]
    fn test_type_info_overloaded_function() {
        let test = cursor_test(
            r#"
from typing import overload

@overload
def process(value: int) -> str: ...
@overload
def process(value: str) -> int: ...
def process(value: int | str) -> int | str:
    if isinstance(value, int):
        return str(value)
    return len(value)

<CURSOR>process
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(
            info.is_some(),
            "type_info should return Some for an overloaded function"
        );
        let info = info.unwrap();

        // Top-level category should be OverloadedFunction
        assert_eq!(
            info.value.category,
            TypeCategory::OverloadedFunction,
            "Expected OverloadedFunction category, got {:?}",
            info.value.category
        );

        // Should have overload_members with the @overload arms
        assert!(
            info.value.overload_members.is_some(),
            "overload_members should be populated"
        );
        let overloads = info.value.overload_members.as_ref().unwrap();
        assert_eq!(
            overloads.len(),
            2,
            "Expected 2 @overload arms, got {}",
            overloads.len()
        );

        // Each overload arm should be a Function category
        for (i, arm) in overloads.iter().enumerate() {
            assert_eq!(
                arm.category,
                TypeCategory::Function,
                "Overload arm {} should be Function category",
                i
            );
            assert!(
                arm.display.starts_with("def process"),
                "Overload arm {} display should start with 'def process', got: {}",
                i,
                arm.display
            );
            // Each arm should have a type_definition pointing to its source
            assert!(
                arm.type_definition.is_some(),
                "Overload arm {} should have a type_definition",
                i
            );
        }

        // Should have an implementation member
        assert!(
            info.value.implementation_member.is_some(),
            "implementation_member should be populated"
        );
        let impl_member = info.value.implementation_member.as_ref().unwrap();
        assert_eq!(
            impl_member.category,
            TypeCategory::Function,
            "Implementation should be Function category"
        );
        assert!(
            impl_member.display.starts_with("def process"),
            "Implementation display should start with 'def process', got: {}",
            impl_member.display
        );
    }

    #[test]
    fn test_non_overloaded_function_has_no_overload_members() {
        let test = cursor_test(
            r#"
def foo(x: int) -> str:
    return str(x)

<CURSOR>foo
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Function);
        assert!(
            info.value.overload_members.is_none(),
            "Non-overloaded function should not have overload_members"
        );
        assert!(
            info.value.implementation_member.is_none(),
            "Non-overloaded function should not have implementation_member"
        );
    }

    #[test]
    fn test_overloaded_function_without_implementation() {
        let test = cursor_test(
            r#"
from typing import overload

@overload
def convert(x: int) -> str: ...
@overload
def convert(x: str) -> int: ...

<CURSOR>convert
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some(), "type_info should return Some");
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::OverloadedFunction);

        let overloads = info
            .value
            .overload_members
            .as_ref()
            .expect("should have overloads");
        assert_eq!(overloads.len(), 2, "Expected 2 @overload arms");

        // Without a concrete implementation def, implementation_member should be None
        assert!(
            info.value.implementation_member.is_none(),
            "Overloaded function without implementation body should have no implementation_member"
        );
    }

    #[test]
    fn test_overloaded_function_display_strings() {
        let test = cursor_test(
            r#"
from typing import overload

@overload
def add(x: int, y: int) -> int: ...
@overload
def add(x: str, y: str) -> str: ...
def add(x, y):
    return x + y

<CURSOR>add
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();

        let overloads = info
            .value
            .overload_members
            .as_ref()
            .expect("should have overloads");
        assert_eq!(overloads.len(), 2);

        // Verify each arm has a distinct display string with the correct signature
        assert!(
            overloads[0].display.contains("int") && overloads[0].display.contains("int"),
            "First arm should contain 'int' types, got: {}",
            overloads[0].display
        );
        assert!(
            overloads[1].display.contains("str") && overloads[1].display.contains("str"),
            "Second arm should contain 'str' types, got: {}",
            overloads[1].display
        );

        // Implementation should exist
        let impl_member = info
            .value
            .implementation_member
            .as_ref()
            .expect("should have implementation");
        assert!(
            impl_member.display.starts_with("def add"),
            "Implementation display should start with 'def add', got: {}",
            impl_member.display
        );
    }
}
