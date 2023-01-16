use rustc_hash::FxHashMap;
use rustpython_ast::{Alias, Expr, Located};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::settings::hashable::HashableHashMap;
use crate::violation::Violation;

pub type Settings = HashableHashMap<String, ApiBan>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ApiBan {
    /// The message to display when the API is used.
    pub msg: String,
}

define_violation!(
    pub struct BannedApi {
        pub name: String,
        pub message: String,
    }
);
impl Violation for BannedApi {
    fn message(&self) -> String {
        let BannedApi { name, message } = self;
        format!("`{name}` is banned: {message}")
    }

    fn placeholder() -> Self {
        BannedApi {
            name: "...".to_string(),
            message: "...".to_string(),
        }
    }
}

/// TID251
pub fn name_is_banned(
    module: &str,
    name: &Alias,
    api_bans: &FxHashMap<String, ApiBan>,
) -> Option<Diagnostic> {
    let full_name = format!("{module}.{}", &name.node.name);
    if let Some(ban) = api_bans.get(&full_name) {
        return Some(Diagnostic::new(
            BannedApi {
                name: full_name,
                message: ban.msg.to_string(),
            },
            Range::from_located(name),
        ));
    }
    None
}

/// TID251
pub fn name_or_parent_is_banned<T>(
    located: &Located<T>,
    name: &str,
    api_bans: &FxHashMap<String, ApiBan>,
) -> Option<Diagnostic> {
    let mut name = name;
    loop {
        if let Some(ban) = api_bans.get(name) {
            return Some(Diagnostic::new(
                BannedApi {
                    name: name.to_string(),
                    message: ban.msg.to_string(),
                },
                Range::from_located(located),
            ));
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return None,
        }
    }
}

/// TID251
pub fn banned_attribute_access(checker: &mut Checker, expr: &Expr) {
    if let Some(call_path) = checker.resolve_call_path(expr) {
        for (banned_path, ban) in checker.settings.flake8_tidy_imports.banned_api.iter() {
            if call_path == banned_path.split('.').collect::<Vec<_>>() {
                checker.diagnostics.push(Diagnostic::new(
                    BannedApi {
                        name: banned_path.to_string(),
                        message: ban.msg.to_string(),
                    },
                    Range::from_located(expr),
                ));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use super::ApiBan;
    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test]
    fn banned_api_true_positives() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID251.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "cgi".to_string(),
                            ApiBan {
                                msg: "The cgi module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "typing.TypedDict".to_string(),
                            ApiBan {
                                msg: "Use typing_extensions.TypedDict instead.".to_string(),
                            },
                        ),
                    ])
                    .into(),
                    ..Default::default()
                },
                ..Settings::for_rules(vec![RuleCode::TID251])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
