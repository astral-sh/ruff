use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `debug=True` in Flask.
///
/// ## Why is this bad?
/// Enabling debug mode shows an interactive debugger in the browser if an
/// error occurs, and allows running arbitrary Python code from the browser.
/// This could leak sensitive information, or allow an attacker to run
/// arbitrary code.
///
/// ## Example
/// ```python
/// import flask
///
/// app = Flask()
///
/// app.run(debug=True)
/// ```
///
/// Use instead:
/// ```python
/// import flask
///
/// app = Flask()
///
/// app.run(debug=os.environ["ENV"] == "dev")
/// ```
///
/// ## References
/// - [Flask documentation: Debug Mode](https://flask.palletsprojects.com/en/latest/quickstart/#debug-mode)
#[derive(ViolationMetadata)]
pub(crate) struct FlaskDebugTrue;

impl Violation for FlaskDebugTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `debug=True` in Flask app detected".to_string()
    }
}

/// S201
pub(crate) fn flask_debug_true(checker: &Checker, call: &ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "run" {
        return;
    }

    let Some(debug_argument) = call.arguments.find_keyword("debug") else {
        return;
    };

    if !is_const_true(&debug_argument.value) {
        return;
    }

    if typing::resolve_assignment(value, checker.semantic())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["flask", "Flask"]))
    {
        checker.report_diagnostic(Diagnostic::new(FlaskDebugTrue, debug_argument.range()));
    }
}
