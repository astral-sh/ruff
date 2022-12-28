use rustpython_ast::{Expr, Keyword, KeywordData, Located};

use crate::ast::helpers::{find_keyword, match_module_member};
use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
//
// #[derive(Debug)]
// struct MiddleContent {
//     content: Option<String>,
//     is_multi_line: bool,
// }
//
// impl MiddleContent {
//     fn new(content: Option<String>, is_multi_line: bool) -> Self {
//         Self {
//             content,
//             is_multi_line,
//         }
//     }
// }
//
// fn dirty_count(iter: impl Iterator<Item = char>) -> usize {
//     let mut the_count = 0;
//     for current_char in iter {
//         if current_char == ' ' || current_char == ',' || current_char == '\n' {
//             the_count += 1;
//         } else {
//             break;
//         }
//     }
//     the_count
// }
//
// fn clean_middle_args(checker: &Checker, range: &Range) -> Option<MiddleContent> {
//     let mut contents = checker.locator.slice_source_code_range(range);
//     let is_multi_line = contents.contains('\n');
//     let start_gap = dirty_count(contents.chars());
//     if contents.len() == start_gap {
//         return None;
//     }
//     for _ in 0..start_gap {
//         contents.remove(0);
//     }
//     let end_gap = dirty_count(contents.chars().rev());
//     for _ in 0..end_gap {
//         contents.pop();
//     }
//     MiddleContent::new(contents, is_multi_line)
// }

/// UP022
pub fn replace_stdout_stderr(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    if match_module_member(
        expr,
        "subprocess",
        "run",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        // Find `stdout` and `stderr` kwargs.
        let Some(stdout) = find_keyword(kwargs, "stdout") else {
            return;
        };
        let Some(stderr) = find_keyword(kwargs, "stderr") else {
            return;
        };

        // Verify that they're both set to `subprocess.PIPE`.
        if !match_module_member(
            &stdout.node.value,
            "subprocess",
            "PIPE",
            &checker.from_imports,
            &checker.import_aliases,
        ) || !match_module_member(
            &stderr.node.value,
            "subprocess",
            "PIPE",
            &checker.from_imports,
            &checker.import_aliases,
        ) {
            return;
        }

        let mut check = Check::new(CheckKind::ReplaceStdoutStderr, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
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
            let replace_range = Range {
                location: first.location,
                end_location: last.end_location.unwrap(),
            };
            let keep_range = Range {
                location: first.end_location.unwrap(),
                end_location: last.location,
            };

            let mut replace_str = String::from("capture_output=True");
            replace_str.push_str(&checker.locator.slice_source_code_range(&keep_range));

            check.amend(Fix::replacement(
                replace_str,
                replace_range.location,
                replace_range.end_location,
            ));
        }
        checker.add_check(check);
    }
}
