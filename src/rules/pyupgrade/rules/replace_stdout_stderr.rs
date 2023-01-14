use rustpython_ast::{Expr, Keyword};

use crate::ast::helpers::find_keyword;
use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

#[derive(Debug)]
struct MiddleContent<'a> {
    contents: &'a str,
    multi_line: bool,
}

/// Return the number of "dirty" characters.
fn dirty_count(iter: impl Iterator<Item = char>) -> usize {
    let mut the_count = 0;
    for current_char in iter {
        if current_char == ' ' || current_char == ',' || current_char == '\n' {
            the_count += 1;
        } else {
            break;
        }
    }
    the_count
}

/// Extract the `Middle` content between two arguments.
fn extract_middle(contents: &str) -> Option<MiddleContent> {
    let multi_line = contents.contains('\n');
    let start_gap = dirty_count(contents.chars());
    if contents.len() == start_gap {
        return None;
    }
    let end_gap = dirty_count(contents.chars().rev());
    Some(MiddleContent {
        contents: &contents[start_gap..contents.len() - end_gap],
        multi_line,
    })
}

/// UP022
pub fn replace_stdout_stderr(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    if checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path == ["subprocess", "run"])
    {
        // Find `stdout` and `stderr` kwargs.
        let Some(stdout) = find_keyword(kwargs, "stdout") else {
            return;
        };
        let Some(stderr) = find_keyword(kwargs, "stderr") else {
            return;
        };

        // Verify that they're both set to `subprocess.PIPE`.
        if !checker
            .resolve_call_path(&stdout.node.value)
            .map_or(false, |call_path| call_path == ["subprocess", "PIPE"])
            || !checker
                .resolve_call_path(&stderr.node.value)
                .map_or(false, |call_path| call_path == ["subprocess", "PIPE"])
        {
            return;
        }

        let mut diagnostic =
            Diagnostic::new(violations::ReplaceStdoutStderr, Range::from_located(expr));
        if checker.patch(diagnostic.kind.code()) {
            let first = if stdout.location < stderr.location {
                stdout
            } else {
                stderr
            };
            let last = if stdout.location > stderr.location {
                stdout
            } else {
                stderr
            };
            let mut contents = String::from("capture_output=True");
            if let Some(middle) =
                extract_middle(&checker.locator.slice_source_code_range(&Range::new(
                    first.end_location.unwrap(),
                    last.location,
                )))
            {
                if middle.multi_line {
                    contents.push(',');
                    contents.push('\n');
                    contents.push_str(&indentation(checker, first));
                } else {
                    contents.push(',');
                    contents.push(' ');
                }
                contents.push_str(middle.contents);
            }
            diagnostic.amend(Fix::replacement(
                contents,
                first.location,
                last.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
