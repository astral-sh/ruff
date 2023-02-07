use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::rules::flake8_use_pathlib::violations::{
    PathlibAbspath, PathlibBasename, PathlibChmod, PathlibDirname, PathlibExists,
    PathlibExpanduser, PathlibGetcwd, PathlibIsAbs, PathlibIsDir, PathlibIsFile, PathlibIsLink,
    PathlibJoin, PathlibMakedirs, PathlibMkdir, PathlibOpen, PathlibPyPath, PathlibReadlink,
    PathlibRemove, PathlibRename, PathlibReplace, PathlibRmdir, PathlibSamefile, PathlibSplitext,
    PathlibStat, PathlibUnlink,
};

pub fn replaceable_by_pathlib(checker: &mut Checker, expr: &Expr) {
    if let Some(diagnostic_kind) =
        checker
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["os", "path", "abspath"] => Some(PathlibAbspath.into()),
                ["os", "chmod"] => Some(PathlibChmod.into()),
                ["os", "mkdir"] => Some(PathlibMkdir.into()),
                ["os", "makedirs"] => Some(PathlibMakedirs.into()),
                ["os", "rename"] => Some(PathlibRename.into()),
                ["os", "replace"] => Some(PathlibReplace.into()),
                ["os", "rmdir"] => Some(PathlibRmdir.into()),
                ["os", "remove"] => Some(PathlibRemove.into()),
                ["os", "unlink"] => Some(PathlibUnlink.into()),
                ["os", "getcwd"] => Some(PathlibGetcwd.into()),
                ["os", "getcwdb"] => Some(PathlibGetcwd.into()),
                ["os", "path", "exists"] => Some(PathlibExists.into()),
                ["os", "path", "expanduser"] => Some(PathlibExpanduser.into()),
                ["os", "path", "isdir"] => Some(PathlibIsDir.into()),
                ["os", "path", "isfile"] => Some(PathlibIsFile.into()),
                ["os", "path", "islink"] => Some(PathlibIsLink.into()),
                ["os", "readlink"] => Some(PathlibReadlink.into()),
                ["os", "stat"] => Some(PathlibStat.into()),
                ["os", "path", "isabs"] => Some(PathlibIsAbs.into()),
                ["os", "path", "join"] => Some(PathlibJoin.into()),
                ["os", "path", "basename"] => Some(PathlibBasename.into()),
                ["os", "path", "dirname"] => Some(PathlibDirname.into()),
                ["os", "path", "samefile"] => Some(PathlibSamefile.into()),
                ["os", "path", "splitext"] => Some(PathlibSplitext.into()),
                ["", "open"] => Some(PathlibOpen.into()),
                ["py", "path", "local"] => Some(PathlibPyPath.into()),
                _ => None,
            })
    {
        let diagnostic =
            Diagnostic::new::<DiagnosticKind>(diagnostic_kind, Range::from_located(expr));

        if checker.settings.rules.enabled(diagnostic.kind.rule()) {
            checker.diagnostics.push(diagnostic);
        }
    }
}
