use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, PythonVersion, StringFlags};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing::{self, PathlibPathChecker, TypeChecker};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `pathlib.Path.with_suffix()` calls where
/// the given suffix does not have a leading dot
/// or the given suffix is a single dot `"."` and the
/// Python version is less than 3.14.
///
/// ## Why is this bad?
/// `Path.with_suffix()` will raise an error at runtime
/// if the given suffix is not prefixed with a dot
/// or, in versions prior to Python 3.14, if it is a single dot `"."`.
///
/// ## Example
///
/// ```python
/// from pathlib import Path
///
/// path = Path()
///
/// path.with_suffix("py")
/// ```
///
/// Use instead:
///
/// ```python
/// from pathlib import Path
///
/// path = Path()
///
/// path.with_suffix(".py")
/// ```
///
/// ## Known problems
/// This rule is likely to have false negatives, as Ruff can only emit the
/// lint if it can say for sure that a binding refers to a `Path` object at
/// runtime. Due to type inference limitations, Ruff is currently only
/// confident about this if it can see that the binding originates from a
/// function parameter annotated with `Path` or from a direct assignment to a
/// `Path()` constructor call.
///
/// ## Fix safety
/// The fix for this rule adds a leading period to the string passed
/// to the `with_suffix()` call. This fix is marked as unsafe, as it
/// changes runtime behaviour: the call would previously always have
/// raised an exception, but no longer will.
///
/// Moreover, it's impossible to determine if this is the correct fix
/// for a given situation (it's possible that the string was correct
/// but was being passed to the wrong method entirely, for example).
///
/// No fix is offered if the suffix `"."` is given, since the intent is unclear.
#[derive(ViolationMetadata)]
pub(crate) struct InvalidPathlibWithSuffix {
    single_dot: bool,
}

impl Violation for InvalidPathlibWithSuffix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.single_dot {
            "Invalid suffix passed to `.with_suffix()`".to_string()
        } else {
            "Dotless suffix passed to `.with_suffix()`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = if self.single_dot {
            "Remove \".\" or extend to valid suffix"
        } else {
            "Add a leading dot"
        };
        Some(title.to_string())
    }
}

/// PTH210
pub(crate) fn invalid_pathlib_with_suffix(checker: &Checker, call: &ast::ExprCall) {
    let (func, arguments) = (&call.func, &call.arguments);

    if !is_path_with_suffix_call(checker.semantic(), func) {
        return;
    }

    if arguments.len() > 1 {
        return;
    }

    let Some(ast::Expr::StringLiteral(string)) = arguments.find_argument_value("suffix", 0) else {
        return;
    };

    let string_value = string.value.to_str();

    if string_value.is_empty() {
        return;
    }

    if string_value.starts_with('.') && string_value.len() > 1 {
        return;
    }

    let [first_part, ..] = string.value.as_slice() else {
        return;
    };

    let single_dot = string_value == ".";

    // As of Python 3.14, a single dot is considered a valid suffix.
    // https://docs.python.org/3.14/library/pathlib.html#pathlib.PurePath.with_suffix
    if single_dot && checker.target_version() >= PythonVersion::PY314 {
        return;
    }

    let mut diagnostic =
        checker.report_diagnostic(InvalidPathlibWithSuffix { single_dot }, call.range);
    if !single_dot {
        let after_leading_quote = string.start() + first_part.flags.opener_len();
        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
            ".".to_string(),
            after_leading_quote,
        )));
    }
}

fn is_path_with_suffix_call(semantic: &SemanticModel, func: &ast::Expr) -> bool {
    let ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func else {
        return false;
    };

    if attr != "with_suffix" {
        return false;
    }

    match &**value {
        ast::Expr::Name(name) => {
            let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
                return false;
            };
            typing::is_pathlib_path(binding, semantic)
        }
        expr => PathlibPathChecker::match_initializer(expr, semantic),
    }
}
