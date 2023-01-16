use rustpython_ast::Stmt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

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
    pub struct RelativeImports(pub Strictness);
);
impl Violation for RelativeImports {
    fn message(&self) -> String {
        let RelativeImports(strictness) = self;
        match strictness {
            Strictness::Parents => "Relative imports from parent modules are banned".to_string(),
            Strictness::All => "Relative imports are banned".to_string(),
        }
    }

    fn placeholder() -> Self {
        RelativeImports(Strictness::All)
    }
}

/// TID252
pub fn banned_relative_import(
    stmt: &Stmt,
    level: Option<&usize>,
    strictness: &Strictness,
) -> Option<Diagnostic> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level? > &strictness_level {
        Some(Diagnostic::new(
            RelativeImports(strictness.clone()),
            Range::from_located(stmt),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use super::Strictness;
    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![RuleCode::TID252])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![RuleCode::TID252])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
