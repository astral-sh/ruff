use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for uses of `bin(...)[2:]` (or `hex`, or `oct`) to convert
/// an integer into a string with no base prefix.
///
/// ## Why is this bad?
/// Using f-strings is more concise and readable than using the
/// `bin`, `hex`, or `oct` functions followed by slicing them.
///
/// ## Example
/// ```python
/// print(bin(1337)[2:])
/// ```
///
/// Use instead:
/// ```python
/// print(f"{1337:b}")
/// ```
#[violation]
pub struct FStringNumberFormat {
    replacement: Option<SourceCodeSnippet>,
    int_base: IntBase,
}

impl Violation for FStringNumberFormat {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let FStringNumberFormat {
            replacement,
            int_base,
        } = self;
        let function_name = int_base.function_name();

        if let Some(snippet) = replacement {
            if let Some(full_display) = snippet.full_display() {
                return format!("Replace `{function_name}` parsing with `{full_display}`.");
            }
        }
        format!("Replace `{function_name}` parsing with f-string.")
    }

    fn fix_title(&self) -> Option<String> {
        let FStringNumberFormat {
            replacement,
            int_base,
        } = self;
        let function_name = int_base.function_name();

        if let Some(snippet) = replacement {
            if let Some(full_display) = snippet.full_display() {
                return Some(format!(
                    "Replace `{function_name}` parsing with `{full_display}`."
                ));
            }
        }
        Some(format!("Replace `{function_name}` parsing with f-string."))
    }
}

/// FURB116
pub(crate) fn fstring_number_format(checker: &mut Checker, subscript: &ast::ExprSubscript) {
    // Validate the slice to be what we expect, `[2:]`
    let Expr::Slice(ast::ExprSlice {
        lower: Some(lower),
        upper: None,
        step: None,
        ..
    }) = subscript.slice.as_ref()
    else {
        return;
    };

    let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: ast::Number::Int(int),
        ..
    }) = lower.as_ref()
    else {
        return;
    };

    // Ensure the starting slice value is 2
    if *int != 2 {
        return;
    }

    // Validate the value to be an integer base-changing function call
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = subscript.value.as_ref()
    else {
        return;
    };

    let [inner] = &*arguments.args else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };

    if !checker.semantic().is_builtin(id) {
        return;
    }

    let Some(base) = IntBase::from_str(id) else {
        return;
    };

    // we can't always generate a replacement for this lint, and we'll just
    // emit a diagnostic
    let mut replacement = None;
    let mut fix = None;
    if matches!(
        inner,
        Expr::NumberLiteral(_) | Expr::Name(_) | Expr::Attribute(_)
    ) {
        let inner_source = checker.locator().slice(inner);

        let quote = checker.stylist().quote();
        let shorthand = base.shorthand();

        let replacement_string = format!("f{quote}{{{inner_source}:{shorthand}}}{quote}");
        replacement = Some(SourceCodeSnippet::from_str(&replacement_string));

        fix = Some(Fix::safe_edit(Edit::range_replacement(
            replacement_string,
            subscript.range(),
        )));
    }

    let mut diagnostic = Diagnostic::new(
        FStringNumberFormat {
            replacement,
            int_base: base,
        },
        subscript.range(),
    );

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }

    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum IntBase {
    Hex,
    Bin,
    Oct,
}

impl IntBase {
    fn shorthand(self) -> &'static str {
        match self {
            IntBase::Hex => "x",
            IntBase::Bin => "b",
            IntBase::Oct => "o",
        }
    }

    fn function_name(self) -> &'static str {
        match self {
            IntBase::Hex => "hex",
            IntBase::Bin => "bin",
            IntBase::Oct => "oct",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "hex" => Some(IntBase::Hex),
            "bin" => Some(IntBase::Bin),
            "oct" => Some(IntBase::Oct),
            _ => None,
        }
    }
}
