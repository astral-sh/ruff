use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
use crate::codes::Category;

/// ## Removed
/// This rule is identical to [S307] which should be used instead.
///
/// ## What it does
/// Checks for uses of the builtin `eval()` function.
///
/// ## Why is this bad?
/// The `eval()` function is insecure as it enables arbitrary code execution.
///
/// ## Example
/// ```python
/// def foo():
///     x = eval(input("Enter a number: "))
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     x = input("Enter a number: ")
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `eval`](https://docs.python.org/3/library/functions.html#eval)
/// - [_Eval really is dangerous_ by Ned Batchelder](https://nedbatchelder.com/blog/201206/eval_really_is_dangerous.html)
///
/// [S307]: https://docs.astral.sh/ruff/rules/eval/
#[derive(ViolationMetadata)]
#[violation_metadata(removed_since = "v0.2.0", category = Category::Security)]
pub(crate) struct PygrepHooksEval;

/// PGH001
impl Violation for PygrepHooksEval {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No builtin `eval()` allowed".to_string()
    }
}
