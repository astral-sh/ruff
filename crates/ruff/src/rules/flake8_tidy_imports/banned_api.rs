use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Alias, Expr, Located};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation, CacheKey};
use ruff_python_ast::types::{CallPath, Range};

use crate::checkers::ast::Checker;

pub type Settings = FxHashMap<String, ApiBan>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ApiBan {
    /// The message to display when the API is used.
    pub msg: String,
}

/// ## What it does
/// Checks for banned imports.
///
/// ## Why is this bad?
/// Projects may want to ensure that specific modules or module members are
/// not be imported or accessed.
///
/// Security or other company policies may be a reason to impose
/// restrictions on importing external Python libraries. In some cases,
/// projects may adopt conventions around the use of certain modules or
/// module members that are not enforceable by the language itself.
///
/// This rule enforces certain import conventions project-wide in an
/// automatic way.
///
/// ## Options
/// - `flake8-tidy-imports.banned-api`
#[violation]
pub struct BannedApi {
    pub name: String,
    pub message: String,
}

impl Violation for BannedApi {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedApi { name, message } = self;
        format!("`{name}` is banned: {message}")
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
            Range::from(name),
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
                Range::from(located),
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
    if let Some((banned_path, ban)) = checker.ctx.resolve_call_path(expr).and_then(|call_path| {
        checker
            .settings
            .flake8_tidy_imports
            .banned_api
            .iter()
            .find(|(banned_path, ..)| call_path == banned_path.split('.').collect::<CallPath>())
    }) {
        checker.diagnostics.push(Diagnostic::new(
            BannedApi {
                name: banned_path.to_string(),
                message: ban.msg.to_string(),
            },
            Range::from(expr),
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use rustc_hash::FxHashMap;

    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    use super::ApiBan;

    #[test]
    fn banned_api_true_positives() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID251.py"),
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
                    ]),
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
