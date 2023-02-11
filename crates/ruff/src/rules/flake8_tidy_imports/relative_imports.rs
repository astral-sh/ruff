use std::path::Path;

use rustpython_parser::ast::{Stmt, StmtKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string::is_lower_with_underscore;

use crate::ast::helpers::{create_stmt, unparse_stmt};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::{AutofixKind, Availability, Violation};

pub type Settings = Strictness;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    Parents,
    /// Ban all relative imports.
    All,
}

define_violation!(
    /// ## What it does
    /// Checks for relative imports.
    ///
    /// ## Why is this bad?
    /// Absolute imports, or relative imports from siblings, are recommended by [PEP 8](https://peps.python.org/pep-0008/#imports):
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
    /// Note that degree of strictness packages can be specified via the
    /// [`ban-relative-imports`](https://github.com/charliermarsh/ruff#ban-relative-imports)
    /// configuration option, which allows banning all relative imports
    /// (`ban-relative-imports = "all"`) or only those that extend into the parent module or beyond
    /// (`ban-relative-imports = "parents"`, the default).
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
    pub struct RelativeImports {
        pub strictness: Strictness,
    }
);
impl Violation for RelativeImports {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

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
    path: &Path,
    stylist: &Stylist,
) -> Option<Fix> {
    let base = if let Some(module) = module {
        module.to_string()
    } else {
        String::new()
    };

    let mut parent = path.parent()?;
    for _ in 0..*level? {
        parent = parent.parent()?;
    }

    let module_name = parent.file_name()?.to_string_lossy().to_string();

    // Require import to be a valid PEP 8 module:
    // https://python.org/dev/peps/pep-0008/#package-and-module-names
    if !is_lower_with_underscore(module_name.as_str()) {
        return None;
    }

    let new_import = if base.is_empty() {
        module_name
    } else {
        format!("{}.{}", module_name, base)
    };

    let content = match &stmt.node {
        StmtKind::ImportFrom { names, .. } => unparse_stmt(
            &create_stmt(StmtKind::ImportFrom {
                module: Some(new_import),
                names: names.clone(),
                level: Some(0),
            }),
            stylist,
        ),
        _ => return None,
    };

    Some(Fix::replacement(
        content,
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// TID252
pub fn banned_relative_import(
    checker: &Checker,
    stmt: &Stmt,
    level: Option<&usize>,
    module: Option<&str>,
    strictness: &Strictness,
    path: &Path,
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
            Range::from_located(stmt),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(fix) =
                fix_banned_relative_import(stmt, level, module, path, checker.stylist)
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

    use crate::assert_yaml_snapshot;
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
}
