use std::fmt;

use ruff_python_ast::{self as ast};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to `re.sub`, `re.subn`, and `re.split` that pass `count`,
/// `maxsplit`, or `flags` as positional arguments.
///
/// ## Why is this bad?
/// Passing `count`, `maxsplit`, or `flags` as positional arguments to
/// `re.sub`, `re.subn`, or `re.split` can lead to confusion, as most methods in
/// the `re` module accept `flags` as the third positional argument, while
/// `re.sub`, `re.subn`, and `re.split` have different signatures.
///
/// Instead, pass `count`, `maxsplit`, and `flags` as keyword arguments.
///
/// ## Example
/// ```python
/// import re
///
/// re.split("pattern", "replacement", 1)
/// ```
///
/// Use instead:
/// ```python
/// import re
///
/// re.split("pattern", "replacement", maxsplit=1)
/// ```
///
/// ## References
/// - [Python documentation: `re.sub`](https://docs.python.org/3/library/re.html#re.sub)
/// - [Python documentation: `re.subn`](https://docs.python.org/3/library/re.html#re.subn)
/// - [Python documentation: `re.split`](https://docs.python.org/3/library/re.html#re.split)
#[violation]
pub struct ReSubPositionalArgs {
    method: Method,
}

impl Violation for ReSubPositionalArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ReSubPositionalArgs { method } = self;
        let param_name = method.param_name();
        format!(
            "`{method}` should pass `{param_name}` and `flags` as keyword arguments to avoid confusion due to unintuitive argument positions"
        )
    }
}

/// B034
pub(crate) fn re_sub_positional_args(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::RE) {
        return;
    }

    let Some(method) = checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .and_then(|qualified_name| match qualified_name.segments() {
            ["re", "sub"] => Some(Method::Sub),
            ["re", "subn"] => Some(Method::Subn),
            ["re", "split"] => Some(Method::Split),
            _ => None,
        })
    else {
        return;
    };

    if call.arguments.args.len() > method.num_args() {
        checker.diagnostics.push(Diagnostic::new(
            ReSubPositionalArgs { method },
            call.range(),
        ));
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Method {
    Sub,
    Subn,
    Split,
}

impl fmt::Display for Method {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Sub => fmt.write_str("re.sub"),
            Self::Subn => fmt.write_str("re.subn"),
            Self::Split => fmt.write_str("re.split"),
        }
    }
}

impl Method {
    const fn param_name(self) -> &'static str {
        match self {
            Self::Sub => "count",
            Self::Subn => "count",
            Self::Split => "maxsplit",
        }
    }

    const fn num_args(self) -> usize {
        match self {
            Self::Sub => 3,
            Self::Subn => 3,
            Self::Split => 2,
        }
    }
}
