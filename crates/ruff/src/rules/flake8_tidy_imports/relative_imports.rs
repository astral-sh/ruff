use rustpython_parser::ast::{Stmt, StmtKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation, CacheKey};
use ruff_python_ast::helpers::{create_stmt, from_relative_import, unparse_stmt};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;
use ruff_python_stdlib::identifiers::is_module_name;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

pub type Settings = Strictness;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, JsonSchema, Default)]
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
    level: Option<&usize>,
    module: Option<&str>,
    module_path: Option<&Vec<String>>,
    stylist: &Stylist,
) -> Option<Fix> {
    // Only fix is the module path is known.
    if let Some(mut parts) = module_path.cloned() {
        if *level? >= parts.len() {
            return None;
        }

        // Remove relative level from module path.
        for _ in 0..*level? {
            parts.pop();
        }

        let module_name = if let Some(module) = module {
            let call_path = from_relative_import(&parts, module);
            // Require import to be a valid PEP 8 module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !call_path.iter().all(|part| is_module_name(part)) {
                return None;
            }
            call_path.as_slice().join(".")
        } else if parts.len() > 1 {
            let module = parts.pop().unwrap();
            let call_path = from_relative_import(&parts, &module);
            // Require import to be a valid PEP 8 module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !call_path.iter().all(|part| is_module_name(part)) {
                return None;
            }
            call_path.as_slice().join(".")
        } else {
            // Require import to be a valid PEP 8 module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !parts.iter().all(|part| is_module_name(part)) {
                return None;
            }
            parts.join(".")
        };

        let StmtKind::ImportFrom { names, .. } = &stmt.node else {
            unreachable!("Expected StmtKind::ImportFrom");
        };
        let content = unparse_stmt(
            &create_stmt(StmtKind::ImportFrom {
                module: Some(module_name),
                names: names.clone(),
                level: Some(0),
            }),
            stylist,
        );

        Some(Fix::replacement(
            content,
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    } else {
        None
    }
}

/// TID252
pub fn banned_relative_import(
    checker: &Checker,
    stmt: &Stmt,
    level: Option<&usize>,
    module: Option<&str>,
    module_path: Option<&Vec<String>>,
    strictness: &Strictness,
) -> Option<Diagnostic> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level? > &strictness_level {
        let mut diagnostic = Diagnostic::new(
            RelativeImports {
                strictness: strictness.clone(),
            },
            Range::from(stmt),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(fix) =
                fix_banned_relative_import(stmt, level, module, module_path, checker.stylist)
            {
                diagnostic.amend(fix);
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
    use insta::assert_yaml_snapshot;

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
        assert_yaml_snapshot!(diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
