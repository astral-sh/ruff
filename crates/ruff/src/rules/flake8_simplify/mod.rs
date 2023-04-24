//! Rules from [flake8-simplify](https://pypi.org/project/flake8-simplify/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::DuplicateIsinstanceCall, Path::new("SIM101.py"); "SIM101")]
    #[test_case(Rule::CollapsibleIf, Path::new("SIM102.py"); "SIM102")]
    #[test_case(Rule::NeedlessBool, Path::new("SIM103.py"); "SIM103")]
    #[test_case(Rule::SuppressibleException, Path::new("SIM105.py"); "SIM105")]
    #[test_case(Rule::ReturnInTryExceptFinally, Path::new("SIM107.py"); "SIM107")]
    #[test_case(Rule::IfElseBlockInsteadOfIfExp, Path::new("SIM108.py"); "SIM108")]
    #[test_case(Rule::CompareWithTuple, Path::new("SIM109.py"); "SIM109")]
    #[test_case(Rule::ReimplementedBuiltin, Path::new("SIM110.py"); "SIM110")]
    #[test_case(Rule::ReimplementedBuiltin, Path::new("SIM111.py"); "SIM111")]
    #[test_case(Rule::UncapitalizedEnvironmentVariables, Path::new("SIM112.py"); "SIM112")]
    #[test_case(Rule::OpenFileWithContextHandler, Path::new("SIM115.py"); "SIM115")]
    #[test_case(Rule::MultipleWithStatements, Path::new("SIM117.py"); "SIM117")]
    #[test_case(Rule::InDictKeys, Path::new("SIM118.py"); "SIM118")]
    #[test_case(Rule::NegateEqualOp, Path::new("SIM201.py"); "SIM201")]
    #[test_case(Rule::NegateNotEqualOp, Path::new("SIM202.py"); "SIM202")]
    #[test_case(Rule::DoubleNegation, Path::new("SIM208.py"); "SIM208")]
    #[test_case(Rule::IfExprWithTrueFalse, Path::new("SIM210.py"); "SIM210")]
    #[test_case(Rule::IfExprWithFalseTrue, Path::new("SIM211.py"); "SIM211")]
    #[test_case(Rule::IfExprWithTwistedArms, Path::new("SIM212.py"); "SIM212")]
    #[test_case(Rule::ExprAndNotExpr, Path::new("SIM220.py"); "SIM220")]
    #[test_case(Rule::ExprOrNotExpr, Path::new("SIM221.py"); "SIM221")]
    #[test_case(Rule::ExprOrTrue, Path::new("SIM222.py"); "SIM222")]
    #[test_case(Rule::ExprAndFalse, Path::new("SIM223.py"); "SIM223")]
    #[test_case(Rule::YodaConditions, Path::new("SIM300.py"); "SIM300")]
    #[test_case(Rule::IfElseBlockInsteadOfDictGet, Path::new("SIM401.py"); "SIM401")]
    #[test_case(Rule::DictGetWithNoneDefault, Path::new("SIM910.py"); "SIM910")]
    #[test_case(Rule::IfElseBlockInsteadOfDictLookup, Path::new("SIM116.py"); "SIM116")]
    #[test_case(Rule::IfWithSameArms, Path::new("SIM114.py"); "SIM114")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_simplify").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
