//! This module exposes name resolution and reaching-definition analysis to the language server
//! without requiring it to run type inference.
//!
//! It provides two entry points on [`SemanticModel`]:
//!
//! - [`SemanticModel::name_load`] resolves a name load in the model's file. It selects the
//!   appropriate point-in-time, deferred, or string-annotation binding state. It returns `None`
//!   when selecting that state requires type inference.
//! - [`SemanticModel::definitions_for_module_global`] resolves an explicit module global using the
//!   bindings available at the end of the module.
//!
//! Both entry points return a [`DefinitionResolution`], which exposes the definitions found by
//! name resolution and whether the result is complete, may be unbound, may be deleted, or crosses
//! a `global` or `nonlocal` declaration.
//!
//! ## Example
//!
//! ```py
//! if use_fallback:
//!     from .fallback import handler  # definition A
//! else:
//!     from .primary import handler  # definition B
//!
//! handler(request)  # load U
//! ```
//!
//! Calling [`SemanticModel::name_load`] for `U` returns a [`DefinitionResolution`] containing
//! definitions A and B. Because every branch defines `handler`,
//! [`DefinitionResolution::is_definitely_bound`] returns `true`. If the `else` branch were absent,
//! the resolution would still contain definition A, but `is_definitely_bound` would return
//! `false`.

use ruff_python_ast as ast;
use ty_module_resolver::Module;
use ty_python_core::place::PlaceExpr;
use ty_python_core::semantic_index;

use crate::SemanticModel;
use crate::definition_resolution::DefinitionResolution;
use crate::definition_resolution::{definitions_for_module_global, definitions_for_place_load};
use crate::place_load::{PlaceLoadMode, resolve_place_load};

use super::source_backed_definitions;

impl<'db> SemanticModel<'db> {
    /// Resolves the definitions for a name load.
    ///
    /// Returns `None` when choosing the correct binding state would require type inference.
    pub fn name_load(&self, name: &ast::ExprName) -> Option<DefinitionResolution<'db>> {
        let environment = self.program_environment();
        let index = semantic_index(self.db(), self.program_file());
        let scope = self
            .scope(name.into())?
            .to_scope_id(self.db(), self.program_file());
        let mode = if self.is_in_string_annotation() {
            PlaceLoadMode::StringAnnotation
        } else if index.place_load_is_deferred(ast::ExprRef::Name(name))? {
            PlaceLoadMode::Deferred
        } else {
            PlaceLoadMode::AtExpression(name.into())
        };

        let mut place_load_resolution = resolve_place_load(
            self.db(),
            index,
            scope,
            PlaceExpr::from_expr_name(name),
            mode,
        );

        let resolution =
            definitions_for_place_load(self.db(), &environment, scope, &mut place_load_resolution);

