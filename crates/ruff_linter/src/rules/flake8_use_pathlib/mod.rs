//! Rules from [flake8-use-pathlib](https://pypi.org/project/flake8-use-pathlib/).
mod helpers;
pub(crate) mod rules;
pub(crate) mod violations;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use ruff_python_ast::PythonVersion;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::settings;
    use crate::settings::types::PreviewMode;
    use crate::test::test_path;

    #[test_case(Path::new("full_name.py"))]
    #[test_case(Path::new("import_as.py"))]
    #[test_case(Path::new("import_from_as.py"))]
    #[test_case(Path::new("import_from.py"))]
    #[test_case(Path::new("use_pathlib.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::LinterSettings::for_rules(vec![
                Rule::OsPathAbspath,
                Rule::OsChmod,
                Rule::OsMkdir,
                Rule::OsMakedirs,
                Rule::OsRename,
                Rule::OsReplace,
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
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::PyPath, Path::new("py_path_1.py"))]
    #[test_case(Rule::PyPath, Path::new("py_path_2.py"))]
    #[test_case(Rule::PathConstructorCurrentDirectory, Path::new("PTH201.py"))]
    #[test_case(Rule::OsPathGetsize, Path::new("PTH202.py"))]
    #[test_case(Rule::OsPathGetsize, Path::new("PTH202_2.py"))]
    #[test_case(Rule::OsPathGetatime, Path::new("PTH203.py"))]
    #[test_case(Rule::OsPathGetmtime, Path::new("PTH204.py"))]
    #[test_case(Rule::OsPathGetctime, Path::new("PTH205.py"))]
    #[test_case(Rule::OsSepSplit, Path::new("PTH206.py"))]
    #[test_case(Rule::Glob, Path::new("PTH207.py"))]
    #[test_case(Rule::OsListdir, Path::new("PTH208.py"))]
    #[test_case(Rule::InvalidPathlibWithSuffix, Path::new("PTH210.py"))]
    #[test_case(Rule::InvalidPathlibWithSuffix, Path::new("PTH210_1.py"))]
    #[test_case(Rule::OsSymlink, Path::new("PTH211.py"))]
    fn rules_pypath(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("full_name.py"))]
    #[test_case(Path::new("import_as.py"))]
    #[test_case(Path::new("import_from_as.py"))]
    #[test_case(Path::new("import_from.py"))]
    fn preview_rules(path: &Path) -> Result<()> {
        let snapshot = format!("preview_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rules(vec![
                    Rule::OsPathAbspath,
                    Rule::OsChmod,
                    Rule::OsMkdir,
                    Rule::OsMakedirs,
                    Rule::OsRename,
                    Rule::OsReplace,
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
                ])
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::PathConstructorCurrentDirectory, Path::new("PTH201.py"))]
    #[test_case(Rule::OsPathGetsize, Path::new("PTH202.py"))]
    #[test_case(Rule::OsPathGetsize, Path::new("PTH202_2.py"))]
    #[test_case(Rule::OsPathGetatime, Path::new("PTH203.py"))]
    #[test_case(Rule::OsPathGetmtime, Path::new("PTH204.py"))]
    #[test_case(Rule::OsPathGetctime, Path::new("PTH205.py"))]
    fn preview_flake8_use_pathlib(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::InvalidPathlibWithSuffix, Path::new("PTH210_2.py"))]
    fn pathlib_with_suffix_py314(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "py314__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_use_pathlib").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY314.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
