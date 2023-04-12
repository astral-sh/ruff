//! Rules from [flake8-use-pathlib](https://pypi.org/project/flake8-use-pathlib/).
pub(crate) mod helpers;
pub(crate) mod violations;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Path::new("full_name.py"); "PTH1_1")]
    #[test_case(Path::new("import_as.py"); "PTH1_2")]
    #[test_case(Path::new("import_from_as.py"); "PTH1_3")]
    #[test_case(Path::new("import_from.py"); "PTH1_4")]
    #[test_case(Path::new("use_pathlib.py"); "PTH1_5")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::Settings::for_rules(vec![
                Rule::OsPathAbspath,
                Rule::OsChmod,
                Rule::OsMkdir,
                Rule::OsMakedirs,
                Rule::OsRename,
                Rule::PathlibReplace,
                Rule::OsRmdir,
                Rule::OsRemove,
                Rule::OsUnlink,
                Rule::OsGetcwd,
                Rule::OsPathExists,
                Rule::OsPathExpanduser,
                Rule::OsPathIsdir,
                Rule::OsPathIsfile,
                Rule::OsPathIslink,
                Rule::OsReadlink,
                Rule::OsStat,
                Rule::OsPathIsabs,
                Rule::OsPathJoin,
                Rule::OsPathBasename,
                Rule::OsPathDirname,
                Rule::OsPathSamefile,
                Rule::OsPathSplitext,
                Rule::BuiltinOpen,
            ]),
        )?;
        crate::assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::PyPath, Path::new("py_path_1.py"); "PTH024_1")]
    #[test_case(Rule::PyPath, Path::new("py_path_2.py"); "PTH024_2")]
    fn rules_pypath(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        crate::assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
