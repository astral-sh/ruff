pub mod cformat;
pub mod checks;
pub mod fixes;
pub mod format;
pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::F401, Path::new("F401_0.py"); "F401_0")]
    #[test_case(CheckCode::F401, Path::new("F401_1.py"); "F401_1")]
    #[test_case(CheckCode::F401, Path::new("F401_2.py"); "F401_2")]
    #[test_case(CheckCode::F401, Path::new("F401_3.py"); "F401_3")]
    #[test_case(CheckCode::F401, Path::new("F401_4.py"); "F401_4")]
    #[test_case(CheckCode::F401, Path::new("F401_5.py"); "F401_5")]
    #[test_case(CheckCode::F401, Path::new("F401_6.py"); "F401_6")]
    #[test_case(CheckCode::F402, Path::new("F402.py"); "F402")]
    #[test_case(CheckCode::F403, Path::new("F403.py"); "F403")]
    #[test_case(CheckCode::F404, Path::new("F404.py"); "F404")]
    #[test_case(CheckCode::F405, Path::new("F405.py"); "F405")]
    #[test_case(CheckCode::F406, Path::new("F406.py"); "F406")]
    #[test_case(CheckCode::F407, Path::new("F407.py"); "F407")]
    #[test_case(CheckCode::F501, Path::new("F50x.py"); "F501")]
    #[test_case(CheckCode::F502, Path::new("F502.py"); "F502_1")]
    #[test_case(CheckCode::F502, Path::new("F50x.py"); "F502_0")]
    #[test_case(CheckCode::F503, Path::new("F503.py"); "F503_1")]
    #[test_case(CheckCode::F503, Path::new("F50x.py"); "F503_0")]
    #[test_case(CheckCode::F504, Path::new("F504.py"); "F504_1")]
    #[test_case(CheckCode::F504, Path::new("F50x.py"); "F504_0")]
    #[test_case(CheckCode::F505, Path::new("F504.py"); "F505_1")]
    #[test_case(CheckCode::F505, Path::new("F50x.py"); "F505_0")]
    #[test_case(CheckCode::F506, Path::new("F50x.py"); "F506")]
    #[test_case(CheckCode::F507, Path::new("F50x.py"); "F507")]
    #[test_case(CheckCode::F508, Path::new("F50x.py"); "F508")]
    #[test_case(CheckCode::F509, Path::new("F50x.py"); "F509")]
    #[test_case(CheckCode::F521, Path::new("F521.py"); "F521")]
    #[test_case(CheckCode::F522, Path::new("F522.py"); "F522")]
    #[test_case(CheckCode::F523, Path::new("F523.py"); "F523")]
    #[test_case(CheckCode::F524, Path::new("F524.py"); "F524")]
    #[test_case(CheckCode::F525, Path::new("F525.py"); "F525")]
    #[test_case(CheckCode::F541, Path::new("F541.py"); "F541")]
    #[test_case(CheckCode::F601, Path::new("F601.py"); "F601")]
    #[test_case(CheckCode::F602, Path::new("F602.py"); "F602")]
    #[test_case(CheckCode::F622, Path::new("F622.py"); "F622")]
    #[test_case(CheckCode::F631, Path::new("F631.py"); "F631")]
    #[test_case(CheckCode::F632, Path::new("F632.py"); "F632")]
    #[test_case(CheckCode::F633, Path::new("F633.py"); "F633")]
    #[test_case(CheckCode::F634, Path::new("F634.py"); "F634")]
    #[test_case(CheckCode::F701, Path::new("F701.py"); "F701")]
    #[test_case(CheckCode::F702, Path::new("F702.py"); "F702")]
    #[test_case(CheckCode::F704, Path::new("F704.py"); "F704")]
    #[test_case(CheckCode::F706, Path::new("F706.py"); "F706")]
    #[test_case(CheckCode::F707, Path::new("F707.py"); "F707")]
    #[test_case(CheckCode::F722, Path::new("F722.py"); "F722")]
    #[test_case(CheckCode::F821, Path::new("F821_0.py"); "F821_0")]
    #[test_case(CheckCode::F821, Path::new("F821_1.py"); "F821_1")]
    #[test_case(CheckCode::F821, Path::new("F821_2.py"); "F821_2")]
    #[test_case(CheckCode::F821, Path::new("F821_3.py"); "F821_3")]
    #[test_case(CheckCode::F821, Path::new("F821_4.py"); "F821_4")]
    #[test_case(CheckCode::F821, Path::new("F821_5.py"); "F821_5")]
    #[test_case(CheckCode::F822, Path::new("F822.py"); "F822")]
    #[test_case(CheckCode::F823, Path::new("F823.py"); "F823")]
    #[test_case(CheckCode::F831, Path::new("F831.py"); "F831")]
    #[test_case(CheckCode::F841, Path::new("F841_0.py"); "F841_0")]
    #[test_case(CheckCode::F841, Path::new("F841_1.py"); "F841_1")]
    #[test_case(CheckCode::F901, Path::new("F901.py"); "F901")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn f841_dummy_variable_rgx() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/F841_0.py"),
            &settings::Settings {
                dummy_variable_rgx: Regex::new(r"^z$").unwrap(),
                ..settings::Settings::for_rule(CheckCode::F841)
            },
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn init() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/__init__.py"),
            &settings::Settings::for_rules(vec![CheckCode::F821, CheckCode::F822]),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/future_annotations.py"),
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F821]),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
