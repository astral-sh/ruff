use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Expr, Located};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation, CacheKey};
use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::types::Range;

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
pub fn name_is_banned<T>(checker: &mut Checker, name: String, located: &Located<T>) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if let Some(ban) = banned_api.get(&name) {
        checker.diagnostics.push(Diagnostic::new(
            BannedApi {
                name,
                message: ban.msg.to_string(),
            },
            Range::from(located),
        ));
    }
}

/// TID251
pub fn name_or_parent_is_banned<T>(checker: &mut Checker, name: &str, located: &Located<T>) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    let mut name = name;
    loop {
        if let Some(ban) = banned_api.get(name) {
            checker.diagnostics.push(Diagnostic::new(
                BannedApi {
                    name: name.to_string(),
                    message: ban.msg.to_string(),
                },
                Range::from(located),
            ));
            return;
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return,
        }
    }
}

/// TID251
pub fn banned_attribute_access(checker: &mut Checker, expr: &Expr) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if let Some((banned_path, ban)) = checker.ctx.resolve_call_path(expr).and_then(|call_path| {
        banned_api
            .iter()
            .find(|(banned_path, ..)| call_path == from_qualified_name(banned_path))
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
    use rustc_hash::FxHashMap;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    use super::ApiBan;

    #[test]
    fn banned_api() -> Result<()> {
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn banned_api_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "attrs".to_string(),
                            ApiBan {
                                msg: "The attrs module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "my_package.sublib.protocol".to_string(),
                            ApiBan {
                                msg: "The protocol module is deprecated.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
