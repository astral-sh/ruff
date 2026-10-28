use ruff_diagnostics::Applicability;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::flake8_use_pathlib::helpers::is_file_descriptor;
use crate::rules::flake8_use_pathlib::{
    rules::glob_call,
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
            checker.report_diagnostic_if_enabled(OsPathSplitext, range);
        }
        // PTH124
        ["py", "path", "local"] => {
            checker.report_diagnostic_if_enabled(PyPath, range);
        }
        // PTH207
        ["glob", function @ ("glob" | "iglob")] => {
            glob_call(checker, call, function);
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
