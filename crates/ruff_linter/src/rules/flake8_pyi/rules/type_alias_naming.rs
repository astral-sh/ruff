use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type aliases that do not use the CamelCase naming convention.
///
/// ## Why is this bad?
/// It's conventional to use the CamelCase naming convention for type aliases,
/// to distinguish them from other variables.
///
/// ## Example
/// ```pyi
/// type_alias_name: TypeAlias = int
/// ```
///
/// Use instead:
/// ```pyi
/// TypeAliasName: TypeAlias = int
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SnakeCaseTypeAlias {
    name: String,
}

impl Violation for SnakeCaseTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Type alias `{name}` should be CamelCase")
    }
}

/// ## What it does
/// Checks for private type alias definitions suffixed with 'T'.
///
/// ## Why is this bad?
/// It's conventional to use the 'T' suffix for type variables; the use of
/// such a suffix implies that the object is a `TypeVar`.
///
/// Adding the 'T' suffix to a non-`TypeVar`, it can be misleading and should
/// be avoided.
///
/// ## Example
/// ```pyi
/// from typing import TypeAlias
///
/// _MyTypeT: TypeAlias = int
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import TypeAlias
///
/// _MyType: TypeAlias = int
/// ```
///
/// ## References
/// - [PEP 484: Type Aliases](https://peps.python.org/pep-0484/#type-aliases)
#[derive(ViolationMetadata)]
pub(crate) struct TSuffixedTypeAlias {
    name: String,
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
pub(crate) fn snake_case_type_alias(checker: &Checker, target: &Expr) {
    if let Expr::Name(ast::ExprName { id, range, .. }) = target {
        if !is_snake_case_type_alias(id) {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(
            SnakeCaseTypeAlias {
                name: id.to_string(),
            },
            *range,
        ));
    }
}

/// PYI043
pub(crate) fn t_suffixed_type_alias(checker: &Checker, target: &Expr) {
    if let Expr::Name(ast::ExprName { id, range, .. }) = target {
        if !is_t_suffixed_type_alias(id) {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(
            TSuffixedTypeAlias {
                name: id.to_string(),
            },
            *range,
        ));
    }
}
