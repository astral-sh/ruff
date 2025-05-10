use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_ast::{self as ast, Expr, ExprBooleanLiteral, ExprCall};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_use_pathlib::rules::{
    Glob, OsPathGetatime, OsPathGetctime, OsPathGetmtime, OsPathGetsize,
};
use crate::rules::flake8_use_pathlib::violations::{
    BuiltinOpen, Joiner, OsChmod, OsGetcwd, OsListdir, OsMakedirs, OsMkdir, OsPathAbspath,
    OsPathBasename, OsPathDirname, OsPathExists, OsPathExpanduser, OsPathIsabs, OsPathIsdir,
    OsPathIsfile, OsPathIslink, OsPathJoin, OsPathSamefile, OsPathSplitext, OsReadlink, OsRemove,
    OsRename, OsReplace, OsRmdir, OsStat, OsUnlink, PyPath,
};
use ruff_python_ast::PythonVersion;

pub(crate) fn replaceable_by_pathlib(checker: &Checker, call: &ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    let diagnostic_kind: DiagnosticKind = match qualified_name.segments() {
        // PTH100
        ["os", "path", "abspath"] => OsPathAbspath.into(),
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
                || is_argument_non_default(&call.arguments, "dir_fd", 2)
            {
                return;
            }
            OsChmod.into()
        }
        // PTH102
        ["os", "makedirs"] => OsMakedirs.into(),
        // PTH103
        ["os", "mkdir"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.mkdir)
            // ```text
            //           0     1                2
            // os.mkdir(path, mode=0o777, *, dir_fd=None)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 2) {
                return;
            }
            OsMkdir.into()
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
            if is_argument_non_default(&call.arguments, "src_dir_fd", 2)
                || is_argument_non_default(&call.arguments, "dst_dir_fd", 3)
            {
                return;
            }
            OsRename.into()
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
            if is_argument_non_default(&call.arguments, "src_dir_fd", 2)
                || is_argument_non_default(&call.arguments, "dst_dir_fd", 3)
            {
                return;
            }
            OsReplace.into()
        }
        // PTH106
        ["os", "rmdir"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.rmdir)
            // ```text
            //            0         1
            // os.rmdir(path, *, dir_fd=None)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 1) {
                return;
            }
            OsRmdir.into()
        }
        // PTH107
        ["os", "remove"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.remove)
            // ```text
            //            0         1
            // os.remove(path, *, dir_fd=None)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 1) {
                return;
            }
            OsRemove.into()
        }
        // PTH108
        ["os", "unlink"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.unlink)
            // ```text
            //            0         1
            // os.unlink(path, *, dir_fd=None)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 1) {
                return;
            }
            OsUnlink.into()
        }
        // PTH109
        ["os", "getcwd"] => OsGetcwd.into(),
        ["os", "getcwdb"] => OsGetcwd.into(),
        // PTH110
        ["os", "path", "exists"] => OsPathExists.into(),
        // PTH111
        ["os", "path", "expanduser"] => OsPathExpanduser.into(),
        // PTH112
        ["os", "path", "isdir"] => OsPathIsdir.into(),
        // PTH113
        ["os", "path", "isfile"] => OsPathIsfile.into(),
        // PTH114
        ["os", "path", "islink"] => OsPathIslink.into(),
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
                || is_argument_non_default(&call.arguments, "dir_fd", 1)
            {
                return;
            }
            OsStat.into()
        }
        // PTH117
        ["os", "path", "isabs"] => OsPathIsabs.into(),
        // PTH118
        ["os", "path", "join"] => OsPathJoin {
            module: "path".to_string(),
            joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                Joiner::Joinpath
            } else {
                Joiner::Slash
            },
        }
        .into(),
        ["os", "sep", "join"] => OsPathJoin {
            module: "sep".to_string(),
            joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                Joiner::Joinpath
            } else {
                Joiner::Slash
            },
        }
        .into(),
        // PTH119
        ["os", "path", "basename"] => OsPathBasename.into(),
        // PTH120
        ["os", "path", "dirname"] => OsPathDirname.into(),
        // PTH121
        ["os", "path", "samefile"] => OsPathSamefile.into(),
        // PTH122
        ["os", "path", "splitext"] => OsPathSplitext.into(),
        // PTH202
        ["os", "path", "getsize"] => OsPathGetsize.into(),
        // PTH203
        ["os", "path", "getatime"] => OsPathGetatime.into(),
        // PTH204
        ["os", "path", "getmtime"] => OsPathGetmtime.into(),
        // PTH205
        ["os", "path", "getctime"] => OsPathGetctime.into(),
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
            BuiltinOpen.into()
        }
        // PTH124
        ["py", "path", "local"] => PyPath.into(),
        // PTH207
        ["glob", "glob"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/glob.html#glob.glob)
            // ```text
            //               0           1              2            3                 4
            // glob.glob(pathname, *, root_dir=None, dir_fd=None, recursive=False, include_hidden=False)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 2) {
                return;
            }

            Glob {
                function: "glob".to_string(),
            }
            .into()
        }

        ["glob", "iglob"] => {
            // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
            // Signature as of Python 3.13 (https://docs.python.org/3/library/glob.html#glob.iglob)
            // ```text
            //                0           1              2            3                 4
            // glob.iglob(pathname, *, root_dir=None, dir_fd=None, recursive=False, include_hidden=False)
            // ```
            if is_argument_non_default(&call.arguments, "dir_fd", 2) {
                return;
            }

            Glob {
                function: "iglob".to_string(),
            }
            .into()
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
            if is_argument_non_default(&call.arguments, "dir_fd", 1) {
                return;
            }
            OsReadlink.into()
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
            OsListdir.into()
        }
        _ => return,
    };

    if checker.enabled(diagnostic_kind.rule()) {
        checker.report_diagnostic(Diagnostic::new(diagnostic_kind, call.func.range()));
    }
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
