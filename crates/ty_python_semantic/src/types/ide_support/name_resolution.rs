//! Definition-resolution support for IDE refactors.

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

/// Selects name-load records from cached inference for one file.
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

    /// Runs inference as needed and returns the selected name-load records.
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
    /// Returns the inference result for `name`, if inference visited it.
    pub fn get(&self, name: &ast::ExprName) -> Option<&InferredNameLoad<'db>> {
        self.loads.get(&name.range())
    }
}

impl<'db> SemanticModel<'db> {
    /// Selects name loads from this file's cached inference results.
    ///
    /// Call [`crate::enable_place_load_recording`] for this file before creating the model.
    /// Otherwise no records are returned.
    pub fn name_load_inference(&self) -> NameLoadInference<'db> {
        NameLoadInference {
            db: self.db(),
            program_file: self.program_file(),
            requests_by_scope: FxIndexMap::default(),
        }
    }

    /// Infers selected name loads and returns their deferredness and definition resolution.
    ///
    /// Recording must already be enabled for this file; see [`Self::name_load_inference`].
    pub fn infer_name_loads<'ast>(
        &self,
        names: impl IntoIterator<Item = &'ast ast::ExprName>,
    ) -> InferredNameLoads<'db> {
        let mut inference = self.name_load_inference();
        inference.extend(self, names);
        inference.finish()
    }

    /// Resolves the definitions for an explicit module global.
    pub fn definitions_for_module_global(
        &self,
        module: Module<'db>,
        name: &str,
    ) -> Option<DefinitionResolution<'db>> {
        definitions_for_module_global(self.db(), self.program(), module, name)
            .map(|resolution| source_backed_resolution(self.db(), resolution))
    }
}

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
    fn observed_load_uses_point_in_time_bindings() {
        let definitions = definition_texts(
            r#"
import first as value
before = value
import second as value
"#,
            &["value"],
        );

        assert_eq!(definitions, ["first as value"]);
    }

    #[test]
    fn observed_loads_inside_string_annotation_are_distinct() {
        let source = r#"
import first
import second
annotation: "tuple[first.C, second.C]"
"#;
        let path = "/src/test.py";
        let mut db = TestDbBuilder::new()
            .with_place_load_recording_mode(PlaceLoadRecordingMode::OnDemand)
            .with_file(path, source)
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, path).expect("test file should exist");
        db.enable_place_load_recording(file);
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

    fn definition_texts(source: &str, searched: &[&str]) -> Vec<String> {
        let path = "/src/test.py";
        let mut db = TestDbBuilder::new()
            .with_place_load_recording_mode(PlaceLoadRecordingMode::OnDemand)
            .with_file(path, source)
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, path).expect("test file should exist");
        db.enable_place_load_recording(file);
        let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let names = loaded_names(&module, searched);
        let model = SemanticModel::new(&db, file);

        definition_texts_for_names(&db, &module, source, &model, names)
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

    fn loaded_names<'ast>(
        module: &'ast ParsedModuleRef,
        searched: &[&str],
    ) -> Vec<&'ast ast::ExprName> {
        let mut collector = NameCollector {
            searched,
            names: Vec::new(),
        };
        collector.visit_body(&module.syntax().body);
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
