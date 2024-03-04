use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_ast::{Expr, ExprBooleanLiteral, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_use_pathlib::rules::{
    Glob, OsPathGetatime, OsPathGetctime, OsPathGetmtime, OsPathGetsize,
};
use crate::rules::flake8_use_pathlib::violations::{
    BuiltinOpen, Joiner, OsChmod, OsGetcwd, OsMakedirs, OsMkdir, OsPathAbspath, OsPathBasename,
    OsPathDirname, OsPathExists, OsPathExpanduser, OsPathIsabs, OsPathIsdir, OsPathIsfile,
    OsPathIslink, OsPathJoin, OsPathSamefile, OsPathSplitext, OsReadlink, OsRemove, OsRename,
    OsReplace, OsRmdir, OsStat, OsUnlink, PyPath,
};
use crate::settings::types::PythonVersion;

pub(crate) fn replaceable_by_pathlib(checker: &mut Checker, call: &ExprCall) {
    if let Some(diagnostic_kind) = checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .and_then(|qualified_name| match qualified_name.segments() {
            // PTH100
            ["os", "path", "abspath"] => Some(OsPathAbspath.into()),
            // PTH101
            ["os", "chmod"] => Some(OsChmod.into()),
            // PTH102
            ["os", "makedirs"] => Some(OsMakedirs.into()),
            // PTH103
            ["os", "mkdir"] => Some(OsMkdir.into()),
            // PTH104
            ["os", "rename"] => Some(OsRename.into()),
            // PTH105
            ["os", "replace"] => Some(OsReplace.into()),
            // PTH106
            ["os", "rmdir"] => Some(OsRmdir.into()),
            // PTH107
            ["os", "remove"] => Some(OsRemove.into()),
            // PTH108
            ["os", "unlink"] => Some(OsUnlink.into()),
            // PTH109
            ["os", "getcwd"] => Some(OsGetcwd.into()),
            ["os", "getcwdb"] => Some(OsGetcwd.into()),
            // PTH110
            ["os", "path", "exists"] => Some(OsPathExists.into()),
            // PTH111
            ["os", "path", "expanduser"] => Some(OsPathExpanduser.into()),
            // PTH112
            ["os", "path", "isdir"] => Some(OsPathIsdir.into()),
            // PTH113
            ["os", "path", "isfile"] => Some(OsPathIsfile.into()),
            // PTH114
            ["os", "path", "islink"] => Some(OsPathIslink.into()),
            // PTH116
            ["os", "stat"] => Some(OsStat.into()),
            // PTH117
            ["os", "path", "isabs"] => Some(OsPathIsabs.into()),
            // PTH118
            ["os", "path", "join"] => Some(
                OsPathJoin {
                    module: "path".to_string(),
                    joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                        Joiner::Joinpath
                    } else {
                        Joiner::Slash
                    },
                }
                .into(),
            ),
            ["os", "sep", "join"] => Some(
                OsPathJoin {
                    module: "sep".to_string(),
                    joiner: if call.arguments.args.iter().any(Expr::is_starred_expr) {
                        Joiner::Joinpath
                    } else {
                        Joiner::Slash
                    },
                }
                .into(),
            ),
            // PTH119
            ["os", "path", "basename"] => Some(OsPathBasename.into()),
            // PTH120
            ["os", "path", "dirname"] => Some(OsPathDirname.into()),
            // PTH121
            ["os", "path", "samefile"] => Some(OsPathSamefile.into()),
            // PTH122
            ["os", "path", "splitext"] => Some(OsPathSplitext.into()),
            // PTH202
            ["os", "path", "getsize"] => Some(OsPathGetsize.into()),
            // PTH203
            ["os", "path", "getatime"] => Some(OsPathGetatime.into()),
            // PTH204
            ["os", "path", "getmtime"] => Some(OsPathGetmtime.into()),
            // PTH205
            ["os", "path", "getctime"] => Some(OsPathGetctime.into()),
            // PTH123
            ["" | "builtin", "open"] => {
                // `closefd` and `openener` are not supported by pathlib, so check if they are
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
                    .find_argument("closefd", 6)
                    .is_some_and(|expr| {
                        !matches!(
                            expr,
                            Expr::BooleanLiteral(ExprBooleanLiteral { value: true, .. })
                        )
                    })
                    || call
                        .arguments
                        .find_argument("opener", 7)
                        .is_some_and(|expr| !expr.is_none_literal_expr())
                {
                    return None;
                }
                Some(BuiltinOpen.into())
            }
            // PTH124
            ["py", "path", "local"] => Some(PyPath.into()),
            // PTH207
            ["glob", "glob"] => Some(
                Glob {
                    function: "glob".to_string(),
                }
                .into(),
            ),
            ["glob", "iglob"] => Some(
                Glob {
                    function: "iglob".to_string(),
                }
                .into(),
            ),
            // PTH115
            // Python 3.9+
            ["os", "readlink"] if checker.settings.target_version >= PythonVersion::Py39 => {
                Some(OsReadlink.into())
            }
            _ => None,
        })
    {
        let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, call.func.range());

        if checker.enabled(diagnostic.kind.rule()) {
            checker.diagnostics.push(diagnostic);
        }
    }
}
