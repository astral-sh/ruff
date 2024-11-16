use red_knot_python_semantic::{HasTy, SemanticModel};
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{SystemPath, SystemPathBuf, TestSystem};
use ruff_python_ast::visitor::source_order;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{self as ast, Alias, Expr, Parameter, ParameterWithDefault, Stmt};

fn setup_db(workspace_root: &SystemPath, system: TestSystem) -> anyhow::Result<RootDatabase> {
    let workspace = WorkspaceMetadata::discover(workspace_root, &system, None)?;
    RootDatabase::new(workspace, system)
}

fn get_workspace_root() -> anyhow::Result<SystemPathBuf> {
    Ok(SystemPathBuf::from(String::from_utf8(
        std::process::Command::new("cargo")
            .args(["locate-project", "--workspace", "--message-format", "plain"])
            .output()?
            .stdout,
    )?)
    .parent()
    .unwrap()
    .to_owned())
}

/// Test that all snippets in testcorpus can be checked without panic (except for [`KNOWN_FAILURES`])
#[test]
#[allow(clippy::print_stdout)]
fn corpus_no_panic() -> anyhow::Result<()> {
    let root = SystemPathBuf::from("/src");

    let system = TestSystem::default();
    let memory_fs = system.memory_file_system();
    memory_fs.create_directory_all(root.as_ref())?;

    let mut db = setup_db(&root, system.clone())?;

    let crate_root = String::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = get_workspace_root()?;
    let workspace_root = workspace_root.to_string();

    let corpus = vec![
        format!("{crate_root}/resources/test/corpus/**/*.py"),
        format!("{workspace_root}/crates/ruff_python_parser/resources/**/*.py"),
        format!("{workspace_root}/crates/ruff_linter/resources/**/*.py"),
        // TODO: Enable running over typeshed stubs once there are fewer failures:
        // format!("{workspace_root}/crates/red_knot_vendored/vendor/typeshed/**/*.pyi"),
    ]
    .into_iter()
    .flat_map(|pattern| glob::glob(&pattern).unwrap());

    for path in corpus {
        let path = path?;
        let relative_path = path.strip_prefix(&workspace_root)?;

        let (py_expected_to_fail, pyi_expected_to_fail) = KNOWN_FAILURES
            .iter()
            .find_map(|(path, py_fail, pyi_fail)| {
                if Some(*path)
                    == relative_path
                        .to_str()
                        .map(|p| p.replace('\\', "/"))
                        .as_deref()
                {
                    Some((*py_fail, *pyi_fail))
                } else {
                    None
                }
            })
            .unwrap_or((false, false));

        let source = path.as_path();
        let source_filename = source.file_name().unwrap().to_str().unwrap();

        let code = std::fs::read_to_string(source)?;

        let mut check_with_file_name = |path: &SystemPath| {
            memory_fs.write_file(path, &code).unwrap();
            File::sync_path(&mut db, path);

            // this test is only asserting that we can pull every expression type without a panic
            // (and some non-expressions that clearly define a single type)
            let file = system_path_to_file(&db, path).unwrap();

            let result = std::panic::catch_unwind(|| pull_types(&db, file));

            let expected_to_fail = if path.extension().map(|e| e == "pyi").unwrap_or(false) {
                pyi_expected_to_fail
            } else {
                py_expected_to_fail
            };
            if let Err(err) = result {
                if !expected_to_fail {
                    println!("Check failed for {relative_path:?}. Consider fixing it or adding it to KNOWN_FAILURES");
                    std::panic::resume_unwind(err);
                }
            } else {
                assert!(!expected_to_fail, "Expected to panic, but did not. Consider removing this path from KNOWN_FAILURES");
            }

            memory_fs.remove_all();
            file.sync(&mut db);
        };

        if source.extension().map(|e| e == "pyi").unwrap_or(false) {
            println!("checking {relative_path:?}");
            let pyi_dest = root.join(source_filename);
            check_with_file_name(&pyi_dest);
        } else {
            println!("checking {relative_path:?}");
            let py_dest = root.join(source_filename);
            check_with_file_name(&py_dest);

            let pyi_dest = root.join(format!("{source_filename}i"));
            println!("re-checking as stub file: {pyi_dest:?}");
            check_with_file_name(&pyi_dest);
        }
    }
    Ok(())
}

