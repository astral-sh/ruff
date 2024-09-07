use std::fs;
use std::path::PathBuf;

use red_knot_python_semantic::{HasTy, SemanticModel};
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{Alias, Expr, Parameter, ParameterWithDefault, Stmt};

fn setup_db(workspace_root: &SystemPath) -> anyhow::Result<RootDatabase> {
    let system = OsSystem::new(workspace_root);
    let workspace = WorkspaceMetadata::from_path(workspace_root, &system, None)?;
    RootDatabase::new(workspace, system)
}

/// Test that all snippets in testcorpus can be checked without panic
#[test]
#[allow(clippy::print_stdout)]
fn corpus_no_panic() -> anyhow::Result<()> {
    let root = SystemPathBuf::from_path_buf(tempfile::TempDir::new()?.into_path()).unwrap();
    let db = setup_db(&root)?;

    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/corpus");

    for path in fs::read_dir(&corpus)? {
        let source = path?.path();
        println!("checking {source:?}");
        let source_fn = source.file_name().unwrap().to_str().unwrap();
        let py_dest = root.join(source_fn);
        fs::copy(&source, py_dest.as_std_path())?;
        // this test is only asserting that we can pull every expression type without a panic
        // (and some non-expressions that clearly define a single type)
        let file = system_path_to_file(&db, py_dest).unwrap();
        pull_types(&db, file);

        // try the file as a stub also
        println!("re-checking as .pyi");
        let pyi_dest = root.join(format!("{source_fn}i"));
        std::fs::copy(source, pyi_dest.as_std_path())?;
        let file = system_path_to_file(&db, pyi_dest).unwrap();
        pull_types(&db, file);
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
            Stmt::AnnAssign(_)
            | Stmt::Return(_)
            | Stmt::Delete(_)
            | Stmt::Assign(_)
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
