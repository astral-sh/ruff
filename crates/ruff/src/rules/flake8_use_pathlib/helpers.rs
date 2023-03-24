use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_use_pathlib::violations::{
    BuiltinOpen, OsChmod, OsGetcwd, OsMakedirs, OsMkdir, OsPathAbspath, OsPathBasename,
    OsPathDirname, OsPathExists, OsPathExpanduser, OsPathIsabs, OsPathIsdir, OsPathIsfile,
    OsPathIslink, OsPathJoin, OsPathSamefile, OsPathSplitext, OsReadlink, OsRemove, OsRename,
    OsRmdir, OsStat, OsUnlink, PathlibReplace, PyPath,
};
use crate::settings::types::PythonVersion;

pub fn replaceable_by_pathlib(checker: &mut Checker, expr: &Expr) {
    if let Some(diagnostic_kind) =
        checker
            .ctx
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["os", "path", "abspath"] => Some(OsPathAbspath.into()),
                ["os", "chmod"] => Some(OsChmod.into()),
                ["os", "mkdir"] => Some(OsMkdir.into()),
                ["os", "makedirs"] => Some(OsMakedirs.into()),
                ["os", "rename"] => Some(OsRename.into()),
                ["os", "replace"] => Some(PathlibReplace.into()),
                ["os", "rmdir"] => Some(OsRmdir.into()),
                ["os", "remove"] => Some(OsRemove.into()),
                ["os", "unlink"] => Some(OsUnlink.into()),
                ["os", "getcwd"] => Some(OsGetcwd.into()),
                ["os", "getcwdb"] => Some(OsGetcwd.into()),
                ["os", "path", "exists"] => Some(OsPathExists.into()),
                ["os", "path", "expanduser"] => Some(OsPathExpanduser.into()),
                ["os", "path", "isdir"] => Some(OsPathIsdir.into()),
                ["os", "path", "isfile"] => Some(OsPathIsfile.into()),
                ["os", "path", "islink"] => Some(OsPathIslink.into()),
                ["os", "stat"] => Some(OsStat.into()),
                ["os", "path", "isabs"] => Some(OsPathIsabs.into()),
                ["os", "path", "join"] => Some(OsPathJoin.into()),
                ["os", "path", "basename"] => Some(OsPathBasename.into()),
                ["os", "path", "dirname"] => Some(OsPathDirname.into()),
                ["os", "path", "samefile"] => Some(OsPathSamefile.into()),
                ["os", "path", "splitext"] => Some(OsPathSplitext.into()),
                ["", "open"] => Some(BuiltinOpen.into()),
                ["py", "path", "local"] => Some(PyPath.into()),
                // Python 3.9+
                ["os", "readlink"] if checker.settings.target_version >= PythonVersion::Py39 => {
                    Some(OsReadlink.into())
                }
                _ => None,
            })
    {
        let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, Range::from(expr));

        if checker.settings.rules.enabled(diagnostic.kind.rule()) {
            checker.diagnostics.push(diagnostic);
        }
    }
}
