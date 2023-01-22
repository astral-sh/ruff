//! Rules from [flake8-use-pathlib](https://pypi.org/project/flake8-use-pathlib/).
pub(crate) mod helpers;
pub(crate) mod violations;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings;

    #[test_case(Path::new("full_name.py"); "PTH1_1")]
    #[test_case(Path::new("import_as.py"); "PTH1_2")]
    #[test_case(Path::new("import_from_as.py"); "PTH1_3")]
    #[test_case(Path::new("import_from.py"); "PTH1_4")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_use_pathlib")
                .join(path)
                .as_path(),
            &settings::Settings::for_rules(vec![
                Rule::PathlibAbspath,
                Rule::PathlibChmod,
                Rule::PathlibMkdir,
                Rule::PathlibMakedirs,
                Rule::PathlibRename,
                Rule::PathlibReplace,
                Rule::PathlibRmdir,
                Rule::PathlibRemove,
                Rule::PathlibUnlink,
                Rule::PathlibGetcwd,
                Rule::PathlibExists,
                Rule::PathlibExpanduser,
                Rule::PathlibIsDir,
                Rule::PathlibIsFile,
                Rule::PathlibIsLink,
                Rule::PathlibReadlink,
                Rule::PathlibStat,
                Rule::PathlibIsAbs,
                Rule::PathlibJoin,
                Rule::PathlibBasename,
                Rule::PathlibDirname,
                Rule::PathlibSamefile,
                Rule::PathlibSplitext,
            ]),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
