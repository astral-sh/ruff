use rustpython_ast::{Expr, Keyword, KeywordData, Located};

use crate::ast::helpers::{find_keyword, match_module_member};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

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
            println!("{:?}", kwarg);
            if is_pipe {
                kwarg_vec.push(kwarg);
            } else {
                return;
            }
        }
        let range1 = Range::from_located(kwarg_vec.get(0).unwrap());
        let range2 = Range::from_located(kwarg_vec.get(1).unwrap());
        let mut check1 = Check::new(CheckKind::ReplaceStdoutStderr, range1);
        let mut check2 = Check::new(CheckKind::ReplaceStdoutStderr, range2);
        let stdout = kwarg_vec.get(0).unwrap();
        let stderr = kwarg_vec.get(1).unwrap();
        if checker.patch(check1.kind.code()) {
            check1.amend(Fix::replacement(
                "capture_output=True".to_string(),
                stdout.location,
                stdout.end_location.unwrap(),
            ));
            check2.amend(Fix::deletion(stderr.location, stderr.end_location.unwrap()));
        }
        checker.add_check(check1);
        checker.add_check(check2);
    }
}
