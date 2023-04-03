use anyhow::Result;
use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::autofix::actions::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct ReplaceStdoutStderr;

impl AlwaysAutofixableViolation for ReplaceStdoutStderr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Sending stdout and stderr to pipe is deprecated, use `capture_output`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `capture_output` keyword argument".to_string()
    }
}

/// Generate a [`Edit`] for a `stdout` and `stderr` [`Keyword`] pair.
fn generate_fix(
    locator: &Locator,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    stdout: &Keyword,
    stderr: &Keyword,
) -> Result<Fix> {
    let (first, second) = if stdout.location < stderr.location {
        (stdout, stderr)
    } else {
        (stderr, stdout)
    };
    Ok(Fix::new(vec![
        Edit::replacement(
            "capture_output=True".to_string(),
            first.location,
            first.end_location.unwrap(),
        ),
        remove_argument(
            locator,
            func.location,
            second.location,
            second.end_location.unwrap(),
            args,
            keywords,
            false,
        )?,
    ]))
}

/// UP022
pub fn replace_stdout_stderr(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["subprocess", "run"]
        })
    {
        // Find `stdout` and `stderr` kwargs.
        let Some(stdout) = find_keyword(keywords, "stdout") else {
            return;
        };
        let Some(stderr) = find_keyword(keywords, "stderr") else {
            return;
        };

        // Verify that they're both set to `subprocess.PIPE`.
        if !checker
            .ctx
            .resolve_call_path(&stdout.node.value)
            .map_or(false, |call_path| {
                call_path.as_slice() == ["subprocess", "PIPE"]
            })
            || !checker
                .ctx
                .resolve_call_path(&stderr.node.value)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["subprocess", "PIPE"]
                })
        {
            return;
        }

        let mut diagnostic = Diagnostic::new(ReplaceStdoutStderr, Range::from(expr));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                generate_fix(checker.locator, func, args, keywords, stdout, stderr)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
