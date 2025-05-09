use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprCall, Number, PythonVersion, UnaryOp};
use ruff_source_file::find_newline;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for uses of `bin(...)[2:]` (or `hex`, or `oct`) to convert
/// an integer into a string.
///
/// ## Why is this bad?
/// When converting an integer to a baseless binary, hexadecimal, or octal
/// string, using f-strings is more concise and readable than using the
/// `bin`, `hex`, or `oct` functions followed by a slice.
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
///
/// ## Fix safety
/// The fix is only marked as safe for integer literals, all other cases
/// are display-only, as they may change the runtime behaviour of the program
/// or introduce syntax errors.
#[derive(ViolationMetadata)]
pub(crate) struct FStringNumberFormat {
    replacement: Option<SourceCodeSnippet>,
    base: Base,
}

impl Violation for FStringNumberFormat {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let FStringNumberFormat { replacement, base } = self;
        let function_name = base.function_name();

        if let Some(display) = replacement
            .as_ref()
            .and_then(SourceCodeSnippet::full_display)
        {
            format!("Replace `{function_name}` call with `{display}`")
        } else {
            format!("Replace `{function_name}` call with f-string")
        }
    }

    fn fix_title(&self) -> Option<String> {
        if let Some(display) = self
            .replacement
            .as_ref()
            .and_then(SourceCodeSnippet::full_display)
        {
            Some(format!("Replace with `{display}`"))
        } else {
            Some("Replace with f-string".to_string())
        }
    }
}

/// FURB116
pub(crate) fn fstring_number_format(checker: &Checker, subscript: &ast::ExprSubscript) {
    // The slice must be exactly `[2:]`.
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

    if *int != 2 {
        return;
    }

    // The call must be exactly `hex(...)`, `bin(...)`, or `oct(...)`.
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = subscript.value.as_ref()
    else {
        return;
    };

    if !arguments.keywords.is_empty() {
        return;
    }

    let [arg] = &*arguments.args else {
        return;
    };

    let Some(id) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };

    let Some(base) = Base::from_str(id) else {
        return;
    };

    // float and complex numbers are false positives, ignore them.
    if matches!(
        arg,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: Number::Float(_) | Number::Complex { .. },
            ..
        })
    ) {
        return;
    }

    let maybe_number = if let Some(maybe_number) = arg
        .as_unary_op_expr()
        .filter(|unary_expr| unary_expr.op == UnaryOp::UAdd)
        .map(|unary_expr| &unary_expr.operand)
    {
        maybe_number
    } else {
        arg
    };

    let applicability = if matches!(maybe_number, Expr::NumberLiteral(_)) {
        Applicability::Safe
    } else {
        Applicability::DisplayOnly
    };

    let replacement = try_create_replacement(checker, arg, base);

    let mut diagnostic = Diagnostic::new(
        FStringNumberFormat {
            replacement: replacement.as_deref().map(SourceCodeSnippet::from_str),
            base,
        },
        subscript.range(),
    );

    if let Some(replacement) = replacement {
        let edit = Edit::range_replacement(replacement, subscript.range());
        diagnostic.set_fix(Fix::applicable_edit(edit, applicability));
    }

    checker.report_diagnostic(diagnostic);
}

/// Generate a replacement, if possible.
fn try_create_replacement(checker: &Checker, arg: &Expr, base: Base) -> Option<String> {
    if !matches!(
        arg,
        Expr::NumberLiteral(_) | Expr::Name(_) | Expr::Attribute(_) | Expr::UnaryOp(_)
    ) {
        return None;
    }

    let inner_source = checker.locator().slice(arg);

    // On Python 3.11 and earlier, trying to replace an `arg` that contains a backslash
    // would create a `SyntaxError` in the f-string.
    if checker.target_version() <= PythonVersion::PY311 && inner_source.contains('\\') {
        return None;
    }

    // On Python 3.11 and earlier, trying to replace an `arg` that spans multiple lines
    // would create a `SyntaxError` in the f-string.
    if checker.target_version() <= PythonVersion::PY311 && find_newline(inner_source).is_some() {
        return None;
    }

    let quote = checker.stylist().quote();
    let shorthand = base.shorthand();

    // If the `arg` contains double quotes we need to create the f-string with single quotes
    // to avoid a `SyntaxError` in Python 3.11 and earlier.
    if checker.target_version() <= PythonVersion::PY311 && inner_source.contains(quote.as_str()) {
        return None;
    }

    // If the `arg` contains a brace add an space before it to avoid a `SyntaxError`
    // in the f-string.
    if inner_source.starts_with('{') {
        Some(format!("f{quote}{{ {inner_source}:{shorthand}}}{quote}"))
    } else {
        Some(format!("f{quote}{{{inner_source}:{shorthand}}}{quote}"))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Base {
    Hex,
    Bin,
    Oct,
}

impl Base {
    /// Returns the shorthand for the base.
    fn shorthand(self) -> &'static str {
        match self {
            Base::Hex => "x",
            Base::Bin => "b",
            Base::Oct => "o",
        }
    }

    /// Returns the builtin function name for the base.
    fn function_name(self) -> &'static str {
        match self {
            Base::Hex => "hex",
            Base::Bin => "bin",
            Base::Oct => "oct",
        }
    }

    /// Parses the base from a string.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "hex" => Some(Base::Hex),
            "bin" => Some(Base::Bin),
            "oct" => Some(Base::Oct),
            _ => None,
        }
    }
}
