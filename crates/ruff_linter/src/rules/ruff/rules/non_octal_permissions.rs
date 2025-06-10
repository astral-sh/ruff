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
/// This rule's fix is marked as unsafe because it changes runtime behavior.
///
/// Consider these examples:
///
/// ```python
/// os.chmod("foo", 400)
/// os.chmod("bar", 256)
/// ```
///
///  `400` corresponds to `0o620` (`u=rw,g=w,o=`). If the intention was `0o400`
/// (`u=r,go=`), the fix can be accepted safely, fixing a permissions issue.
///
/// `256` corresponds to `0o400` (`u=r,go=`). It is unlikely that `0o256`
/// (`u=w,g=rx,o=rw`) was the intention here and so the fix should not be
/// accepted. It is recommended to change this case to `0o400` manually.
///
/// ## Fix availability
///
/// A fix is only available if the existing digits could make up a valid octal literal.
#[derive(ViolationMetadata)]
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
    let mode = int.as_u16();

    let suggested = match (mode_literal.starts_with('0'), mode) {
        (true, _) => None,
        (false, Some(found)) => match u16::from_str_radix(&found.to_string(), 8) {
            Ok(suggested) if suggested <= 0o7777 => Some(suggested),
            _ => None,
        },
        _ => None,
    };

    let mut diagnostic = checker.report_diagnostic(NonOctalPermissions, mode_arg.range());
    if let Some(suggested) = suggested {
        let edit = Edit::range_replacement(format!("{suggested:#o}"), mode_arg.range());
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
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
