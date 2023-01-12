pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::Settings;
    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;

    #[test_case(RuleCode::E401, Path::new("E40.py"))]
    #[test_case(RuleCode::E402, Path::new("E40.py"))]
    #[test_case(RuleCode::E402, Path::new("E402.py"))]
    #[test_case(RuleCode::E501, Path::new("E501.py"))]
    #[test_case(RuleCode::E711, Path::new("E711.py"))]
    #[test_case(RuleCode::E712, Path::new("E712.py"))]
    #[test_case(RuleCode::E713, Path::new("E713.py"))]
    #[test_case(RuleCode::E714, Path::new("E714.py"))]
    #[test_case(RuleCode::E721, Path::new("E721.py"))]
    #[test_case(RuleCode::E722, Path::new("E722.py"))]
    #[test_case(RuleCode::E731, Path::new("E731.py"))]
    #[test_case(RuleCode::E741, Path::new("E741.py"))]
    #[test_case(RuleCode::E742, Path::new("E742.py"))]
    #[test_case(RuleCode::E743, Path::new("E743.py"))]
    #[test_case(RuleCode::E999, Path::new("E999.py"))]
    #[test_case(RuleCode::W292, Path::new("W292_0.py"))]
    #[test_case(RuleCode::W292, Path::new("W292_1.py"))]
    #[test_case(RuleCode::W292, Path::new("W292_2.py"))]
    #[test_case(RuleCode::W292, Path::new("W292_3.py"))]
    #[test_case(RuleCode::W292, Path::new("W292_4.py"))]
    #[test_case(RuleCode::W605, Path::new("W605_0.py"))]
    #[test_case(RuleCode::W605, Path::new("W605_1.py"))]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/constant_literals.py"),
            &settings::Settings::for_rules(vec![RuleCode::E711, RuleCode::E712, RuleCode::F632]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test_case(false)]
    #[test_case(true)]
    fn task_tags(ignore_overlong_task_comments: bool) -> Result<()> {
        let snapshot = format!("task_tags_{ignore_overlong_task_comments}");
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/E501_1.py"),
            &settings::Settings {
                pycodestyle: Settings {
                    ignore_overlong_task_comments,
                    ..Settings::default()
                },
                ..settings::Settings::for_rule(RuleCode::E501)
            },
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn max_doc_length() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/W505.py"),
            &settings::Settings {
                pycodestyle: Settings {
                    max_doc_length: Some(50),
                    ..Settings::default()
                },
                ..settings::Settings::for_rule(RuleCode::W505)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
