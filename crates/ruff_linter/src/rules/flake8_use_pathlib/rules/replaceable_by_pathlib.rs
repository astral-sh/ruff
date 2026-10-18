use ruff_diagnostics::Applicability;
use ruff_python_ast::{self as ast, Expr, ExprCall, Int, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_path_splitext_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    is_file_descriptor, is_keyword_only_argument_non_default,
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
            checker.report_diagnostic_if_enabled(
                OsPathJoin {
                    module: module.to_string(),
                    joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                        Joiner::Joinpath
                    } else {
                        Joiner::Slash
                    },
                },
                range,
            );
        }
        // PTH122
        ["os", "path", "splitext"] => {
            os_path_splitext(checker, call);
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

            checker.report_diagnostic_if_enabled(
                Glob {
                    function: function.to_string(),
                },
                range,
            );
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

fn os_path_splitext(checker: &Checker, call: &ExprCall) {
    let extension = splitext_extension(checker, call);
    let Some(mut diagnostic) = checker.report_diagnostic_if_enabled(
        OsPathSplitext {
            fixable: extension.is_some(),
        },
        call.func.range(),
    ) else {
        return;
    };

    let Some((subscript, path)) = extension else {
        return;
    };

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        Ok(Fix::unsafe_edits(
            Edit::range_replacement(
                format!("{binding}({}).suffix", checker.locator().slice(path)),
                subscript.range(),
            ),
            [import_edit],
        ))
    });
}

fn splitext_extension<'a>(
    checker: &'a Checker,
    call: &'a ExprCall,
) -> Option<(&'a ast::ExprSubscript, &'a Expr)> {
    if !is_fix_os_path_splitext_enabled(checker.settings()) || !call.arguments.keywords.is_empty() {
        return None;
    }

    let [path] = call.arguments.args.as_ref() else {
        return None;
    };
    if path.is_starred_expr() || path.is_bytes_literal_expr() {
        return None;
    }

    let Some(Expr::Subscript(subscript)) = checker.semantic().current_expression_parent() else {
        return None;
    };
    if subscript.value.range() != call.range() || subscript.ctx != ast::ExprContext::Load {
        return None;
    }

    // Both indices select the extension from the tuple.
    let index = match subscript.slice.as_ref() {
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            ..
        }) => operand.as_ref(),
        index => index,
    };
    if !matches!(
        index,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            ..
        })
    ) {
        return None;
    }

    Some((subscript, path))
}
