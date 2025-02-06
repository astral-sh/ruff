//! `Checker` for AST-based syntax errors.
//!
//! The `Checker` is responsible for traversing the AST and running the (enabled-by-default) rules
//! at the appropriate place and time.
//!
//! The implementation is heavily inspired by the `Checker` from the `ruff_linter` crate but with a
//! sole focus of detecting syntax errors rather than being a more general diagnostic tool.

use ruff_diagnostics::Diagnostic;
use ruff_linter::settings::types::PythonVersion;
use ruff_python_ast::{
    visitor::{self, Visitor},
    ModModule, Stmt,
};
use ruff_python_parser::Parsed;
use ruff_text_size::TextRange;

pub(crate) struct Checker<'a> {
    /// The [`Parsed`] output for the source code.
    parsed: &'a Parsed<ModModule>,
    /// The target Python version for detecting backwards-incompatible syntax
    /// changes.
    target_version: PythonVersion,
    /// The cumulative set of diagnostics computed across all lint rules.
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    pub(crate) fn new(parsed: &'a Parsed<ModModule>, target_version: PythonVersion) -> Self {
        Self {
            parsed,
            target_version,
            diagnostics: Vec::new(),
        }
    }
}

mod rules {
    pub(crate) use match_before_py310::*;

    mod match_before_py310 {
        use ruff_diagnostics::Violation;
        use ruff_macros::{derive_message_formats, ViolationMetadata};

        #[derive(ViolationMetadata)]
        pub(crate) struct MatchBeforePy310;

        impl Violation for MatchBeforePy310 {
            #[derive_message_formats]
            fn message(&self) -> String {
                "`match` can only be used on Python 3.10+".to_string()
            }
        }
    }
}

impl<'a> Visitor<'a> for Checker<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(_) => {}
            Stmt::ClassDef(_) => {}
            Stmt::Return(_) => {}
            Stmt::Delete(_) => {}
            Stmt::TypeAlias(_) => {}
            Stmt::Assign(_) => {}
            Stmt::AugAssign(_) => {}
            Stmt::AnnAssign(_) => {}
            Stmt::For(_) => {}
            Stmt::While(_) => {}
            Stmt::If(_) => {}
            Stmt::With(_) => {}
            Stmt::Match(_) => {
                if self.target_version.minor() < 10 {
                    self.diagnostics.push(Diagnostic::new(
                        rules::MatchBeforePy310,
                        TextRange::default(),
                    ));
                }
            }
            Stmt::Raise(_) => {}
            Stmt::Try(_) => {}
            Stmt::Assert(_) => {}
            Stmt::Import(_) => {}
            Stmt::ImportFrom(_) => {}
            Stmt::Global(_) => {}
            Stmt::Nonlocal(_) => {}
            Stmt::Expr(_) => {}
            Stmt::Pass(_) => {}
            Stmt::Break(_) => {}
            Stmt::Continue(_) => {}
            Stmt::IpyEscapeCommand(_) => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

pub fn check_syntax(parsed: &Parsed<ModModule>, target_version: PythonVersion) -> Vec<Diagnostic> {
    let mut checker = Checker::new(parsed, target_version);
    checker.visit_body(checker.parsed.suite());

    checker.diagnostics
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_debug_snapshot;
    use ruff_diagnostics::Diagnostic;
    use ruff_linter::{settings::types::PythonVersion, source_kind::SourceKind};
    use ruff_python_ast::PySourceType;
    use ruff_python_trivia::textwrap::dedent;

    use crate::check_syntax;

    /// Run [`check_path`] on a snippet of Python code.
    fn test_snippet(contents: &str, target_version: PythonVersion) -> Vec<Diagnostic> {
        let path = Path::new("<filename>");
        let contents = SourceKind::Python(dedent(contents).into_owned());
        let source_type = PySourceType::from(path);
        let parsed =
            ruff_python_parser::parse_unchecked_source(contents.source_code(), source_type);
        check_syntax(&parsed, target_version)
    }

    #[test]
    fn hello_world() {
        assert_debug_snapshot!(test_snippet(
            r#"
match var:
    case 1:
        print("it's one")
"#,
            PythonVersion::Py39,
        ), @r#"
        [
            Diagnostic {
                kind: DiagnosticKind {
                    name: "MatchBeforePy310",
                    body: "`match` can only be used on Python 3.10+",
                    suggestion: None,
                },
                range: 0..0,
                fix: None,
                parent: None,
            },
        ]
        "#);
    }
}
