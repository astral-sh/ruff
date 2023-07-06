use ruff_diagnostics::Violation;
use rustpython_parser::ast::ExprCall;

use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for regular expressions that can, under certain inputs, lead to catastrophic
/// backtracking in the Python `re` module.
///
/// ## Why is this bad?
/// Catastrophic backtracking will often lead to denial-of-service. Catastrophic cases may take
/// days, weeks, or years to complete which may leave your service degraded or unusable.
///
/// ## Example
/// ```python
/// import re
///
/// subject = 'a' * 64
/// re.search(r'(.|[abc])+z', subject)  # Boom
/// ```
///
/// Use instead:
/// ```python
/// import re
///
/// subject = 'a' * 64
/// re.search(r'.+z', subject)
/// ```
///
/// ## References
/// - [Runaway Regular Expressions: Catastrophic Backtracking](https://www.regular-expressions.info/catastrophic.html)
/// - [Preventing Regular Expression Denial of Service (ReDoS)](https://www.regular-expressions.info/redos.html)
#[violation]
pub struct CatastrophicReUse;

impl Violation for CatastrophicReUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Potentially dangerous regex expression can lead to catastrophic backtracking")
    }
}

/// DUO138
pub(crate) fn catastrophic_re_use(checker: &mut Checker, call: &ExprCall) {}
