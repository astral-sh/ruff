use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_use_pathlib::helpers::{
    is_file_descriptor, is_keyword_only_argument_non_default,
};
use crate::rules::flake8_use_pathlib::{
    rules::Glob,
    violations::{Joiner, OsListdir, OsPathJoin, OsPathSplitext, OsStat, PyPath},
};

pub(crate) fn replaceable_by_pathlib(checker: &Checker, call: &ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    let range = call.func.range();
    match qualified_name.segments() {
        // PTH116
        ["os", "stat"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.stat)
            // ```text
            //           0         1           2
            // os.stat(path, *, dir_fd=None, follow_symlinks=True)
            // ```
            if call
                .arguments
                .find_argument_value("path", 0)
                .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
                || is_keyword_only_argument_non_default(&call.arguments, "dir_fd")
            {
                return;
            }
            checker.report_diagnostic_if_enabled(OsStat, range)
        }
        // PTH118
        ["os", "path", "join"] => checker.report_diagnostic_if_enabled(
            OsPathJoin {
                module: "path".to_string(),
                joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                    Joiner::Joinpath
                } else {
                    Joiner::Slash
                },
            },
            range,
        ),
        ["os", "sep", "join"] => checker.report_diagnostic_if_enabled(
            OsPathJoin {
                module: "sep".to_string(),
                joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                    Joiner::Joinpath
                } else {
                    Joiner::Slash
                },
            },
            range,
        ),
        // PTH122
        ["os", "path", "splitext"] => checker.report_diagnostic_if_enabled(OsPathSplitext, range),
        // PTH124
        ["py", "path", "local"] => checker.report_diagnostic_if_enabled(PyPath, range),
        // PTH207
        ["glob", "glob"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/glob.html#glob.glob)
            // ```text
            //               0           1              2            3                 4
            // glob.glob(pathname, *, root_dir=None, dir_fd=None, recursive=False, include_hidden=False)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }

            checker.report_diagnostic_if_enabled(
                Glob {
                    function: "glob".to_string(),
                },
                range,
            )
        }

        ["glob", "iglob"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/glob.html#glob.iglob)
            // ```text
            //                0           1              2            3                 4
            // glob.iglob(pathname, *, root_dir=None, dir_fd=None, recursive=False, include_hidden=False)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }

            checker.report_diagnostic_if_enabled(
                Glob {
                    function: "iglob".to_string(),
                },
                range,
            )
        }
        // PTH208
        ["os", "listdir"] => {
            if call
                .arguments
                .find_argument_value("path", 0)
                .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
            {
                return;
            }
            checker.report_diagnostic_if_enabled(OsListdir, range)
        }

        _ => return,
    };
}
