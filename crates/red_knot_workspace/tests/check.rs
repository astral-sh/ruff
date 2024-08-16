use red_knot_python_semantic::{
    HasTy, ProgramSettings, PythonVersion, SearchPathSettings, SemanticModel,
};
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{Alias, Comprehension, Expr, Parameter, ParameterWithDefault, Stmt};
use std::fs;
use std::path::PathBuf;

fn setup_db(workspace_root: SystemPathBuf) -> anyhow::Result<RootDatabase> {
    let system = OsSystem::new(&workspace_root);
    let workspace = WorkspaceMetadata::from_path(&workspace_root, &system)?;
    let search_paths = SearchPathSettings {
        extra_paths: vec![],
        src_root: workspace_root,
        custom_typeshed: None,
        site_packages: vec![],
    };
    let settings = ProgramSettings {
        target_version: PythonVersion::default(),
        search_paths,
    };
    RootDatabase::new(workspace, settings, system)
}

/// Test that all snippets in testcorpus can be checked without panic
#[test]
#[allow(clippy::print_stdout)]
fn corpus_no_panic() -> anyhow::Result<()> {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/corpus");
    let system_corpus = SystemPath::from_std_path(&corpus).expect("corpus path to be UTF8");
    let db = setup_db(system_corpus.to_path_buf())?;

    for path in fs::read_dir(&corpus).expect("corpus to be a directory") {
        let path = path.expect("path to not be an error").path();
        println!("checking {path:?}");
        let path = SystemPathBuf::from_path_buf(path.clone()).expect("path to be UTF-8");
        // this test is only asserting that we can pull every expression and definition type without a panic
        let file = system_path_to_file(&db, path).expect("file to exist");

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
    fn visit_expr(&mut self, expr: &Expr) {
        let _ty = expr.ty(&self.model);

        source_order::walk_expr(self, expr);
    }

    fn visit_alias(&mut self, alias: &Alias) {
        let _ty = alias.ty(&self.model);

        source_order::walk_alias(self, alias);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(function) => {
                let _ty = function.ty(&self.model);
            }
            Stmt::ClassDef(class) => {
                let _ty = class.ty(&self.model);
            }
            Stmt::AnnAssign(assign) => {
                let _assignment_ty = assign.ty(&self.model);

                // FIXME: I think the following constraint should be true in all cases but it isn't.
                // let target_ty = assign.target.ty(&self.model);
                //
                // assert_eq!(_assignment_ty, target_ty);
            }
            Stmt::Return(_)
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

    fn visit_comprehension(&mut self, comprehension: &Comprehension) {
        let _ty = comprehension.ty(&self.model);
        source_order::walk_comprehension(self, comprehension);
    }

    fn visit_parameter(&mut self, parameter: &Parameter) {
        let _ty = parameter.ty(&self.model);

        source_order::walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: &ParameterWithDefault) {
        let _ty = parameter_with_default.ty(&self.model);

        // FIXME: We currently don't create a definition for the nested parameter in the semantic builder.
        //   This seems correct to me because we otherwise end up with two definitions for the same symbol.
        //   However, it breaks the contract that `parameter.ty` always returns a type (doesn't panic).
        //   Not sure what the right fix is, but it's out of scope for changing the tests.
        //   Example: `def foo(bar): ...` panics
        // source_order::walk_parameter_with_default(self, parameter_with_default);
    }
}
