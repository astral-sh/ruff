use ruff_python_ast as ast;
use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::place::PlaceExpr;
use ty_python_core::{BindingWithConstraintsIterator, FileScopeId, ProgramFile, semantic_index};

use crate::place::{class_body_implicit_symbol, place_from_bindings};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadMode, PlaceLoadResolutionStep, PlaceLoadSourceKind,
    resolve_place_load,
};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::{Db, ProgramEnvironment};

/// A set of definitions found by name resolution.
pub(crate) struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
}

impl<'db> DefinitionResolution<'db> {
    /// Returns the definitions found by name resolution.
    pub(crate) fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Resolves the reachable definitions supplied by the given bindings.
    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut definitions = SmallVec::new();

        while let Some(binding) = bindings.next() {
            let reachability = bindings.reachability_constraints().evaluate(
                db,
                bindings.predicates(),
                binding.reachability_constraint,
            );
            if reachability.is_always_false() {
                continue;
            }

            if let DefinitionState::Defined(definition) = binding.binding
                && !definitions.contains(&definition)
            {
                definitions.push(definition);
            }
        }

        Self { definitions }
    }
}

/// Returns definitions that can reach this expression's load.
///
/// Names follow Python's scope lookup rules, stopping at a definitely bound source.
/// Other tracked places use the bindings recorded at their use site.
pub(crate) fn reaching_definitions_at<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    expr: &ast::Expr,
) -> SmallVec<[Definition<'db>; 2]> {
    let index = semantic_index(db, file);
    let Some(scope) = index.try_expression_scope_id(expr) else {
        return SmallVec::new();
    };
    let Some(use_id) = index.try_expression_use_id(expr.into()) else {
        return SmallVec::new();
    };
    let mut definitions = SmallVec::new();
    let mut add_bindings = |bindings: BindingWithConstraintsIterator<'db, 'db>| {
        let resolution = DefinitionResolution::from_bindings(db, bindings);
        definitions.extend_from_slice(resolution.definitions());
    };
    if let ast::Expr::Name(name) = expr {
        let env = ProgramEnvironment::from_file(file);
        let mut resolution = resolve_place_load(
            db,
            index,
            scope.to_scope_id(db, file),
            PlaceExpr::from_expr_name(name),
            PlaceLoadMode::AtExpression(name.into()),
        );
        while let Some(PlaceLoadResolutionStep::Source(source)) = resolution.next() {
            match source.kind {
                PlaceLoadSourceKind::Bindings(bindings) => {
                    let bound = place_from_bindings(db, &env, bindings.clone())
                        .place
                        .is_definitely_bound();
                    add_bindings(bindings);
                    if bound {
                        break;
                    }
                }
                PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                    let index = semantic_index(db, scope.program_file(db));
                    add_bindings(
                        index
                            .use_def_map(scope.file_scope_id(db))
                            .reachable_bindings(id),
                    );
                    break;
                }
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(name)) => {
                    if class_body_implicit_symbol(db, &env, &name)
                        .place
                        .is_definitely_bound()
                    {
                        break;
                    }
                }
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                    file,
                    name,
                }) => {
                    let index = semantic_index(db, file);
                    if let Some(id) = index.place_table(FileScopeId::global()).symbol_id(&name) {
                        add_bindings(
                            index
                                .use_def_map(FileScopeId::global())
                                .reachable_symbol_bindings(id),
                        );
                    }
                    break;
                }
                PlaceLoadSourceKind::Implicit(_) => break,
            }
        }
    } else {
        add_bindings(index.use_def_map(scope).bindings_at_use(use_id));
    }
    definitions
}