fn pull_types(db: &RootDatabase, file: File) {
    let mut visitor = PullTypesVisitor::new(db, file);

    let ast = parsed_module(db, file);

    visitor.visit_body(ast.suite());
}

struct PullTypesVisitor<'db> {
    model: SemanticModel<'db>,
}

impl<'db> PullTypesVisitor<'db> {
    fn new(db: &'db RootDatabase, file: File) -> Self {
        Self {
            model: SemanticModel::new(db, file),
        }
    }

    fn visit_assign_target(&mut self, target: &Expr) {
        match target {
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for element in elts {
                    self.visit_assign_target(element);
                }
            }
            _ => self.visit_expr(target),
        }
    }
}

impl SourceOrderVisitor<'_> for PullTypesVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(function) => {
                let _ty = function.ty(&self.model);
            }
            Stmt::ClassDef(class) => {
                let _ty = class.ty(&self.model);
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.visit_assign_target(target);
                }
                return;
            }
            Stmt::AnnAssign(_)
            | Stmt::Return(_)
            | Stmt::Delete(_)
            | Stmt::AugAssign(_)
            | Stmt::TypeAlias(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::If(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Raise(_)
            | Stmt::Try(_)
            | Stmt::Assert(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::IpyEscapeCommand(_) => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        let _ty = expr.ty(&self.model);

        source_order::walk_expr(self, expr);
    }

    fn visit_parameter(&mut self, parameter: &Parameter) {
        let _ty = parameter.ty(&self.model);

        source_order::walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: &ParameterWithDefault) {
        let _ty = parameter_with_default.ty(&self.model);

        source_order::walk_parameter_with_default(self, parameter_with_default);
    }

    fn visit_alias(&mut self, alias: &Alias) {
        let _ty = alias.ty(&self.model);

        source_order::walk_alias(self, alias);
    }
}

/// Whether or not the .py/.pyi version of this file is expected to fail
const KNOWN_FAILURES: &[(&str, bool, bool)] = &[
    // Probably related to missing support for type aliases / type params:
    ("crates/ruff_python_parser/resources/inline/err/type_param_invalid_bound_expr.py", true, true),
    ("crates/ruff_python_parser/resources/inline/err/type_param_type_var_invalid_default_expr.py", true, true),
    ("crates/ruff_python_parser/resources/inline/err/type_param_param_spec_invalid_default_expr.py", true, true),
    ("crates/ruff_python_parser/resources/inline/err/type_param_type_var_missing_default.py", true, true),
    ("crates/ruff_python_parser/resources/inline/err/type_param_type_var_tuple_invalid_default_expr.py", true, true),
    ("crates/ruff_python_parser/resources/inline/ok/type_param_param_spec.py", true, true),
    ("crates/ruff_python_parser/resources/inline/ok/type_param_type_var.py", true, true),
    ("crates/ruff_python_parser/resources/inline/ok/type_param_type_var_tuple.py", true, true),
    ("crates/ruff_python_parser/resources/valid/statement/type.py", true, true),
    // Fails for unknown reasons:
    ("crates/ruff_python_parser/resources/valid/expressions/f_string.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_future_annotations/no_future_import_uses_union_inner.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI011.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI015.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI016.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI019.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI020.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI020.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI030.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI030.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI034.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI034.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI035.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI035.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI036.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI036.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI041.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI041.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI051.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI051.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI052.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI052.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI055.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI055.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI062.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI062.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI063.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI063.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI064.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_pyi/PYI064.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/TCH004_13.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/TCH004_13.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/TCH004_15.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/TCH004_15.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote2.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote2.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote3.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/flake8_type_checking/quote3.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F401_19.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F401_19.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F541.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F541.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F632.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F632.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F811_19.py", true, false),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_14.py", false, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_15.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_15.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_17.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_17.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_2.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_2.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_20.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_20.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/F821_26.py", true, false),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/project/foo/bar.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/project/foo/bar.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/project/foo/bop/baz.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyflakes/project/foo/bop/baz.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pylint/single_string_slots.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pylint/single_string_slots.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyupgrade/UP037_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyupgrade/UP037_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyupgrade/UP039.py", true, false),
    ("crates/ruff_linter/resources/test/fixtures/pyupgrade/UP044.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/pyupgrade/UP044.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF013_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF013_0.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF013_3.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF013_3.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF022.py", true, true),
    ("crates/ruff_linter/resources/test/fixtures/ruff/RUF022.py", true, true),
];
