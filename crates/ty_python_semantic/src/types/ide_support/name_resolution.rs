//! Retrieves inferred name loads and their source definitions for IDE refactoring.
//!
//! Callers use a database with recording enabled, then select names with
//! [`SemanticModel::infer_name_loads`]. [`NameLoadInference`] also supports collecting names
//! from both the file's AST and parsed string annotations before running inference.

#![allow(
    dead_code,
    reason = "source-backed definition resolution is retained for IDE consumers"
)]

use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::Module;
use ty_python_core::{ProgramFile, scope::ScopeId};

use crate::place::definitions::{DefinitionResolution, definitions_for_module_global};
use crate::types::{
    InferredNameLoad, complete_inference_scope, place_load_metadata_from_inference,
};
use crate::{FxIndexMap, SemanticModel};

use super::user_visible_definitions;

/// Collects name loads from one file and retrieves their inference records in one batch.
///
/// Create this with [`SemanticModel::name_load_inference`], add names with [`Self::extend`],
/// and retrieve the records with [`Self::finish`].
pub struct NameLoadInference<'db> {
    db: &'db dyn crate::Db,
    program_file: ProgramFile<'db>,
    requests_by_scope: FxIndexMap<ScopeId<'db>, FxHashSet<ruff_text_size::TextRange>>,
}

impl<'db> NameLoadInference<'db> {
    /// Adds selected name loads from `model` to this inference run.
    ///
    /// Passing the submodel returned by [`SemanticModel::enter_string_annotation`] allows one run
    /// to include both regular file nodes and names parsed from string annotations. `model` must
    /// refer to the same program file as the model that created this inference run.
    pub fn extend<'ast>(
        &mut self,
        model: &SemanticModel<'db>,
        names: impl IntoIterator<Item = &'ast ast::ExprName>,
    ) {
        debug_assert_eq!(self.program_file, model.program_file());

        for name in names {
            let Some(file_scope) = model.scope(name.into()) else {
                continue;
            };
            let scope = file_scope.to_scope_id(self.db, self.program_file);
            let scope = complete_inference_scope(self.db, scope);
            self.requests_by_scope
                .entry(scope)
                .or_default()
                .insert(name.range());
        }
    }

    /// Runs inference as needed and returns records for the selected names that inference visited.
    ///
    /// Names that share an enclosing inference scope are retrieved together, including names in
    /// lambdas or comprehensions that need its type context. The result is empty if recording is
    /// disabled in the database.
    pub fn finish(self) -> InferredNameLoads<'db> {
        let mut loads = FxHashMap::default();
        for (scope, requested) in self.requests_by_scope {
            loads.extend(
                place_load_metadata_from_inference(self.db, scope, &requested)
                    .into_iter()
                    .map(|(range, (deferred_state, resolution))| {
                        let resolution = source_backed_resolution(self.db, resolution);
                        let load = InferredNameLoad::new(deferred_state, resolution);
                        (range, load)
                    }),
            );
        }

        InferredNameLoads { loads }
    }
}

/// Inference results for a requested set of name loads.
pub struct InferredNameLoads<'db> {
    loads: FxHashMap<ruff_text_size::TextRange, InferredNameLoad<'db>>,
}

impl<'db> InferredNameLoads<'db> {
    /// Returns the record for a selected `name` that inference visited with recording enabled.
    ///
    /// `name` must belong to the file used to create these results; lookup uses its source range.
    pub fn get(&self, name: &ast::ExprName) -> Option<&InferredNameLoad<'db>> {
        self.loads.get(&name.range())
    }
}

