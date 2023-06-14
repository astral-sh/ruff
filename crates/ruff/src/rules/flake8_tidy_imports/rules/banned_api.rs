use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::from_qualified_name;

use crate::checkers::ast::Checker;

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
    name: String,
    message: String,
}

impl Violation for BannedApi {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedApi { name, message } = self;
        format!("`{name}` is banned: {message}")
    }
}

/// TID251
pub(crate) fn name_is_banned<T>(checker: &mut Checker, name: String, located: &T)
where
    T: Ranged,
{
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if let Some(ban) = banned_api.get(&name) {
        checker.diagnostics.push(Diagnostic::new(
            BannedApi {
                name,
                message: ban.msg.to_string(),
            },
            located.range(),
        ));
    }
}

/// TID251
pub(crate) fn name_or_parent_is_banned<T>(checker: &mut Checker, name: &str, located: &T)
where
    T: Ranged,
{
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    let mut name = name;
    loop {
        if let Some(ban) = banned_api.get(name) {
            checker.diagnostics.push(Diagnostic::new(
                BannedApi {
                    name: name.to_string(),
                    message: ban.msg.to_string(),
                },
                located.range(),
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
pub(crate) fn banned_attribute_access(checker: &mut Checker, expr: &Expr) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if let Some((banned_path, ban)) =
        checker
            .semantic()
            .resolve_call_path(expr)
            .and_then(|call_path| {
                banned_api
                    .iter()
                    .find(|(banned_path, ..)| call_path == from_qualified_name(banned_path))
            })
    {
        checker.diagnostics.push(Diagnostic::new(
            BannedApi {
                name: banned_path.to_string(),
                message: ban.msg.to_string(),
            },
            expr.range(),
        ));
    }
}
