use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::rules::flake8_use_pathlib::violations::{
    PathlibAbspath, PathlibBasename, PathlibChmod, PathlibDirname, PathlibExists,
    PathlibExpanduser, PathlibGetcwd, PathlibIsAbs, PathlibIsDir, PathlibIsFile, PathlibIsLink,
    PathlibJoin, PathlibMakedirs, PathlibMkdir, PathlibReadlink, PathlibRemove, PathlibRename,
    PathlibReplace, PathlibRmdir, PathlibSamefile, PathlibSplitext, PathlibStat, PathlibUnlink,
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
}

pub fn replaceable_by_pathlib(checker: &mut Checker, expr: &Expr) {
    if let Some(os_call) =
        checker
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["os", "path", "abspath"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibAbspath) {
                        Some(OsCall::Abspath)
                    } else {
                        None
                    }
                }
                ["os", "chmod"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibChmod) {
                        Some(OsCall::Chmod)
                    } else {
                        None
                    }
                }
                ["os", "mkdir"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibMkdir) {
                        Some(OsCall::Mkdir)
                    } else {
                        None
                    }
                }
                ["os", "makedirs"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibMakedirs) {
                        Some(OsCall::Makedirs)
                    } else {
                        None
                    }
                }
                ["os", "rename"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibRename) {
                        Some(OsCall::Rename)
                    } else {
                        None
                    }
                }
                ["os", "replace"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibReplace) {
                        Some(OsCall::Replace)
                    } else {
                        None
                    }
                }
                ["os", "rmdir"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibRmdir) {
                        Some(OsCall::Rmdir)
                    } else {
                        None
                    }
                }
                ["os", "remove"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibRemove) {
                        Some(OsCall::Remove)
                    } else {
                        None
                    }
                }
                ["os", "unlink"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibUnlink) {
                        Some(OsCall::Unlink)
                    } else {
                        None
                    }
                }
                ["os", "getcwd"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibGetcwd) {
                        Some(OsCall::Getcwd)
                    } else {
                        None
                    }
                }
                ["os", "path", "exists"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibExists) {
                        Some(OsCall::Exists)
                    } else {
                        None
                    }
                }
                ["os", "path", "expanduser"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibExpanduser) {
                        Some(OsCall::Expanduser)
                    } else {
                        None
                    }
                }
                ["os", "path", "isdir"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibIsDir) {
                        Some(OsCall::IsDir)
                    } else {
                        None
                    }
                }
                ["os", "path", "isfile"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibIsFile) {
                        Some(OsCall::IsFile)
                    } else {
                        None
                    }
                }
                ["os", "path", "islink"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibIsLink) {
                        Some(OsCall::IsLink)
                    } else {
                        None
                    }
                }
                ["os", "readlink"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibReadlink) {
                        Some(OsCall::Readlink)
                    } else {
                        None
                    }
                }
                ["os", "stat"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibStat) {
                        Some(OsCall::Stat)
                    } else {
                        None
                    }
                }
                ["os", "path", "isabs"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibIsAbs) {
                        Some(OsCall::IsAbs)
                    } else {
                        None
                    }
                }
                ["os", "path", "join"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibJoin) {
                        Some(OsCall::Join)
                    } else {
                        None
                    }
                }
                ["os", "path", "basename"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibBasename) {
                        Some(OsCall::Basename)
                    } else {
                        None
                    }
                }
                ["os", "path", "dirname"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibDirname) {
                        Some(OsCall::Dirname)
                    } else {
                        None
                    }
                }
                ["os", "path", "samefile"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibSamefile) {
                        Some(OsCall::Samefile)
                    } else {
                        None
                    }
                }
                ["os", "path", "splitext"] => {
                    if checker.settings.rules.enabled(&Rule::PathlibSplitext) {
                        Some(OsCall::Splitext)
                    } else {
                        None
                    }
                }
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
            },
            Range::from_located(expr),
        );
        checker.diagnostics.push(diagnostic);
    }
}