impl<'db> SemanticModel<'db> {
    /// Starts a batch of name loads to retrieve from this file's inference results.
    ///
    /// The database must have recording enabled; otherwise no records are returned.
    pub fn name_load_inference(&self) -> NameLoadInference<'db> {
        NameLoadInference {
            db: self.db(),
            program_file: self.program_file(),
            requests_by_scope: FxIndexMap::default(),
        }
    }

    /// Infers selected name loads and returns their deferredness and definition resolution.
    ///
    /// Recording must be enabled in the database; see [`Self::name_load_inference`].
    pub fn infer_name_loads<'ast>(
        &self,
        names: impl IntoIterator<Item = &'ast ast::ExprName>,
    ) -> InferredNameLoads<'db> {
        let mut inference = self.name_load_inference();
        inference.extend(self, names);
        inference.finish()
    }

    /// Returns the source definitions that may supply a module global at the end of its scope.
    ///
    /// Returns `None` if the module has no file or the name has no entry in its symbol table.
    /// A result does not guarantee the name is bound; inspect its resolution flags before editing.
    pub fn definitions_for_module_global(
        &self,
        module: Module<'db>,
        name: &str,
    ) -> Option<DefinitionResolution<'db>> {
        definitions_for_module_global(self.db(), self.program(), module, name)
            .map(|resolution| source_backed_resolution(self.db(), resolution))
    }
}

/// Replaces synthetic bindings with their user-visible definitions while preserving resolution flags.
///
/// If any binding has no user-visible definition, the result is marked incomplete so refactoring
/// consumers do not mistake a partial set of definitions for the full set.
pub(super) fn source_backed_resolution<'db>(
    db: &'db dyn crate::Db,
    resolution: DefinitionResolution<'db>,
) -> DefinitionResolution<'db> {
    resolution.project_definitions(|definition| user_visible_definitions(db, [definition]))
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ruff_python_ast::visitor::{Visitor, walk_expr};
    use ruff_python_ast::{self as ast};
    use ruff_text_size::Ranged;
    use ty_python_core::ProgramFile;

    use crate::db::tests::TestDbBuilder;
    use crate::{PlaceLoadRecordingMode, SemanticModel};

    #[test]
    fn observed_loads_inside_string_annotation_are_distinct() {
        let source = r#"
import first
import second
annotation: "tuple[first.C, second.C]"
"#;
        let path = "/src/test.py";
        let db = TestDbBuilder::new()
            .with_place_load_recording_mode(PlaceLoadRecordingMode::Enabled)
            .with_file(path, source)
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, path).expect("test file should exist");
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let assignment = module
            .syntax()
            .body
            .last()
            .and_then(ast::Stmt::as_ann_assign_stmt)
            .expect("last statement should be an annotated assignment");
        let annotation = assignment
            .annotation
            .as_string_literal_expr()
            .expect("assignment annotation should be a string literal");
        let model = SemanticModel::new(&db, file);
        let (annotation, model) = model
            .enter_string_annotation(annotation)
            .expect("annotation should parse as a string annotation");
        let names = loaded_names_in_expression(annotation.expr(), &["first", "second"]);
        let definitions = definition_texts_for_names(&db, &module, source, &model, names);

        assert_eq!(definitions, ["first", "second"]);
    }

    fn definition_texts_for_names<'ast>(
        db: &'ast dyn crate::Db,
        module: &ParsedModuleRef,
        source: &str,
        model: &SemanticModel<'ast>,
        names: Vec<&'ast ast::ExprName>,
    ) -> Vec<String> {
        let loads = model.infer_name_loads(names.iter().copied());

        names
            .into_iter()
            .flat_map(|name| {
                let load = loads.get(name);
                assert!(
                    load.is_some(),
                    "inference should observe the requested name load at {:?}",
                    name.range()
                );
                load.expect("asserted that inference observed this name load")
                    .resolution()
                    .definitions()
            })
            .map(|definition| {
                let range = definition.full_range(db, module).range();
                source[range].to_string()
            })
            .collect()
    }

    fn loaded_names_in_expression<'ast>(
        expression: &'ast ast::Expr,
        searched: &[&str],
    ) -> Vec<&'ast ast::ExprName> {
        let mut collector = NameCollector {
            searched,
            names: Vec::new(),
        };
        collector.visit_expr(expression);
        collector.names
    }

    struct NameCollector<'ast, 'name> {
        searched: &'name [&'name str],
        names: Vec<&'ast ast::ExprName>,
    }

    impl<'ast> Visitor<'ast> for NameCollector<'ast, '_> {
        fn visit_expr(&mut self, expression: &'ast ast::Expr) {
            if let ast::Expr::Name(name) = expression
                && name.ctx.is_load()
                && self.searched.contains(&name.id.as_str())
            {
                self.names.push(name);
            }
            walk_expr(self, expression);
        }
    }
}
