//! [`SyntaxChecker`] for AST-based syntax errors. //
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SourceOrderVisitor::enter_node`] method should be called on every node by
//! a parent `Visitor`.

use ruff_python_ast::{Stmt, StmtExpr, StmtImportFrom, StmtMatch};
use ruff_text_size::TextRange;

pub struct SyntaxChecker {
    /// The target Python version for detecting backwards-incompatible syntax
    /// changes.
    target_version: PythonVersion,
    /// The cumulative set of syntax errors found when visiting the source AST.
    errors: Vec<SyntaxError>,

    /// these could be grouped into a bitflags struct like `SemanticModel`
    seen_futures_boundary: bool,
    seen_docstring_boundary: bool,
}

impl SyntaxChecker {
    pub fn new(target_version: PythonVersion) -> Self {
        Self {
            target_version,
            errors: Vec::new(),
            seen_futures_boundary: false,
            seen_docstring_boundary: false,
        }
    }

    pub fn finish(&self) -> impl Iterator<Item = &SyntaxError> {
        self.errors.iter()
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
    pub target_version: PythonVersion,
}

impl SyntaxError {
    pub fn message(&self) -> String {
        match self.kind {
            SyntaxErrorKind::MatchBeforePy310 => format!(
                "Cannot use `match` statement on Python {major}.{minor} (syntax was new in Python 3.10)",
                major = self.target_version.major,
                minor = self.target_version.minor,
            ),
            SyntaxErrorKind::LateFutureImport => {
				"__future__ imports must be at the top of the file".to_string()
			}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxErrorKind {
    MatchBeforePy310,
    LateFutureImport,
}

impl SyntaxErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            SyntaxErrorKind::MatchBeforePy310 => "match-before-python-310",
            SyntaxErrorKind::LateFutureImport => "late-future-import",
        }
    }
}

impl SyntaxChecker {
    fn check(&mut self, stmt: &ruff_python_ast::Stmt) {
        match stmt {
            Stmt::Match(StmtMatch { range, .. }) => {
                if self.target_version < PythonVersion::PY310 {
                    self.errors.push(SyntaxError {
                        kind: SyntaxErrorKind::MatchBeforePy310,
                        range: *range,
                        target_version: self.target_version,
                    });
                }
            }
            Stmt::ImportFrom(StmtImportFrom { range, module, .. }) => {
                if self.seen_futures_boundary && matches!(module.as_deref(), Some("__future__")) {
                    self.errors.push(SyntaxError {
                        kind: SyntaxErrorKind::LateFutureImport,
                        range: *range,
                        target_version: self.target_version,
                    });
                }
            }
            _ => {}
        }
    }

    pub fn enter_stmt(&mut self, stmt: &ruff_python_ast::Stmt) {
        match stmt {
            Stmt::Expr(StmtExpr { value, .. })
                if !self.seen_docstring_boundary && value.is_string_literal_expr() =>
            {
                self.seen_docstring_boundary = true;
            }
            Stmt::ImportFrom(StmtImportFrom { module, .. }) => {
                self.seen_docstring_boundary = true;
                // Allow __future__ imports until we see a non-__future__ import.
                if let Some("__future__") = module.as_deref() {
                } else {
                    self.seen_futures_boundary = true;
                }
            }
            Stmt::Import(_) => {
                self.seen_docstring_boundary = true;
                self.seen_futures_boundary = true;
            }
            _ => {
                self.seen_docstring_boundary = true;
                self.seen_futures_boundary = true;
            }
        }

        self.check(stmt);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_debug_snapshot;
    use ruff_python_ast::PySourceType;
    use ruff_python_trivia::textwrap::dedent;

    use crate::{PythonVersion, SyntaxChecker, SyntaxError};

    /// Run [`check_syntax`] on a snippet of Python code.
    fn test_snippet(contents: &str, target_version: PythonVersion) -> Vec<SyntaxError> {
        let path = Path::new("<filename>");
        let source_type = PySourceType::from(path);
        let parsed = ruff_python_parser::parse_unchecked_source(&dedent(contents), source_type);
        let mut checker = SyntaxChecker::new(target_version);
        for stmt in parsed.suite() {
            checker.enter_stmt(stmt);
        }
        checker.errors
    }

    #[test]
    fn match_before_py310() {
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
                target_version: PythonVersion {
                    major: 3,
                    minor: 9,
                },
            },
        ]
        ");
    }

    #[test]
    fn match_on_py310() {
        assert_debug_snapshot!(test_snippet(
            r#"
match var:
    case 1:
        print("it's one")
"#,
            PythonVersion::PY310,
        ), @"[]");
    }
}
