//! `Checker` for AST-based syntax errors.
//!
//! The `Checker` is responsible for traversing the AST and running the (enabled-by-default) rules
//! at the appropriate place and time.
//!
//! The implementation is heavily inspired by the `Checker` from the `ruff_linter` crate but with a
//! sole focus of detecting syntax errors rather than being a more general diagnostic tool.

use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_ast::{
    visitor::{self, Visitor},
    ModModule, Stmt, StmtMatch,
};
use ruff_python_parser::Parsed;
use ruff_text_size::TextRange;

pub(crate) struct Checker<'a> {
    /// The [`Parsed`] output for the source code.
    parsed: &'a Parsed<ModModule>,
    /// The target Python version for detecting backwards-incompatible syntax
    /// changes.
    target_version: PythonVersion,
    /// The cumulative set of syntax errors found when visiting the source AST.
    errors: Vec<SyntaxError>,
}

impl<'a> Checker<'a> {
    pub(crate) fn new(parsed: &'a Parsed<ModModule>, target_version: PythonVersion) -> Self {
        Self {
            parsed,
            target_version,
            errors: Vec::new(),
        }
    }
}

/// Representation of a Python version.
///
/// Based on the flexible implementation in the `red_knot_python_semantic` crate for easier
/// interoperability.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PythonVersion {
    pub major: u8,
    pub minor: u8,
}

impl PythonVersion {
    pub const PY39: PythonVersion = PythonVersion { major: 3, minor: 9 };
    pub const PY310: PythonVersion = PythonVersion {
        major: 3,
        minor: 10,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub kind: SyntaxErrorKind,
    pub range: TextRange,
}

impl SyntaxError {
    pub fn into_diagnostic(self, target_version: PythonVersion) -> Diagnostic {
        Diagnostic {
            kind: DiagnosticKind {
                name: self.kind.as_str().to_string(),
                body: self.kind.message(target_version),
                suggestion: None,
            },
            range: self.range,
            fix: None,
            parent: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxErrorKind {
    MatchBeforePy310,
}

impl SyntaxErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            SyntaxErrorKind::MatchBeforePy310 => "match-before-python-310",
        }
    }

    pub fn message(self, target_version: PythonVersion) -> String {
        match self {
            SyntaxErrorKind::MatchBeforePy310 => format!(
                "Cannot use `match` statement on Python {major}.{minor} (syntax was new in Python 3.10)",
                major = target_version.major,
                minor = target_version.minor,
            ),
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
            Stmt::Match(StmtMatch { range, .. }) => {
                if self.target_version < PythonVersion::PY310 {
                    self.errors.push(SyntaxError {
                        kind: SyntaxErrorKind::MatchBeforePy310,
                        range: *range,
                    });
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

pub fn check_syntax(parsed: &Parsed<ModModule>, target_version: PythonVersion) -> Vec<SyntaxError> {
    debug_assert!(
        parsed.errors().is_empty(),
        "Should not call `check_syntax` on invalid AST"
    );
    let mut checker = Checker::new(parsed, target_version);
    checker.visit_body(checker.parsed.suite());
    checker.errors
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_debug_snapshot;
    use ruff_python_ast::PySourceType;
    use ruff_python_trivia::textwrap::dedent;

    use crate::{check_syntax, PythonVersion, SyntaxError};

    /// Run [`check_path`] on a snippet of Python code.
    fn test_snippet(contents: &str, target_version: PythonVersion) -> Vec<SyntaxError> {
        let path = Path::new("<filename>");
        let source_type = PySourceType::from(path);
        let parsed = ruff_python_parser::parse_unchecked_source(&dedent(contents), source_type);
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
            PythonVersion::PY39,
        ), @r"
        [
            SyntaxError {
                kind: MatchBeforePy310,
                range: 1..49,
            },
        ]
        ");
    }
}
