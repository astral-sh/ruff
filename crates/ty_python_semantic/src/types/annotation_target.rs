use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ty_python_core::place::{PlaceExpr, ScopedPlaceId};
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use crate::Db;
use crate::types::infer::{infer_definition_types, nearest_enclosing_function};

/// Returns whether a variable annotation is allowed on this assignment target.
/// Names and attributes of an implicit method receiver (such as `self.x` or `cls.x`) can
/// introduce a declaration.
pub fn is_valid_annotation_target(db: &dyn Db, file: ProgramFile, target: ast::Expr<'_>) -> bool {
    let Some(attribute) = target.as_attribute_expr() else {
        return target.is_name_expr();
    };
    let index = semantic_index(db, file);
    let scope_id = index.expression_scope_id(&target);
    let scope = scope_id.to_scope_id(db, file);
    if is_instance_attribute_assignment(db, index, scope, attribute) {
        return true;
    }

    // A nested function can also annotate an attribute on a captured method receiver.
    attribute.value().as_name_expr().is_some_and(|receiver| {
        receiver_method_scope(db, index, scope, receiver)
            .is_some_and(|receiver_scope_id| receiver_scope_id != scope_id)
    })
}

/// The index marks attributes on the first parameter of the enclosing method, provided the
/// parameter is visible through any intervening scopes. Static methods also have a first
/// parameter, so check whether the enclosing function has an implicit receiver.
pub(super) fn is_instance_attribute_assignment(
    db: &dyn Db,
    index: &SemanticIndex,
    scope: ScopeId,
    target: ast::ExprAttribute<'_>,
) -> bool {
    let Some(place_expr) = PlaceExpr::try_from_expr(target) else {
        return false;
    };
    let place_table = index.place_table(scope.file_scope_id(db));
    let Some(ScopedPlaceId::Member(member_id)) = place_table.place_id(&place_expr) else {
        return false;
    };
    place_table.member(member_id).is_instance_attribute()
        && nearest_enclosing_function(db, index, scope)
            .is_some_and(|function| function.has_implicit_receiver(db))
}

/// Resolve the method scope that defines an implicit receiver referenced by the current scope.
///
/// Returns `None` if the name is shadowed, resolves globally, or belongs to a function that is
/// not a method with an implicit receiver.
///
/// ```python
/// class C:
///     def method(self):
///         def inner():
///             self.attribute = 1
/// ```
pub(super) fn receiver_method_scope(
    db: &dyn Db,
    index: &SemanticIndex,
    scope: ScopeId,
    receiver: ast::ExprName<'_>,
) -> Option<FileScopeId> {
    let receiver_name = receiver.id().as_str();
    let (receiver_scope_id, receiver_scope, receiver_symbol) = index
        .visible_ancestor_scopes(scope.file_scope_id(db))
        .find_map(|(scope_id, scope)| {
            index
                .place_table(scope_id)
                .symbol_by_name(receiver_name)
                .filter(|symbol| symbol.is_local() || symbol.is_global())
                .map(|symbol| (scope_id, scope, symbol))
        })?;
    if receiver_symbol.is_global() {
        return None;
    }
    let function = receiver_scope.node().as_function()?;
    index.class_definition_of_method(receiver_scope_id)?;
    let module = parsed_module(db, scope.python_file(db)).load(db);
    let first_parameter = function
        .node(&module)
        .parameters()
        .iter_non_variadic_params()
        .next()?;
    if first_parameter.name() != receiver_name {
        return None;
    }
    let definition = index.expect_single_definition(function);
    infer_definition_types(db, definition)
        .function_type(definition)?
        .has_implicit_receiver(db)
        .then_some(receiver_scope_id)
}
