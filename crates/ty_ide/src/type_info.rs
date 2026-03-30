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
use ty_python_semantic::types::Type;
use ty_python_semantic::types::ide_support::call_signature_details;
use ty_python_semantic::{
    HasType, ImportAliasResolution, SemanticModel, declared_type_for_definition,
    function_overload_info,
};

/// The category of a type, providing a high-level classification.
///
/// This enumeration mirrors common type categories without exposing
/// internal type system details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCategory {
    Unknown,
    Any,
    Never,
    None,
    Class,
    Instance,
    Union,
    Intersection,
    Function,
    BoundMethod,
    Module,
    TypeVar,
    Literal,
    Tuple,
    TypedDict,
    TypeAlias,
    Callable,
    OverloadedFunction,
    SpecialForm,
    Property,
    NewType,
    SubclassOf,
    TypeGuard,
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
pub fn type_info(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;

    let ty = goto_target.inferred_type(&model)?;

    let declaration = goto_target
        .get_definition_targets(&model, ImportAliasResolution::ResolveAliases)
        .and_then(|defs| get_declaration_from_definitions(db, defs, &ty, &model, &goto_target));

    let signature = goto_target.call_type_simplified_by_overloads(&model);

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
pub fn declared_type_info(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;

    let definitions =
        goto_target.get_definition_targets(&model, ImportAliasResolution::ResolveAliases)?;

    let declared_ty = get_declared_type_from_definitions(db, &definitions)?;

    let info = extract_type_info(db, declared_ty, None, None);

    Some(RangedValue {
        range: ruff_db::files::FileRange::new(file, goto_target.range()),
        value: info,
    })
}

/// Get the declared (annotated) type from definitions.
fn get_declared_type_from_definitions<'db>(
    db: &'db dyn Db,
    definitions: &Definitions<'db>,
) -> Option<Type<'db>> {
    for resolved_def in &definitions.0 {
        if let Some(definition) = resolved_def.definition() {
            if let Some(ty) = declared_type_for_definition(db, definition) {
                return Some(ty);
            }
        }
    }
    None
}

/// Get the expected/contextual type at a specific position in a file.
///
/// Returns the type that the expression at this position is expected to have,
/// based on its context. For example:
/// - `x: int = <expr>` → expected type for `<expr>` is `int`
/// - `foo(<expr>)` where `foo(x: int)` → expected type is `int`
/// - `return <expr>` in a function with `-> int` → expected type is `int`
pub fn expected_type_info(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<TypeInfo>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let tokens = parsed.tokens();

    let token = tokens
        .at_offset(offset)
        .max_by_key(|t| i32::from(t.kind() == TokenKind::Name))?;

    let covering = covering_node(parsed.syntax().into(), token.range())
        .find_first(AnyNodeRef::is_expression)
        .ok()?;

    let expr_node = covering.node();
    let expr_range = expr_node.range();

    let expected_ty = find_expected_type_from_context(db, &model, &covering)?;

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

    for ancestor in covering.ancestors() {
        match ancestor {
            // Case 1: Annotated assignment - `x: int = <expr>`
            AnyNodeRef::StmtAnnAssign(ann_assign) => {
                if let Some(value) = &ann_assign.value {
                    if value.range().contains_range(expr_node.range()) {
                        return ann_assign.annotation.inferred_type(model);
                    }
                }
            }

            // Case 2: Function call - `foo(<expr>)` where foo expects a certain type
            AnyNodeRef::ExprCall(call_expr) => {
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
                        if let Some(return_ty) =
                            find_enclosing_function_return_type(model, covering)
                        {
                            return Some(return_ty);
                        }
                    }
                }
            }

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
    let mut arg_index = None;

    for (i, arg) in call_expr.arguments.args.iter().enumerate() {
        if arg.range().contains_range(expr_node.range()) {
            arg_index = Some(i);
            break;
        }
    }

    let arg_index = arg_index?;

    let signature_details = call_signature_details(model, call_expr);
    let signature = signature_details.first()?;

    let arg_mapping = signature.argument_to_parameter_mapping.get(arg_index)?;

    if !arg_mapping.matched {
        return None;
    }

    let param_index = arg_mapping.parameters.first()?;

    signature.parameter_types.get(*param_index).copied()
}

