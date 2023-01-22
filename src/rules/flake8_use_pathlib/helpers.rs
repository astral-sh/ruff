use rustpython_ast::Expr;

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

enum OsCall {
    Abspath,
    Chmod,
    Mkdir,
    Makedirs,
    Rename,
    Replace,
    Rmdir,
    Remove,
    Unlink,
    Getcwd,
    Exists,
    Expanduser,
    IsDir,
    IsFile,
    IsLink,
    Readlink,
    Stat,
    IsAbs,
    Join,
    Basename,
    Dirname,
    Samefile,
    Splitext,
    Open,
    PyPath,
}

pub fn replaceable_by_pathlib(checker: &mut Checker, expr: &Expr) {
    if let Some(os_call) =
        checker
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["os", "path", "abspath"] => Some(OsCall::Abspath),
                ["os", "chmod"] => Some(OsCall::Chmod),
                ["os", "mkdir"] => Some(OsCall::Mkdir),
                ["os", "makedirs"] => Some(OsCall::Makedirs),
                ["os", "rename"] => Some(OsCall::Rename),
                ["os", "replace"] => Some(OsCall::Replace),
                ["os", "rmdir"] => Some(OsCall::Rmdir),
                ["os", "remove"] => Some(OsCall::Remove),
                ["os", "unlink"] => Some(OsCall::Unlink),
                ["os", "getcwd"] => Some(OsCall::Getcwd),
                ["os", "path", "exists"] => Some(OsCall::Exists),
                ["os", "path", "expanduser"] => Some(OsCall::Expanduser),
                ["os", "path", "isdir"] => Some(OsCall::IsDir),
                ["os", "path", "isfile"] => Some(OsCall::IsFile),
                ["os", "path", "islink"] => Some(OsCall::IsLink),
                ["os", "readlink"] => Some(OsCall::Readlink),
                ["os", "stat"] => Some(OsCall::Stat),
                ["os", "path", "isabs"] => Some(OsCall::IsAbs),
                ["os", "path", "join"] => Some(OsCall::Join),
                ["os", "path", "basename"] => Some(OsCall::Basename),
                ["os", "path", "dirname"] => Some(OsCall::Dirname),
                ["os", "path", "samefile"] => Some(OsCall::Samefile),
                ["os", "path", "splitext"] => Some(OsCall::Splitext),
                ["", "open"] => Some(OsCall::Open),
                ["py", "path", "local"] => Some(OsCall::PyPath),
                _ => None,
            })
    {
        let diagnostic = Diagnostic::new::<DiagnosticKind>(
            match os_call {
                OsCall::Abspath => PathlibAbspath.into(),
                OsCall::Chmod => PathlibChmod.into(),
                OsCall::Mkdir => PathlibMkdir.into(),
                OsCall::Makedirs => PathlibMakedirs.into(),
                OsCall::Rename => PathlibRename.into(),
                OsCall::Replace => PathlibReplace.into(),
                OsCall::Rmdir => PathlibRmdir.into(),
                OsCall::Remove => PathlibRemove.into(),
                OsCall::Unlink => PathlibUnlink.into(),
                OsCall::Getcwd => PathlibGetcwd.into(),
                OsCall::Exists => PathlibExists.into(),
                OsCall::Expanduser => PathlibExpanduser.into(),
                OsCall::IsDir => PathlibIsDir.into(),
                OsCall::IsFile => PathlibIsFile.into(),
                OsCall::IsLink => PathlibIsLink.into(),
                OsCall::Readlink => PathlibReadlink.into(),
                OsCall::Stat => PathlibStat.into(),
                OsCall::IsAbs => PathlibIsAbs.into(),
                OsCall::Join => PathlibJoin.into(),
                OsCall::Basename => PathlibBasename.into(),
                OsCall::Dirname => PathlibDirname.into(),
                OsCall::Samefile => PathlibSamefile.into(),
                OsCall::Splitext => PathlibSplitext.into(),
                OsCall::Open => PathlibOpen.into(),
                OsCall::PyPath => PathlibPyPath.into(),
            },
            Range::from_located(expr),
        );

        if checker.settings.rules.enabled(diagnostic.kind.rule()) {
            checker.diagnostics.push(diagnostic);
        }
    }
}
