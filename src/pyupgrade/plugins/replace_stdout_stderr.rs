use rustpython_ast::{Expr, Keyword, KeywordData, Located};

use crate::ast::helpers::{find_keyword, match_module_member};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

#[derive(Debug)]
struct MiddleContent {
    content: Option<String>,
    is_multi_line: bool,
}

impl MiddleContent {
    fn new(content: Option<String>, is_multi_line: bool) -> Self {
        Self {
            content,
            is_multi_line,
        }
    }
}

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

fn clean_middle_args(checker: &Checker, range: &Range) -> MiddleContent {
    let mut contents = checker.locator.slice_source_code_range(&range).to_string();
    let is_multi_line = contents.contains('\n');
    let start_gap = dirty_count(contents.chars());
    if contents.len() == start_gap { return MiddleContent::new(None, false); }
    for _ in 0..start_gap {
        contents.remove(0);
    }
    let end_gap = dirty_count(contents.chars().rev());
    for _ in 0..end_gap {
        contents.pop();
    }
    MiddleContent::new(Some(contents), is_multi_line)
}

/// UP022
pub fn replace_stdout_stderr(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    if match_module_member(
        expr,
        "subprocess",
        "run",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        let mut kwarg_vec: Vec<&Located<KeywordData>> = vec![];
        for item in &["stdout", "stderr"] {
            let Some(kwarg) = find_keyword(kwargs, item) else { return; };
            let is_pipe = match_module_member(
                &kwarg.node.value,
                "subprocess",
                "PIPE",
                &checker.from_imports,
                &checker.import_aliases,
            );
            if is_pipe {
                kwarg_vec.push(kwarg);
            } else {
                return;
            }
        }
        kwarg_vec.sort_by(|a, b| a.location.cmp(&b.location));
        let replace_range = Range {
            location: kwarg_vec.first().unwrap().location,
            end_location: kwarg_vec.last().unwrap().end_location.unwrap(),
        };
        let keep_range = Range {
            location: kwarg_vec.first().unwrap().end_location.unwrap(),
            end_location: kwarg_vec.last().unwrap().location,
        };
        let middle_str = clean_middle_args(checker, &keep_range);
        println!("{:?}\n", middle_str);

        let mut check = Check::new(CheckKind::ReplaceStdoutStderr, replace_range);
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "capture_output=True".to_string(),
                replace_range.location,
                replace_range.end_location,
            ));
        }
        checker.add_check(check);
    }
}
