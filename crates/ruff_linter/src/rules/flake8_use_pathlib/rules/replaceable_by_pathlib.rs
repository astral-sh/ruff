use ruff_python_ast::{self as ast, Expr, ExprBooleanLiteral, ExprCall};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_use_pathlib::rules::{
    Glob, OsPathGetatime, OsPathGetctime, OsPathGetmtime,
};
use crate::rules::flake8_use_pathlib::violations::{
    BuiltinOpen, Joiner, OsChmod, OsGetcwd, OsListdir, OsMakedirs, OsMkdir, OsPathAbspath,
    OsPathBasename, OsPathDirname, OsPathExists, OsPathExpanduser, OsPathIsabs, OsPathIsdir,
    OsPathIsfile, OsPathIslink, OsPathJoin, OsPathSamefile, OsPathSplitext, OsReadlink, OsRemove,
    OsRename, OsReplace, OsRmdir, OsStat, OsSymlink, OsUnlink, PyPath,
};
use ruff_python_ast::PythonVersion;

pub(crate) fn replaceable_by_pathlib(checker: &Checker, call: &ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    let range = call.func.range();
    match qualified_name.segments() {
        // PTH100
        ["os", "path", "abspath"] => checker.report_diagnostic_if_enabled(OsPathAbspath, range),
        // PTH101
        ["os", "chmod"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.chmod)
            // ```text
            //           0     1          2               3
            // os.chmod(path, mode, *, dir_fd=None, follow_symlinks=True)
            // ```
            if call
                .arguments
                .find_argument_value("path", 0)
                .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
                || is_keyword_only_argument_non_default(&call.arguments, "dir_fd")
            {
                return;
            }
            checker.report_diagnostic_if_enabled(OsChmod, range)
        }
        // PTH102
        ["os", "makedirs"] => checker.report_diagnostic_if_enabled(OsMakedirs, range),
        // PTH103
        ["os", "mkdir"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.mkdir)
            // ```text
            //           0     1                2
            // os.mkdir(path, mode=0o777, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsMkdir, range)
        }
        // PTH104
        ["os", "rename"] => {
            // `src_dir_fd` and `dst_dir_fd` are not supported by pathlib, so check if they are
            // set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.rename)
            // ```text
            //           0    1       2                3
            // os.rename(src, dst, *, src_dir_fd=None, dst_dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "src_dir_fd")
                || is_keyword_only_argument_non_default(&call.arguments, "dst_dir_fd")
            {
                return;
            }
            checker.report_diagnostic_if_enabled(OsRename, range)
        }
        // PTH105
        ["os", "replace"] => {
            // `src_dir_fd` and `dst_dir_fd` are not supported by pathlib, so check if they are
            // set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.replace)
            // ```text
            //              0    1       2                3
            // os.replace(src, dst, *, src_dir_fd=None, dst_dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "src_dir_fd")
                || is_keyword_only_argument_non_default(&call.arguments, "dst_dir_fd")
            {
                return;
            }
            checker.report_diagnostic_if_enabled(OsReplace, range)
        }
        // PTH106
        ["os", "rmdir"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.rmdir)
            // ```text
            //            0         1
            // os.rmdir(path, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsRmdir, range)
        }
        // PTH107
        ["os", "remove"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.remove)
            // ```text
            //            0         1
            // os.remove(path, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsRemove, range)
        }
        // PTH108
        ["os", "unlink"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.unlink)
            // ```text
            //            0         1
            // os.unlink(path, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsUnlink, range)
        }
        // PTH109
        ["os", "getcwd"] => checker.report_diagnostic_if_enabled(OsGetcwd, range),
        ["os", "getcwdb"] => checker.report_diagnostic_if_enabled(OsGetcwd, range),
        // PTH110
        ["os", "path", "exists"] => checker.report_diagnostic_if_enabled(OsPathExists, range),
        // PTH111
        ["os", "path", "expanduser"] => {
            checker.report_diagnostic_if_enabled(OsPathExpanduser, range)
        }
        // PTH112
        ["os", "path", "isdir"] => checker.report_diagnostic_if_enabled(OsPathIsdir, range),
        // PTH113
        ["os", "path", "isfile"] => checker.report_diagnostic_if_enabled(OsPathIsfile, range),
        // PTH114
        ["os", "path", "islink"] => checker.report_diagnostic_if_enabled(OsPathIslink, range),
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
        // PTH117
        ["os", "path", "isabs"] => checker.report_diagnostic_if_enabled(OsPathIsabs, range),
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
        // PTH119
        ["os", "path", "basename"] => checker.report_diagnostic_if_enabled(OsPathBasename, range),
        // PTH120
        ["os", "path", "dirname"] => checker.report_diagnostic_if_enabled(OsPathDirname, range),
        // PTH121
        ["os", "path", "samefile"] => checker.report_diagnostic_if_enabled(OsPathSamefile, range),
        // PTH122
        ["os", "path", "splitext"] => checker.report_diagnostic_if_enabled(OsPathSplitext, range),
        // PTH203
        ["os", "path", "getatime"] => checker.report_diagnostic_if_enabled(OsPathGetatime, range),
        // PTH204
        ["os", "path", "getmtime"] => checker.report_diagnostic_if_enabled(OsPathGetmtime, range),
        // PTH205
        ["os", "path", "getctime"] => checker.report_diagnostic_if_enabled(OsPathGetctime, range),
        // PTH211
        ["os", "symlink"] => {
            // `dir_fd` is not supported by pathlib, so check if there are non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.symlink)
            // ```text
            //            0    1    2                             3
            // os.symlink(src, dst, target_is_directory=False, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsSymlink, range)
        }

        // PTH123
        ["" | "builtins", "open"] => {
            // `closefd` and `opener` are not supported by pathlib, so check if they are
            // are set to non-default values.
            // https://github.com/astral-sh/ruff/issues/7620
            // Signature as of Python 3.11 (https://docs.python.org/3/library/functions.html#open):
            // ```text
            //      0     1         2             3              4            5
            // open(file, mode='r', buffering=-1, encoding=None, errors=None, newline=None,
            //      6             7
            //      closefd=True, opener=None)
            //              ^^^^         ^^^^
            // ```
            // For `pathlib` (https://docs.python.org/3/library/pathlib.html#pathlib.Path.open):
            // ```text
            // Path.open(mode='r', buffering=-1, encoding=None, errors=None, newline=None)
            // ```
            if call
                .arguments
                .find_argument_value("closefd", 6)
                .is_some_and(|expr| {
                    !matches!(
                        expr,
                        Expr::BooleanLiteral(ExprBooleanLiteral { value: true, .. })
                    )
                })
                || is_argument_non_default(&call.arguments, "opener", 7)
                || call
                    .arguments
                    .find_argument_value("file", 0)
                    .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
            {
                return;
            }
            checker.report_diagnostic_if_enabled(BuiltinOpen, range)
        }
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
        // PTH115
        // Python 3.9+
        ["os", "readlink"] if checker.target_version() >= PythonVersion::PY39 => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.readlink)
            // ```text
            //               0         1
            // os.readlink(path, *, dir_fd=None)
            // ```
            if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
                return;
            }
            checker.report_diagnostic_if_enabled(OsReadlink, range)
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

/// Returns `true` if the given expression looks like a file descriptor, i.e., if it is an integer.
fn is_file_descriptor(expr: &Expr, semantic: &SemanticModel) -> bool {
    if matches!(
        expr,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(_),
            ..
        })
    ) {
        return true;
    }

    let Some(name) = get_name_expr(expr) else {
        return false;
    };

    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_int(binding, semantic)
}

fn get_name_expr(expr: &Expr) -> Option<&ast::ExprName> {
    match expr {
        Expr::Name(name) => Some(name),
        Expr::Call(ast::ExprCall { func, .. }) => get_name_expr(func),
        _ => None,
    }
}

/// Returns `true` if argument `name` is set to a non-default `None` value.
fn is_argument_non_default(arguments: &ast::Arguments, name: &str, position: usize) -> bool {
    arguments
        .find_argument_value(name, position)
        .is_some_and(|expr| !expr.is_none_literal_expr())
}

fn is_keyword_only_argument_non_default(arguments: &ast::Arguments, name: &str) -> bool {
    arguments
        .find_keyword(name)
        .is_some_and(|keyword| !keyword.value.is_none_literal_expr())
}
