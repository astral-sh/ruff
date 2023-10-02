use crate::checkers::ast::Checker;
use crate::rules::pyupgrade::rules::is_open_builtin;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::derive_message_formats;
use ruff_macros::violation;
use ruff_python_ast::Constant;
use ruff_python_ast::Expr;
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

#[violation]
pub struct BadOpenMode {
    mode: String,
}

impl Violation for BadOpenMode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "{:#?} is not a valid mode for open. Python supports r, w, a[, x] modes with b, +",
            self.mode
        )
    }
}

/// PLW1501
pub(crate) fn bad_open_mode(checker: &mut Checker, call: &ExprCall) {
    if !is_open_builtin(call.func.as_ref(), checker.semantic()) {
        return;
    }
    let Some(Expr::Constant(a)) = call.arguments.find_argument("mode", 1) else {
        return;
    };
    let Constant::Str(s) = &a.value else { return };
    if !is_open_mode_good(s.value.as_str()) {
        checker.diagnostics.push(Diagnostic::new(
            BadOpenMode {
                mode: s.value.clone(),
            },
            call.range(),
        ))
    }
}

#[rustfmt::skip]
fn is_open_mode_good(mode: &str) -> bool {
    let mut creating  = false;
    let mut reading   = false;
    let mut writing   = false;
    let mut appending = false;
    let mut updating  = false;
    let mut text      = false;
    let mut binary    = false;
    for c in mode.as_bytes() {
        match c {
            b'x' => { if creating  { return false } creating  = true } 
            b'r' => { if reading   { return false } reading   = true } 
            b'w' => { if writing   { return false } writing   = true } 
            b'a' => { if appending { return false } appending = true } 
            b'+' => { if updating  { return false } updating  = true } 
            b't' => { if text      { return false } text      = true } 
            b'b' => { if binary    { return false } binary    = true }
            _ => return false
        }
    }
    if (text && binary)
        || [creating, reading, writing, appending]
            .iter()
            .filter(|i| **i)
            .count()
            > 1
        || !(creating || reading || writing || appending)
    {
        return false
    }
    true
}
