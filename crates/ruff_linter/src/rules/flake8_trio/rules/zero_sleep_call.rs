use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_python_ast::{self as ast, Expr, ExprCall, Int};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of `trio.sleep(0)`.
///
/// ## Why is this bad?
/// `trio.sleep(0)` is equivalent to calling `trio.lowlevel.checkpoint()`.
/// However, the latter better conveys the intent of the code.
///
/// ## Example
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.sleep(0)
/// ```
///
/// Use instead:
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.lowlevel.checkpoint()
/// ```
#[violation]
pub struct TrioZeroSleepCall;

impl AlwaysFixableViolation for TrioZeroSleepCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `trio.lowlevel.checkpoint()` instead of `trio.sleep(0)`")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `trio.lowlevel.checkpoint()`")
    }
}

/// TRIO115
pub(crate) fn zero_sleep_call(checker: &mut Checker, call: &ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(call.func.as_ref())
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["trio", "sleep"]))
    {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument("seconds", 0) else {
        return;
    };

    match arg {
        Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
            let Some(int) = value.as_int() else { return };
            if *int != Int::ZERO {
                return;
            }
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            let scope = checker.semantic().current_scope();
            if let Some(binding_id) = scope.get(id) {
                let binding = checker.semantic().binding(binding_id);
                if binding.kind.is_assignment() || binding.kind.is_named_expr_assignment() {
                    if let Some(parent_id) = binding.source {
                        let parent = checker.semantic().statement(parent_id);
                        if let Stmt::Assign(ast::StmtAssign { value, .. })
                        | Stmt::AnnAssign(ast::StmtAnnAssign {
                            value: Some(value), ..
                        })
                        | Stmt::AugAssign(ast::StmtAugAssign { value, .. }) = parent
                        {
                            let Expr::NumberLiteral(ast::ExprNumberLiteral { value: num, .. }) =
                                value.as_ref()
                            else {
                                return;
                            };
                            let Some(int) = num.as_int() else { return };
                            if *int != Int::ZERO {
                                return;
                            }
                        }
                    }
                }
            }
        }
        _ => return,
    }

    let mut diagnostic = Diagnostic::new(TrioZeroSleepCall, call.range());
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("trio", "lowlevel"),
            call.func.start(),
            checker.semantic(),
        )?;
        let reference_edit =
            Edit::range_replacement(format!("{binding}.checkpoint"), call.func.range());
        let arg_edit = Edit::range_replacement("()".to_string(), call.arguments.range());
        Ok(Fix::safe_edits(import_edit, [reference_edit, arg_edit]))
    });
    checker.diagnostics.push(diagnostic);
}
