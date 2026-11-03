use ruff_diagnostics::Applicability;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::{is_fix_glob_enabled, is_fix_os_path_join_enabled};
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_file_descriptor, is_keyword_only_argument_non_default,
    is_pathlib_path_call,
};
use crate::rules::flake8_use_pathlib::{
    rules::Glob,
    violations::{Joiner, OsListdir, OsPathJoin, OsPathSplitext, PyPath},
};
use crate::{Edit, Fix};

pub(crate) fn replaceable_by_pathlib(checker: &Checker, call: &ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    let range = call.func.range();
    match qualified_name.segments() {
        // PTH118
        ["os", module @ ("path" | "sep"), "join"] => {
            let joiner = if call.arguments.args.iter().any(Expr::is_starred_expr) {
                Joiner::Joinpath
            } else {
                Joiner::Slash
            };

            if let Some(mut diagnostic) = checker.report_diagnostic_if_enabled(
                OsPathJoin {
                    module: module.to_string(),
                    joiner,
                },
                range,
            ) {
                if is_fix_os_path_join_enabled(checker.settings()) {
                    if call.arguments.keywords.is_empty() {
                        let can_fix = match *module {
                            "path" => !call.arguments.args.is_empty(),
                            "sep" => match joiner {
                                Joiner::Joinpath => !call.arguments.args.is_empty(),
                                Joiner::Slash => {
                                    call.arguments.args.len() == 1
                                        && matches!(
                                            &call.arguments.args[0],
                                            Expr::List(_) | Expr::Tuple(_)
                                        )
                                }
                            },
                            _ => false,
                        };

                        if can_fix {
                            diagnostic.try_set_fix(|| {
                                let (import_edit, binding) =
                                    checker.importer().get_or_import_symbol(
                                        &ImportRequest::import("pathlib", "Path"),
                                        call.start(),
                                        checker.semantic(),
                                    )?;

                                let format_first_expr = |first: &Expr| {
                                    let first_code = checker.locator().slice(first.range());
                                    if is_pathlib_path_call(checker, first) {
                                        first_code.to_string()
                                    } else {
                                        format!("{binding}({first_code})")
                                    }
                                };

                                let format_operand = |arg: &Expr| {
                                    let arg_code = checker.locator().slice(arg.range());
                                    if matches!(
                                        arg,
                                        Expr::BinOp(_)
                                            | Expr::BoolOp(_)
                                            | Expr::Named(_)
                                            | Expr::If(_)
                                            | Expr::Lambda(_)
                                            | Expr::Yield(_)
                                            | Expr::YieldFrom(_)
                                    ) {
                                        format!("({arg_code})")
                                    } else {
                                        arg_code.to_string()
                                    }
                                };

                                let replacement = match *module {
                                    "path" => match joiner {
                                        Joiner::Joinpath => {
                                            let first = &call.arguments.args[0];
                                            if first.is_starred_expr() {
                                                let args_code =
                                                    checker.locator().slice(call.arguments.range());
                                                format!("{binding}().joinpath{args_code}")
                                            } else {
                                                let first_str = format_first_expr(first);
                                                let rest_args: Vec<_> = call.arguments.args[1..]
                                                    .iter()
                                                    .map(|arg| {
                                                        checker
                                                            .locator()
                                                            .slice(arg.range())
                                                            .to_string()
                                                    })
                                                    .collect();
                                                format!(
                                                    "{first_str}.joinpath({})",
                                                    rest_args.join(", ")
                                                )
                                            }
                                        }
                                        Joiner::Slash => {
                                            let first = &call.arguments.args[0];
                                            let first_str = format_first_expr(first);
                                            if call.arguments.args.len() == 1 {
                                                first_str
                                            } else {
                                                let rest: Vec<_> = call.arguments.args[1..]
                                                    .iter()
                                                    .map(format_operand)
                                                    .collect();
                                                format!("{first_str} / {}", rest.join(" / "))
                                            }
                                        }
                                    },
                                    "sep" => match joiner {
                                        Joiner::Joinpath => {
                                            let first = &call.arguments.args[0];
                                            let first_str = format_first_expr(first);
                                            let rest_args: Vec<_> = call.arguments.args[1..]
                                                .iter()
                                                .map(|arg| {
                                                    checker.locator().slice(arg.range()).to_string()
                                                })
                                                .collect();
                                            format!(
                                                "{first_str}.joinpath({})",
                                                rest_args.join(", ")
                                            )
                                        }
                                        Joiner::Slash => {
                                            let arg = &call.arguments.args[0];
                                            let elements: &[Expr] = match arg {
                                                Expr::List(list) => &list.elts,
                                                Expr::Tuple(tuple) => &tuple.elts,
                                                _ => &[],
                                            };
                                            if elements.is_empty() {
                                                format!("{binding}()")
                                            } else if elements.len() == 1 {
                                                format_first_expr(&elements[0])
                                            } else {
                                                let first_str = format_first_expr(&elements[0]);
                                                let rest: Vec<_> = elements[1..]
                                                    .iter()
                                                    .map(format_operand)
                                                    .collect();
                                                format!("{first_str} / {}", rest.join(" / "))
                                            }
                                        }
                                    },
                                    _ => unreachable!(),
                                };

                                Ok(Fix::applicable_edits(
                                    Edit::range_replacement(replacement, call.range()),
                                    [import_edit],
                                    Applicability::Unsafe,
                                ))
                            });
                        }
                    }
                }
            }
        }
        // PTH122
        ["os", "path", "splitext"] => {
            checker.report_diagnostic_if_enabled(OsPathSplitext, range);
        }
        // PTH124
        ["py", "path", "local"] => {
            checker.report_diagnostic_if_enabled(PyPath, range);
        }
        // PTH207
        ["glob", function @ ("glob" | "iglob")] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/glob.html#glob.glob)
            // ```text
            //               0           1              2            3                 4
            // glob.glob(pathname, *, root_dir=None, dir_fd=None, recursive=False, include_hidden=False)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }

            if let Some(mut diagnostic) = checker.report_diagnostic_if_enabled(
                Glob {
                    function: function.to_string(),
                },
                range,
            ) {
                if is_fix_glob_enabled(checker.settings()) {
                    if let Some(pathname) = call.arguments.find_argument_value("pathname", 0) {
                        if !has_unknown_keywords_or_starred_expr(
                            &call.arguments,
                            &["pathname", "root_dir"],
                        ) {
                            diagnostic.try_set_fix(|| {
                                let (import_edit, binding) =
                                    checker.importer().get_or_import_symbol(
                                        &ImportRequest::import("pathlib", "Path"),
                                        call.start(),
                                        checker.semantic(),
                                    )?;

                                let pathname_code = checker.locator().slice(pathname.range());
                                let root_dir = call.arguments.find_argument_value("root_dir", 1);

                                let replacement = if let Some(root_dir) = root_dir {
                                    let root_code = checker.locator().slice(root_dir.range());
                                    format!("{binding}({root_code}).glob({pathname_code})")
                                } else {
                                    format!("{binding}().glob({pathname_code})")
                                };

                                let applicability =
                                    if checker.comment_ranges().intersects(call.range()) {
                                        Applicability::Unsafe
                                    } else {
                                        Applicability::DisplayOnly
                                    };

                                Ok(Fix::applicable_edits(
                                    Edit::range_replacement(replacement, call.range()),
                                    [import_edit],
                                    applicability,
                                ))
                            });
                        }
                    }
                }
            }
        }
        // PTH208
        ["os", "listdir"] => {
            let path = call.arguments.find_argument_value("path", 0);
            if path.is_some_and(|expr| is_file_descriptor(expr, checker.semantic())) {
                return;
            }

            if let Some(mut diagnostic) = checker.report_diagnostic_if_enabled(OsListdir, range) {
                if let Some(path) = path {
                    diagnostic.try_set_fix(|| {
                        let (import_edit, binding) = checker.importer().get_or_import_symbol(
                            &ImportRequest::import("pathlib", "Path"),
                            call.start(),
                            checker.semantic(),
                        )?;

                        Ok(Fix::applicable_edits(
                            Edit::range_replacement(
                                format!("{binding}({}).iterdir()", checker.locator().slice(path)),
                                call.range(),
                            ),
                            [import_edit],
                            Applicability::DisplayOnly,
                        ))
                    });
                }
            }
        }

        _ => {}
    }
}
