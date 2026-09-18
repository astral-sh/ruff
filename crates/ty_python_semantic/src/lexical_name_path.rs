use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, name::Name};
use tracing::trace;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::NodeWithScopeKind;
use ty_python_core::semantic_index;

use crate::Db;

/// Returns the module-relative lexical name path to `definition`.
///
/// For example, the path to `method` here is `["Outer", "method"]`:
/// ```python
/// class Outer:
///     def method(): ...
/// ```
pub(crate) fn lexical_name_path_for_definition(
    db: &dyn Db,
    definition: Definition,
) -> Option<Vec<Name>> {
    let parsed = parsed_module(db, definition.python_file(db));
    let module = parsed.load(db);

    let mut path = vec![
        lexical_name_path_component_for_leaf(db, &module, definition)
            .map_err(|()| {
                trace!("Found unsupported DefinitionKind for lexical name path");
            })
            .ok()?,
    ];

    let index = semantic_index(db, definition.program_file(db));
    for (_scope_id, scope) in index.ancestor_scopes(definition.file_scope(db)) {
        let component = lexical_name_path_component_for_node(&module, scope.node())
            .map_err(|()| {
                trace!("Found unsupported NodeScopeKind for lexical name path");
            })
            .ok()?;
        if let Some(component) = component {
            path.push(component);
        }
    }

    path.reverse();
    Some(path)
}

/// Computes a lexical name path component for an enclosing scope.
///
/// See [`lexical_name_path_for_definition`][] for details.
pub(crate) fn lexical_name_path_component_for_node(
    parsed: &ParsedModuleRef,
    node: &NodeWithScopeKind,
) -> Result<Option<Name>, ()> {
    let component = match node {
        NodeWithScopeKind::Module => {
            // This is just implicit, so has no component
            return Ok(None);
        }
        NodeWithScopeKind::Class(class) => class.node(parsed).name.id.clone(),
        NodeWithScopeKind::Function(func) => func.node(parsed).name.id.clone(),
        NodeWithScopeKind::TypeAlias(_)
        | NodeWithScopeKind::ClassTypeParameters(_)
        | NodeWithScopeKind::FunctionTypeParameters(_)
        | NodeWithScopeKind::TypeAliasTypeParameters(_)
        | NodeWithScopeKind::Lambda(_)
        | NodeWithScopeKind::ListComprehension(_)
        | NodeWithScopeKind::SetComprehension(_)
        | NodeWithScopeKind::DictComprehension(_)
        | NodeWithScopeKind::GeneratorExpression(_) => {
            // Not yet implemented
            return Err(());
        }
    };
    Ok(Some(component))
}

/// Computes the final component of a lexical name path.
///
/// See [`lexical_name_path_for_definition`][] for details.
fn lexical_name_path_component_for_leaf(
    db: &dyn Db,
    parsed: &ParsedModuleRef,
    definition: Definition,
) -> Result<Name, ()> {
    let component = match definition.kind(db) {
        DefinitionKind::Function(func) => func.node(parsed).name.id.clone(),
        DefinitionKind::Class(class) => class.node(parsed).name.id.clone(),
        DefinitionKind::Assignment(assignment) => {
            let ast::Expr::Name(name) = assignment.target(parsed) else {
                return Err(());
            };
            name.id.clone()
        }
        DefinitionKind::AnnotatedAssignment(assignment) => {
            let ast::Expr::Name(name) = assignment.target(parsed) else {
                return Err(());
            };
            name.id.clone()
        }
        DefinitionKind::TypeAlias(_)
        | DefinitionKind::Import(_)
        | DefinitionKind::ImportFrom(_)
        | DefinitionKind::ImportFromSubmodule(_)
        | DefinitionKind::StarImport(_)
        | DefinitionKind::NamedExpression(_)
        | DefinitionKind::AugmentedAssignment(_)
        | DefinitionKind::DictKeyAssignment(_)
        | DefinitionKind::For(_)
        | DefinitionKind::Comprehension(_)
        | DefinitionKind::Parameter(_)
        | DefinitionKind::LambdaParameter { .. }
        | DefinitionKind::WithItem(_)
        | DefinitionKind::MatchPattern(_)
        | DefinitionKind::ExceptHandler(_)
        | DefinitionKind::TypeVar(_)
        | DefinitionKind::ParamSpec(_)
        | DefinitionKind::TypeVarTuple(_)
        | DefinitionKind::LoopHeader(_)
        | DefinitionKind::NestedBindings(_) => {
            // Not yet implemented
            return Err(());
        }
    };

    Ok(component)
}
