pub mod checks;
pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::Settings;
    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::settings;

    #[test_case(DiagnosticCode::E401, Path::new("E40.py"))]
    #[test_case(DiagnosticCode::E402, Path::new("E40.py"))]
    #[test_case(DiagnosticCode::E402, Path::new("E402.py"))]
    #[test_case(DiagnosticCode::E501, Path::new("E501.py"))]
    #[test_case(DiagnosticCode::E711, Path::new("E711.py"))]
    #[test_case(DiagnosticCode::E712, Path::new("E712.py"))]
    #[test_case(DiagnosticCode::E713, Path::new("E713.py"))]
    #[test_case(DiagnosticCode::E714, Path::new("E714.py"))]
    #[test_case(DiagnosticCode::E721, Path::new("E721.py"))]
    #[test_case(DiagnosticCode::E722, Path::new("E722.py"))]
    #[test_case(DiagnosticCode::E731, Path::new("E731.py"))]
    #[test_case(DiagnosticCode::E741, Path::new("E741.py"))]
    #[test_case(DiagnosticCode::E742, Path::new("E742.py"))]
    #[test_case(DiagnosticCode::E743, Path::new("E743.py"))]
    #[test_case(DiagnosticCode::E999, Path::new("E999.py"))]
    #[test_case(DiagnosticCode::W292, Path::new("W292_0.py"))]
    #[test_case(DiagnosticCode::W292, Path::new("W292_1.py"))]
    #[test_case(DiagnosticCode::W292, Path::new("W292_2.py"))]
    #[test_case(DiagnosticCode::W292, Path::new("W292_3.py"))]
    #[test_case(DiagnosticCode::W292, Path::new("W292_4.py"))]
    #[test_case(DiagnosticCode::W605, Path::new("W605_0.py"))]
    #[test_case(DiagnosticCode::W605, Path::new("W605_1.py"))]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/constant_literals.py"),
            &settings::Settings::for_rules(vec![
                DiagnosticCode::E711,
                DiagnosticCode::E712,
                DiagnosticCode::F632,
            ]),
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test_case(false)]
    #[test_case(true)]
    fn task_tags(ignore_overlong_task_comments: bool) -> Result<()> {
        let snapshot = format!("task_tags_{ignore_overlong_task_comments}");
        let checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/E501_1.py"),
            &settings::Settings {
                pycodestyle: Settings {
                    ignore_overlong_task_comments,
                },
                ..settings::Settings::for_rule(DiagnosticCode::E501)
            },
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
