use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_tidy_imports::matchers::NameMatchPolicy;

/// ## What it does
/// Checks for banned imports.
///
/// ## Why is this bad?
/// Projects may want to ensure that specific modules or module members are
/// not imported or accessed.
///
/// Security or other company policies may be a reason to impose
/// restrictions on importing external Python libraries. In some cases,
/// projects may adopt conventions around the use of certain modules or
/// module members that are not enforceable by the language itself.
///
/// This rule enforces certain import conventions project-wide automatically.
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-api`
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
pub(crate) fn banned_api<T: Ranged>(checker: &mut Checker, policy: &NameMatchPolicy, node: &T) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if let Some(banned_module) = policy.find(banned_api.keys().map(AsRef::as_ref)) {
        if let Some(reason) = banned_api.get(&banned_module) {
            checker.diagnostics.push(Diagnostic::new(
                BannedApi {
                    name: banned_module,
                    message: reason.msg.to_string(),
                },
                node.range(),
            ));
        }
    }
}

/// TID251
pub(crate) fn banned_attribute_access(checker: &mut Checker, expr: &Expr) {
    let banned_api = &checker.settings.flake8_tidy_imports.banned_api;
    if banned_api.is_empty() {
        return;
    }

    if let Some((banned_path, ban)) =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualified_name| {
                banned_api.iter().find(|(banned_path, ..)| {
                    qualified_name == QualifiedName::from_dotted_name(banned_path)
                })
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
