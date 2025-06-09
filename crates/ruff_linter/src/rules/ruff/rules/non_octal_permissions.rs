use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::{SemanticModel, analyze};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for `chmod` calls which use non-octal integer literals.
///
/// ## Why is this bad?
///
/// Numeric modes are made up of one to four octal digits. Converting a non-octal
/// integer to octal may not be the mode the author intended.
///
/// ## Example
///
/// ```python
/// os.chmod("foo", 644)
/// ```
///
/// Use instead:
///
/// ```python
/// os.chmod("foo", 0o644)
/// ```
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe because it changes runtime behavior.
///
/// ## Fix availability
///
/// A fix is only available if the existing digits could make up a valid octal literal.
#[derive(ViolationMetadata)]
pub(crate) struct NonOctalPermissions {
    reason: Reason,
}

#[derive(Debug, Clone, Copy)]
enum Reason {
    Decimal { found: u16, suggested: u16 },
    Invalid,
}

impl Violation for NonOctalPermissions {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.reason {
            Reason::Decimal { found, suggested } => {
                format!("Non-octal mode `{found}`, did you mean `{suggested:#o}`?")
            }
            Reason::Invalid => "Non-octal mode".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with octal literal".to_string())
    }
}

/// RUF064
pub(crate) fn non_octal_permissions(checker: &Checker, call: &ExprCall) {
    let mode_arg_index = if is_os_chmod(&call.func, checker.semantic()) {
        1
    } else if is_pathlib_chmod(&call.func, checker.semantic()) {
        0
    } else {
        return;
    };

    let Some(mode_arg) = call.arguments.find_argument_value("mode", mode_arg_index) else {
        return;
    };

    let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: ast::Number::Int(int),
        ..
    }) = mode_arg
    else {
        return;
    };

    let mode_literal = &checker.locator().contents()[mode_arg.range()];

    if mode_literal.starts_with("0o") || mode_literal.starts_with("0O") || mode_literal == "0" {
        return;
    }
    let mode = int.as_u16();

    let reason = match (mode_literal.starts_with('0'), mode) {
        (true, _) => Reason::Invalid,
        (false, Some(found)) => match u16::from_str_radix(&found.to_string(), 8) {
            Ok(suggested) if suggested <= 0o7777 => Reason::Decimal { found, suggested },
            _ => Reason::Invalid,
        },
        _ => Reason::Invalid,
    };

    let mut diagnostic =
        checker.report_diagnostic(NonOctalPermissions { reason }, mode_arg.range());
    if let Reason::Decimal { suggested, .. } = reason {
        let edit = Edit::range_replacement(format!("{suggested:#o}"), mode_arg.range());
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
}

/// Returns `true` if an expression resolves to `os.chmod`, `os.fchmod`, or
/// `os.lchmod`.
fn is_os_chmod(func: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    matches!(
        qualified_name.segments(),
        ["os", "chmod" | "fchmod" | "lchmod"]
    )
}

/// Returns `true` if an expression resolves to a `chmod` or `lchmod` call
/// to any concrete `pathlib.Path` class.
fn is_pathlib_chmod(func: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func else {
        return false;
    };

    if attr != "chmod" && attr != "lchmod" {
        return false;
    }

    // First: is this an inlined call to `pathlib.Path.chmod`?
    // ```python
    // from pathlib import Path
    // Path("foo").chmod(0o644)
    // ```
    if let Expr::Call(call) = value.as_ref() {
        if is_concrete_pathlib_path_call(semantic, &call.func) {
            return true;
        }
    }

    // Second, is this a call to `pathlib.Path.chmod` via a variable?
    // ```python
    // from pathlib import Path
    // path = Path("foo")
    // path.chmod()
    // ```
    let Expr::Name(name) = value.as_ref() else {
        return false;
    };

    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };

    let binding = semantic.binding(binding_id);

    let Some(Expr::Call(call)) = analyze::typing::find_binding_value(binding, semantic) else {
        return false;
    };

    is_concrete_pathlib_path_call(semantic, &call.func)
}

fn is_concrete_pathlib_path_call(semantic: &SemanticModel, func: &Expr) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };
    matches!(
        qualified_name.segments(),
        ["pathlib", "Path" | "PosixPath" | "WindowsPath"]
    )
}
