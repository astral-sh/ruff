use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq)]
enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

/// ## What it does
/// Checks for `yield`, `yield from`, and `await` usages outside of functions.
///
/// ## Why is this bad?
/// The use of `yield`, `yield from`, or `await` outside of a function will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// class Foo:
///     yield 1
/// ```
///
/// ## Notebook behavior
/// As an exception, `await` is allowed at the top level of a Jupyter notebook
/// (see: [autoawait]).
///
/// ## References
/// - [Python documentation: `yield`](https://docs.python.org/3/reference/simple_stmts.html#the-yield-statement)
///
/// [autoawait]: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html
#[derive(ViolationMetadata)]
pub(crate) struct YieldOutsideFunction {
    keyword: DeferralKeyword,
}

impl Violation for YieldOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction { keyword } = self;
        format!("`{keyword}` statement outside of a function")
    }
}

/// F704
pub(crate) fn yield_outside_function(checker: &Checker, expr: &Expr) {
    let scope = checker.semantic().current_scope();
    if scope.kind.is_module() || scope.kind.is_class() {
        let keyword = match expr {
            Expr::Yield(_) => DeferralKeyword::Yield,
            Expr::YieldFrom(_) => DeferralKeyword::YieldFrom,
            Expr::Await(_) => DeferralKeyword::Await,
            _ => return,
        };

        // `await` is allowed at the top level of a Jupyter notebook.
        // See: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html.
        if scope.kind.is_module()
            && checker.source_type.is_ipynb()
            && keyword == DeferralKeyword::Await
        {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(
            YieldOutsideFunction { keyword },
            expr.range(),
        ));
    }
}
