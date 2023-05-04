use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct SnakeCaseTypeAlias {
    pub name: String,
}

impl Violation for SnakeCaseTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Type alias `{name}` should be CamelCase")
    }
}

#[violation]
pub struct TSuffixedTypeAlias {
    pub name: String,
}

impl Violation for TSuffixedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Private type alias `{name}` should not be suffixed with `T` (the `T` suffix implies that an object is a `TypeVar`)")
    }
}

/// Return `true` if the given name is a `snake_case` type alias. In this context, we match against
/// any name that begins with an optional underscore, followed by at least one lowercase letter.
fn is_snake_case_type_alias(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('_'), Some('0'..='9' | 'a'..='z')) | (Some('0'..='9' | 'a'..='z'), ..)
    )
}

/// Return `true` if the given name is a T-suffixed type alias. In this context, we match against
/// any name that begins with an underscore, and ends in a lowercase letter, followed by `T`,
/// followed by an optional digit.
fn is_t_suffixed_type_alias(name: &str) -> bool {
    // A T-suffixed, private type alias must begin with an underscore.
    if !name.starts_with('_') {
        return false;
    }

    // It must end in a lowercase letter, followed by `T`, and (optionally) a digit.
    let mut chars = name.chars().rev();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some('0'..='9'), Some('T'), Some('a'..='z')) | (Some('T'), Some('a'..='z'), _)
    )
}

/// PYI042
pub fn snake_case_type_alias(checker: &mut Checker, target: &Expr) {
    if let ExprKind::Name { id, .. } = target.node() {
        if !is_snake_case_type_alias(id) {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            SnakeCaseTypeAlias {
                name: id.to_string(),
            },
            target.range(),
        ));
    }
}

/// PYI043
pub fn t_suffixed_type_alias(checker: &mut Checker, target: &Expr) {
    if let ExprKind::Name { id, .. } = target.node() {
        if !is_t_suffixed_type_alias(id) {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            TSuffixedTypeAlias {
                name: id.to_string(),
            },
            target.range(),
        ));
    }
}