/// Find the return type of the enclosing function.
fn find_enclosing_function_return_type<'db>(
    model: &SemanticModel<'db>,
    covering: &ruff_python_ast::find_node::CoveringNode<'_>,
) -> Option<Type<'db>> {
    for ancestor in covering.ancestors() {
        if let AnyNodeRef::StmtFunctionDef(func_def) = ancestor {
            if let Some(returns) = &func_def.returns {
                return returns.inferred_type(model);
            }
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
    let category = determine_category(&ty);
    let display = ty.display(db).to_string();

    let (union_member_count, literal_value) = extract_type_details(db, &ty);

    let union_members = if let Type::Union(union) = &ty {
        let members: Vec<TypeInfo> = union
            .elements(db)
            .iter()
            .map(|member_ty| extract_type_info(db, *member_ty, None, None))
            .collect();
        Some(members)
    } else {
        None
    };

    let (overload_members, implementation_member) = match function_overload_info(db, ty) {
        Some(info) => {
            let overloads: Vec<TypeInfo> = info
                .overloads
                .into_iter()
                .map(|detail| TypeInfo {
                    category: TypeCategory::Function,
                    display: detail.display,
                    declaration: None,
                    type_definition: None,
                    union_member_count: None,
                    union_members: None,
                    overload_members: None,
                    implementation_member: None,
                    literal_value: None,
                    signature: None,
                })
                .collect();
            let implementation = info.implementation.map(|detail| {
                Box::new(TypeInfo {
                    category: TypeCategory::Function,
                    display: detail.display,
                    declaration: None,
                    type_definition: None,
                    union_member_count: None,
                    union_members: None,
                    overload_members: None,
                    implementation_member: None,
                    literal_value: None,
                    signature: None,
                })
            });
            (Some(overloads), implementation)
        }
        None => (None, None),
    };

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

/// Determine the category of a type.
fn determine_category(ty: &Type<'_>) -> TypeCategory {
    match ty {
        Type::Dynamic(dyn_type) => match dyn_type {
            ty_python_semantic::types::DynamicType::Any => TypeCategory::Any,
            ty_python_semantic::types::DynamicType::Unknown => TypeCategory::Unknown,
            _ => TypeCategory::Dynamic,
        },
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
    type_def: &ty_python_semantic::types::TypeDefinition<'_>,
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
        def func(a: int) -> str:
            return ""

        fun<CURSOR>c
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
        assert!(
            info.is_some(),
            "declared_type_info should return Some for annotated variable"
        );
        let info = info.unwrap();
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

        let info = declared_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_none());
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
        assert!(
            info.value.type_definition.is_some(),
            "type_definition should be populated for class instances"
        );
    }

    #[test]
    fn test_declared_type_info_parameter() {
        let test = cursor_test(
            r#"
        def foo(x<CURSOR>: str):
            pass
        "#,
        );

        let info = declared_type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(
            info.is_some(),
            "declared_type_info should return Some for annotated parameter"
        );
        let info = info.unwrap();
        assert!(info.value.display.contains("str"));
    }

    #[test]
    fn test_overloaded_function_decomposition() {
        let test = cursor_test(
            r#"
        from typing import overload

        @overload
        def func(x: int) -> int: ...
        @overload
        def func(x: str) -> str: ...
        def func(x):
            return x

        fun<CURSOR>c
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Function);
        assert!(
            info.value.overload_members.is_some(),
            "overload_members should be populated for overloaded functions"
        );
        let overloads = info.value.overload_members.unwrap();
        assert_eq!(overloads.len(), 2, "should have 2 @overload signatures");
        assert!(
            info.value.implementation_member.is_some(),
            "implementation_member should be populated when an implementation exists"
        );
    }

    #[test]
    fn test_non_overloaded_function_has_no_overloads() {
        let test = cursor_test(
            r#"
        def bar(x: int) -> int:
            return x

        ba<CURSOR>r
        "#,
        );

        let info = type_info(&test.db, test.cursor.file, test.cursor.offset);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.value.category, TypeCategory::Function);
        assert!(
            info.value.overload_members.is_none(),
            "non-overloaded function should have no overload_members"
        );
        assert!(
            info.value.implementation_member.is_none(),
            "non-overloaded function should have no implementation_member"
        );
    }
}
