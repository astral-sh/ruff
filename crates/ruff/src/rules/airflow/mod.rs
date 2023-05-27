//! Airflow-specific rules
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rustpython_parser::lexer::LexResult;
    use test_case::test_case;
    use textwrap::dedent;

    use ruff_python_ast::source_code::{Indexer, Locator, Stylist};

    use crate::linter::{check_path, LinterResult};
    use crate::registry::{AsRule, Linter, Rule};
    use crate::settings::flags;
    use crate::{directives, settings};

    fn rule_code(contents: &str, expected: &[Rule]) {
        let contents = dedent(contents);
        let settings = settings::Settings::for_rules(&Linter::Airflow);
        let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&contents);
        let locator = Locator::new(&contents);
        let stylist = Stylist::from_tokens(&tokens, &locator);
        let indexer = Indexer::from_tokens(&tokens, &locator);
        let directives = directives::extract_directives(
            &tokens,
            directives::Flags::from_settings(&settings),
            &locator,
            &indexer,
        );
        let LinterResult {
            data: (diagnostics, _imports),
            ..
        } = check_path(
            Path::new("<filename>"),
            None,
            tokens,
            &locator,
            &stylist,
            &indexer,
            &directives,
            &settings,
            flags::Noqa::Enabled,
        );
        let actual: Vec<Rule> = diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.kind.rule())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test_case(r#"
        from airflow.operators import PythonOperator
        my_task = PythonOperator(task_id="my_task")
    "#, &[]; "AIR001_pass")]
    #[test_case(r#"
        from airflow.operators import PythonOperator
        incorrect_name = PythonOperator(task_id="my_task")
    "#, &[Rule::TaskVariableNameNotTaskId]; "AIR001_fail")]
    #[test_case(r#"
        from my_module import MyClass
        incorrect_name = MyClass(task_id="my_task")
        "#, &[]; "AIR001_noop")]

    fn test_airflow(code: &str, expected: &[Rule]) {
        rule_code(code, expected);
    }
}
