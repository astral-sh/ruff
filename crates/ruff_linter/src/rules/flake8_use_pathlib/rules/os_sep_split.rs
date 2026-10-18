use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, Expr, ExprAttribute, Int, Number};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_sep_split_enabled;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `.split(os.sep)`
///
/// ## Why is this bad?
/// The `pathlib` module in the standard library should be used for path
/// manipulation. It provides a high-level API with the functionality
/// needed for common operations on `Path` objects.
///
/// ## Example
/// If not all parts of the path are needed, then the `name` and `parent`
/// attributes of the `Path` object should be used. Otherwise, the `parts`
/// attribute can be used as shown in the last example.
/// ```python
/// import os
///
/// "path/to/file_name.txt".split(os.sep)[-1]
///
/// "path/to/file_name.txt".split(os.sep)[-2]
///
/// # Iterating over the path parts
/// if any(part in blocklist for part in "my/file/path".split(os.sep)):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("path/to/file_name.txt").name
///
/// Path("path/to/file_name.txt").parent.name
///
/// # Iterating over the path parts
/// if any(part in blocklist for part in Path("my/file/path").parts):
///     ...
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than working directly with strings,
/// especially on older versions of Python.
///
/// ## Fix safety
/// The fix is always unsafe because `Path` normalizes trailing separators and
/// `.` components, which can change the result. Receivers with a custom `split`
/// method can also behave differently. The fix may remove comments.
///
/// ## References
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Pedantic)]
pub(crate) struct OsSepSplit {
    fixable: bool,
}

impl Violation for OsSepSplit {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace `.split(os.sep)` with `Path.parts`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        self.fixable
            .then(|| "Replace with `Path(...).name`".to_string())
    }
}

/// PTH206
pub(crate) fn os_sep_split(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "split" {
        return;
    }

    // Match `.split(os.sep)` or `.split(sep=os.sep)`, but avoid cases in which a `maxsplit` is
    // specified.
    if call.arguments.len() != 1 {
        return;
    }

    let Some(sep) = call.arguments.find_argument_value("sep", 0) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(sep)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "sep"]))
    {
        return;
    }

    let last_item = last_split_item(checker, call).filter(|_| !value.is_bytes_literal_expr());
    let mut diagnostic = checker.report_diagnostic(
        OsSepSplit {
            fixable: last_item.is_some(),
        },
        attr.range(),
    );

    let Some(subscript) = last_item else {
        return;
    };

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;
        let path_range = parenthesized_range(
            value.as_ref().into(),
            call.func.as_ref().into(),
            checker.tokens(),
        )
        .unwrap_or(value.range());

        Ok(Fix::unsafe_edits(
            Edit::range_replacement(
                format!("{binding}({}).name", checker.locator().slice(path_range)),
                subscript.range(),
            ),
            [import_edit],
        ))
    });
}

fn last_split_item<'a>(
    checker: &'a Checker,
    call: &ast::ExprCall,
) -> Option<&'a ast::ExprSubscript> {
    if !is_fix_os_sep_split_enabled(checker.settings()) {
        return None;
    }

    let Expr::Subscript(subscript) = checker.semantic().current_expression_parent()? else {
        return None;
    };
    if subscript.value.range() != call.range() || subscript.ctx != ast::ExprContext::Load {
        return None;
    }

    let Expr::UnaryOp(ast::ExprUnaryOp {
        op: ast::UnaryOp::USub,
        operand,
        ..
    }) = subscript.slice.as_ref()
    else {
        return None;
    };
    if !matches!(
        operand.as_ref(),
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            ..
        })
    ) {
        return None;
    }

    Some(subscript)
}
