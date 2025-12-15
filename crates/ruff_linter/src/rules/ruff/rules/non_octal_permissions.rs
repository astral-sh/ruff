use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::{SemanticModel, analyze};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for standard library functions which take a numeric `mode` argument
/// where a non-octal integer literal is passed.
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
/// There are two categories of fix, the first of which is where it looks like
/// the author intended to use an octal literal but the `0o` prefix is missing:
///
/// ```python
/// os.chmod("foo", 400)
/// os.chmod("foo", 644)
/// ```
///
/// This class of fix changes runtime behaviour. In the first case, `400`
/// corresponds to `0o620` (`u=rw,g=w,o=`). As this mode is not deemed likely,
/// it is changed to `0o400` (`u=r,go=`). Similarly, `644` corresponds to
/// `0o1204` (`u=ws,g=,o=r`) and is changed to `0o644` (`u=rw,go=r`).
///
/// The second category is decimal literals which are recognised as likely valid
/// but in decimal form:
///
/// ```python
/// os.chmod("foo", 256)
/// os.chmod("foo", 493)
/// ```
///
/// `256` corresponds to `0o400` (`u=r,go=`) and `493` corresponds to `0o755`
/// (`u=rwx,go=rx`). Both of these fixes keep runtime behavior unchanged. If the
/// original code really intended to use `0o256` (`u=w,g=rx,o=rw`) instead of
/// `256`, this fix should not be accepted.
///
/// As a special case, zero is allowed to omit the `0o` prefix unless it has
/// multiple digits:
///
/// ```python
/// os.chmod("foo", 0)  # Ok
/// os.chmod("foo", 0o000)  # Ok
/// os.chmod("foo", 000)  # Lint emitted and fix suggested
/// ```
///
/// Ruff will suggest a safe fix for multi-digit zeros to add the `0o` prefix.
///
/// ## Fix availability
///
/// A fix is only available if the integer literal matches a set of common modes.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.12.1")]
pub(crate) struct NonOctalPermissions;

impl Violation for NonOctalPermissions {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Non-octal mode".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with octal literal".to_string())
    }
}

/// RUF064
pub(crate) fn non_octal_permissions(checker: &Checker, call: &ExprCall) {
    let mode_arg = find_func_mode_arg(call, checker.semantic())
        .or_else(|| find_method_mode_arg(call, checker.semantic()));

    let Some(mode_arg) = mode_arg else {
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

    let mut diagnostic = checker.report_diagnostic(NonOctalPermissions, mode_arg.range());

    // Don't suggest a fix for 0x or 0b literals.
    if mode_literal.starts_with("0x") || mode_literal.starts_with("0b") {
        return;
    }

    if mode_literal.chars().all(|c| c == '0') {
        // Fix e.g. 000 as 0o000
        let edit = Edit::range_replacement(format!("0o{mode_literal}"), mode_arg.range());
        diagnostic.set_fix(Fix::safe_edit(edit));
        return;
    }

    let Some(suggested) = int.as_u16().and_then(suggest_fix) else {
        return;
    };

    let edit = Edit::range_replacement(format!("{suggested:#o}"), mode_arg.range());
    diagnostic.set_fix(Fix::unsafe_edit(edit));
}

fn find_func_mode_arg<'a>(call: &'a ExprCall, semantic: &SemanticModel) -> Option<&'a Expr> {
    let qualified_name = semantic.resolve_qualified_name(&call.func)?;

    match qualified_name.segments() {
        ["os", "umask"] => call.arguments.find_argument_value("mode", 0),
        [
            "os",
            "chmod" | "fchmod" | "lchmod" | "mkdir" | "makedirs" | "mkfifo" | "mknod",
        ] => call.arguments.find_argument_value("mode", 1),
        ["os", "open"] => call.arguments.find_argument_value("mode", 2),
        ["dbm", "open"] | ["dbm", "gnu" | "ndbm", "open"] => {
            call.arguments.find_argument_value("mode", 2)
        }
        _ => None,
    }
}

fn find_method_mode_arg<'a>(call: &'a ExprCall, semantic: &SemanticModel) -> Option<&'a Expr> {
    let (type_name, attr_name) = resolve_method_call(&call.func, semantic)?;

    match (type_name.segments(), attr_name) {
        (
            ["pathlib", "Path" | "PosixPath" | "WindowsPath"],
            "chmod" | "lchmod" | "mkdir" | "touch",
        ) => call.arguments.find_argument_value("mode", 0),
        _ => None,
    }
}

fn resolve_method_call<'a>(
    func: &'a Expr,
    semantic: &'a SemanticModel,
) -> Option<(QualifiedName<'a>, &'a str)> {
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func else {
        return None;
    };

    // First: is this an inlined call like `pathlib.Path.chmod`?
    // ```python
    // from pathlib import Path
    // Path("foo").chmod(0o644)
    // ```
    if let Expr::Call(call) = value.as_ref() {
        let qualified_name = semantic.resolve_qualified_name(&call.func)?;
        return Some((qualified_name, attr));
    }

    // Second, is this a call like `pathlib.Path.chmod` via a variable?
    // ```python
    // from pathlib import Path
    // path = Path("foo")
    // path.chmod()
    // ```
    let Expr::Name(name) = value.as_ref() else {
        return None;
    };

    let binding_id = semantic.resolve_name(name)?;

    let binding = semantic.binding(binding_id);

    let Some(Expr::Call(call)) = analyze::typing::find_binding_value(binding, semantic) else {
        return None;
    };

    let qualified_name = semantic.resolve_qualified_name(&call.func)?;

    Some((qualified_name, attr))
}

/// Try to determine whether the integer literal
fn suggest_fix(mode: u16) -> Option<u16> {
    // These suggestions are in the form of
    // <missing `0o` prefix> | <mode as decimal> => <octal>
    // If <as decimal> could theoretically be a valid octal literal, the
    // comment explains why it's deemed unlikely to be intentional.
    match mode {
        400 | 256 => Some(0o400), // -w-r-xrw-, group/other > user unlikely
        440 | 288 => Some(0o440),
        444 | 292 => Some(0o444),
        600 | 384 => Some(0o600),
        640 | 416 => Some(0o640), // r----xrw-, other > user unlikely
        644 | 420 => Some(0o644), // r---w----, group write but not read unlikely
        660 | 432 => Some(0o660), // r---wx-w-, write but not read unlikely
        664 | 436 => Some(0o664), // r---wxrw-, other > user unlikely
        666 | 438 => Some(0o666),
        700 | 448 => Some(0o700),
        744 | 484 => Some(0o744),
        750 | 488 => Some(0o750),
        755 | 493 => Some(0o755),
        770 | 504 => Some(0o770), // r-x---r--, other > group unlikely
        775 | 509 => Some(0o775),
        776 | 510 => Some(0o776), // r-x--x---, seems unlikely
        777 | 511 => Some(0o777), // r-x--x--x, seems unlikely
        _ => None,
    }
}
