use rustpython_parser::ast::{Stmt, StmtKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation, CacheKey};
use ruff_python_ast::helpers::{create_stmt, resolve_imported_module_path, unparse_stmt};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

pub type Settings = Strictness;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, JsonSchema, Default,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    Parents,
    /// Ban all relative imports.
    All,
}

/// ## What it does
/// Checks for relative imports.
///
/// ## Why is this bad?
/// Absolute imports, or relative imports from siblings, are recommended by [PEP 8]:
///
/// > Absolute imports are recommended, as they are usually more readable and tend to be better behaved...
/// > ```python
/// > import mypkg.sibling
/// > from mypkg import sibling
/// > from mypkg.sibling import example
/// > ```
/// > However, explicit relative imports are an acceptable alternative to absolute imports,
/// > especially when dealing with complex package layouts where using absolute imports would be
/// > unnecessarily verbose:
/// > ```python
/// > from . import sibling
/// > from .sibling import example
/// > ```
///
/// ## Options
/// - `flake8-tidy-imports.ban-relative-imports`
///
/// ## Example
/// ```python
/// from .. import foo
/// ```
///
/// Use instead:
/// ```python
/// from mypkg import foo
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct RelativeImports {
    pub strictness: Strictness,
}

impl Violation for RelativeImports {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.strictness {
            Strictness::Parents => format!("Relative imports from parent modules are banned"),
            Strictness::All => format!("Relative imports are banned"),
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|RelativeImports { strictness }| match strictness {
            Strictness::Parents => {
                format!("Replace relative imports from parent modules with absolute imports")
            }
            Strictness::All => format!("Replace relative imports with absolute imports"),
        })
    }
}

fn fix_banned_relative_import(
    stmt: &Stmt,
    level: Option<usize>,
    module: Option<&str>,
    module_path: Option<&[String]>,
    stylist: &Stylist,
) -> Option<Edit> {
    // Only fix is the module path is known.
    let Some(module_path) = resolve_imported_module_path(level, module, module_path) else {
        return None;
    };

    // Require import to be a valid module:
    // https://python.org/dev/peps/pep-0008/#package-and-module-names
    if !module_path.split('.').all(is_identifier) {
        return None;
    }

    let StmtKind::ImportFrom { names, .. } = &stmt.node else {
        panic!("Expected StmtKind::ImportFrom");
    };
    let content = unparse_stmt(
        &create_stmt(StmtKind::ImportFrom {
            module: Some(module_path.to_string()),
            names: names.clone(),
            level: Some(0),
        }),
        stylist,
    );

    Some(Edit::replacement(
        content,
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// TID252
pub fn banned_relative_import(
    checker: &Checker,
    stmt: &Stmt,
    level: Option<usize>,
    module: Option<&str>,
    module_path: Option<&[String]>,
    strictness: &Strictness,
) -> Option<Diagnostic> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level? > strictness_level {
        let mut diagnostic = Diagnostic::new(
            RelativeImports {
                strictness: *strictness,
            },
            Range::from(stmt),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(fix) =
                fix_banned_relative_import(stmt, level, module, module_path, checker.stylist)
            {
                diagnostic.set_fix(fix);
            };
        }
        Some(diagnostic)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    use super::Strictness;

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