        Some(source_backed_resolution(self.db(), resolution))
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

fn source_backed_resolution<'db>(
    db: &'db dyn crate::Db,
    resolution: DefinitionResolution<'db>,
) -> DefinitionResolution<'db> {
    resolution.project_definitions(|definition| source_backed_definitions(db, [definition]))
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ruff_db::source::{SourceText, source_text};
    use ruff_python_ast::visitor::{Visitor, walk_expr};
    use ruff_python_ast::{self as ast, PythonVersion};
    use ruff_text_size::Ranged;
    use ty_python_core::ProgramFile;

    use super::DefinitionResolution;
    use crate::SemanticModel;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn name_load_uses_point_in_time_bindings() {
        let db = test_db(
            r#"
import first as value
before = value
import second as value
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(test.definition_texts(&load), ["first as value"]);
    }

    #[test]
    fn name_load_uses_end_of_scope_bindings_for_deferred_annotations() {
        let db = stub_test_db(
            r#"
import first as value
before: value.C
import second as value
"#,
        );
        let test = NameResolutionTest::new(&db, STUB_FILE);
        let load = test.name_load("value");

        assert_eq!(
            test.definition_texts(&load),
            ["first as value", "second as value"]
        );
    }

    #[test]
    fn name_load_uses_end_of_scope_bindings_with_future_annotations() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .with_file(
                    PYTHON_FILE,
                    r#"
from __future__ import annotations
import first as value
before: value.C
import second as value
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(
            test.definition_texts(&load),
            ["first as value", "second as value"]
        );
    }

    #[test]
    fn name_load_uses_end_of_scope_bindings_for_python_314_annotations() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_python_version(PythonVersion::PY314)
                .with_file(
                    PYTHON_FILE,
                    r#"
import first as value
before: value.C
import second as value
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(
            test.definition_texts(&load),
            ["first as value", "second as value"]
        );
    }

    #[test]
    fn name_load_returns_none_when_deferredness_requires_inference() {
        let db = stub_test_db(
            r#"
import first as value
before = value
"#,
        );
        let test = NameResolutionTest::new(&db, STUB_FILE);

        assert!(test.try_name_load("value").is_none());
    }

    #[test]
    fn name_load_uses_end_of_scope_bindings_in_other_deferred_contexts() {
        let db = stub_test_db(
            r#"
import first as value
def function[T: value.C](arg=value): ...
class Class(value.C): ...
type Alias = value.C
callback = lambda arg=value: None
import second as value
"#,
        );
        let test = NameResolutionTest::new(&db, STUB_FILE);
        let loads = test.name_loads("value");

        assert_eq!(loads.len(), 5);
        for load in loads {
            assert_eq!(
                test.definition_texts(&load),
                ["first as value", "second as value"]
            );
        }
    }

    #[test]
    fn name_load_excludes_bindings_that_do_not_reach_the_use() {
        let db = test_db(
            r#"
def test(flag: bool):
    if flag:
        x: int = 1
        return
    x = 2
    print(x)
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("x");

        assert_eq!(test.definition_texts(&load), ["x = 2"]);
    }

    #[test]
    fn name_load_respects_redeclarations() {
        let db = test_db(
            r#"
def test(flag: bool):
    if flag:
        x: int = 10
    else:
        x: str = 'test'
    print(x)
    x: int = 30
    print(x)
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let loads = test.name_loads("x");
        let [first_load, second_load] = loads.as_slice() else {
            panic!("expected two loads of `x`");
        };

        assert_eq!(
            test.definition_texts(first_load),
            ["x: int = 10", "x: str = 'test'"]
        );
        assert_eq!(test.definition_texts(second_load), ["x: int = 30"]);
    }

    #[test]
    fn name_load_reports_possible_unboundness() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_file(
                    PYTHON_FILE,
                    r#"
def test(flag: bool):
    if flag:
        import other as value
    return value
"#,
                )
                .with_file(
                    "/src/other.py",
                    r#"
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(test.definition_texts(&load), ["other as value"]);
        assert!(!load.is_definitely_bound());
        assert!(!load.may_be_deleted());
    }

    #[test]
    fn name_load_reports_reachable_deletions() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_file(
                    PYTHON_FILE,
                    r#"
def test(flag: bool):
    import other as value
    if flag:
        del value
    return value
"#,
                )
                .with_file(
                    "/src/other.py",
                    r#"
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(test.definition_texts(&load), ["other as value"]);
        assert!(!load.is_definitely_bound());
        assert!(load.may_be_deleted());
    }

    #[test]
    fn name_load_reports_deletions_from_previous_loop_iterations() {
        let db = test_db(
            r#"
def random() -> bool:
    return False

x = 0
while random():
    print(x)
    x = 42
    del x
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("x");

        assert!(!load.is_definitely_bound());
        assert!(load.may_be_deleted());
    }

    #[test]
    fn name_load_distinguishes_implicit_values_from_missing_names() {
        let db = test_db(
            r#"
def test():
    return int, missing_name
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let builtin = test.name_load("int");
        let missing = test.name_load("missing_name");

        let [builtin_definition] = builtin.definitions() else {
            panic!("expected exactly one definition for `int`");
        };
        assert_eq!(builtin_definition.name(&db).as_deref(), Some("int"));
        assert!(!builtin.is_complete());
        assert!(builtin.is_definitely_bound());
        assert!(missing.is_complete());
        assert!(!missing.is_definitely_bound());
    }

    #[test]
    fn implicit_builtin_definitions_require_explicit_reexports() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_file(
                    "/src/__builtins__.pyi",
                    r#"
flag: bool
if flag:
    from first import value as value
else:
    from second import value
"#,
                )
                .with_file(
                    "/src/first.py",
                    r#"
value = 1
"#,
                )
                .with_file(
                    "/src/second.py",
                    r#"
value = 2
"#,
                )
                .with_file(
                    PYTHON_FILE,
                    r#"
result = value
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(load.definitions().len(), 1);
        assert!(!load.is_complete());
        assert!(!load.is_definitely_bound());
    }

    #[test]
    fn implicit_builtin_definitions_do_not_use_later_bindings_from_the_same_scope() {
        let db = build_test_db(
            TestDbBuilder::new()
                .with_file(
                    "/src/__builtins__.py",
                    r#"
before = value
from first import value as value
"#,
                )
                .with_file(
                    "/src/first.py",
                    r#"
value = 1
"#,
                ),
        );
        let test = NameResolutionTest::new(&db, "/src/__builtins__.py");
        let load = test.name_load("value");

        assert!(load.definitions().is_empty());
        assert!(!load.is_complete());
    }

    #[test]
    fn name_load_reports_scope_declarations() {
        let db = test_db(
            r#"
value = 1
def test():
    global value
    return value
"#,
        );
        let test = NameResolutionTest::new(&db, PYTHON_FILE);
        let load = test.name_load("value");

        assert_eq!(test.definition_texts(&load), ["value = 1"]);
        assert!(load.crosses_scope_declaration());
    }

    const PYTHON_FILE: &str = "/src/foo.py";
    const STUB_FILE: &str = "/src/foo.pyi";

    fn test_db(source: &str) -> TestDb {
        build_test_db(TestDbBuilder::new().with_file(PYTHON_FILE, source))
    }

    fn stub_test_db(source: &str) -> TestDb {
        build_test_db(TestDbBuilder::new().with_file(STUB_FILE, source))
    }

    fn build_test_db(builder: TestDbBuilder<'_>) -> TestDb {
        builder.build().expect("valid TestDb setup")
    }

    struct NameResolutionTest<'db> {
        db: &'db TestDb,
        model: SemanticModel<'db>,
        module: ParsedModuleRef,
        source: SourceText,
    }

    impl<'db> NameResolutionTest<'db> {
        fn new(db: &'db TestDb, path: &str) -> Self {
            let Ok(file) = system_path_to_file(db, path) else {
                panic!("test file `{path}` should exist");
            };
            let file = ProgramFile::new(db, file, db.program_environment().program(db));
            let module = parsed_module(db, file.python_file(db)).load(db);
            let source = source_text(db, file.file(db));
            let model = SemanticModel::new(db, file);
            Self {
                db,
                model,
                module,
                source,
            }
        }

        fn try_name_load(&self, search: &str) -> Option<DefinitionResolution<'db>> {
            let names = loaded_names(self.module.syntax(), search);
            let &[name] = names.as_slice() else {
                panic!(
                    "expected exactly one load of `{search}`, found {}",
                    names.len()
                );
            };
            self.model.name_load(name)
        }

        fn name_load(&self, search: &str) -> DefinitionResolution<'db> {
            self.try_name_load(search)
                .unwrap_or_else(|| panic!("expected `{search}` load to be supported"))
        }

        fn name_loads(&self, searches: &str) -> Vec<DefinitionResolution<'db>> {
            loaded_names(self.module.syntax(), searches)
                .into_iter()
                .map(|name| {
                    self.model.name_load(name).unwrap_or_else(|| {
                        panic!("expected every `{searches}` load to be supported")
                    })
                })
                .collect()
        }

        fn definition_texts<'a>(&'a self, load: &DefinitionResolution<'db>) -> Vec<&'a str> {
            load.definitions()
                .iter()
                .copied()
                .map(|definition| {
                    let range = definition.full_range(self.db, &self.module).range();
                    &self.source[range]
                })
                .collect()
        }
    }

    fn loaded_names<'ast>(
        module: &'ast ast::ModModule,
        searched: &str,
    ) -> Vec<&'ast ast::ExprName> {
        struct Collector<'ast, 'name> {
            searched: &'name str,
            names: Vec<&'ast ast::ExprName>,
        }

        impl<'ast> Visitor<'ast> for Collector<'ast, '_> {
            fn visit_expr(&mut self, expression: &'ast ast::Expr) {
                if let ast::Expr::Name(name) = expression
                    && name.ctx.is_load()
                    && name.id == self.searched
                {
                    self.names.push(name);
                }
                walk_expr(self, expression);
            }
        }

        let mut collector = Collector {
            searched,
            names: Vec::new(),
        };
        collector.visit_body(&module.body);
        collector.names
    }
}
