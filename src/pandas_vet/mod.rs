pub mod checks;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::lexer::LexResult;
    use test_case::test_case;
    use textwrap::dedent;

    use crate::checks::CheckCode;
    use crate::checks_gen::CheckCodePrefix;
    use crate::linter::check_path;
    use crate::source_code_locator::SourceCodeLocator;
    use crate::{directives, rustpython_helpers, settings};

    fn check_code(contents: &str, expected: &[CheckCode]) -> Result<()> {
        let contents = dedent(contents);
        let settings = settings::Settings::for_rules(CheckCodePrefix::PDV.codes());
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
        let locator = SourceCodeLocator::new(&contents);
        let directives = directives::extract_directives(
            &tokens,
            &locator,
            directives::Flags::from_settings(&settings),
        );
        let mut checks = check_path(
            Path::new("<filename>"),
            &contents,
            tokens,
            &locator,
            &directives,
            &settings,
            true,
            false,
        )?;
        checks.sort_by_key(|check| check.location);
        let actual = checks
            .iter()
            .map(|check| check.kind.code().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test_case("df.drop(['a'], axis=1, inplace=False)", &[]; "PDV002_pass")]
    #[test_case("df.drop(['a'], axis=1, inplace=True)", &[CheckCode::PDV002]; "PDV002_fail")]
    #[test_case("nas = pd.isna(val)", &[]; "PDV003_pass")]
    #[test_case("nulls = pd.isnull(val)", &[CheckCode::PDV003]; "PDV003_fail")]
    #[test_case("print('bah humbug')", &[]; "PDV003_allows_other_calls")]
    #[test_case("not_nas = pd.notna(val)", &[]; "PDV004_pass")]
    #[test_case("not_nulls = pd.notnull(val)", &[CheckCode::PDV004]; "PDV004_fail")]
    #[test_case("new_df = df.loc['d':, 'A':'C']", &[]; "PDV007_pass_loc")]
    #[test_case("new_df = df.iloc[[1, 3, 5], [1, 3]]", &[]; "PDV007_pass_iloc")]
    #[test_case("s = df.ix[[0, 2], 'A']", &[CheckCode::PDV007]; "PDV007_fail")]
    #[test_case("index = df.loc[:, ['B', 'A']]", &[]; "PDV008_pass")]
    #[test_case("index = df.at[:, ['B', 'A']]", &[CheckCode::PDV008]; "PDV008_fail")]
    #[test_case("index = df.iloc[:, 1:3]", &[]; "PDV009_pass")]
    #[test_case("index = df.iat[:, 1:3]", &[CheckCode::PDV009]; "PDV009_fail")]
    #[test_case(r#"table = df.pivot_table(
        df,
        values='D',
        index=['A', 'B'],
        columns=['C'],
        aggfunc=np.sum,
        fill_value=0
    )
    "#, &[]; "PDV010_pass")]
    #[test_case(r#"table = pd.pivot(
        df,
        index='foo',
        columns='bar',
        values='baz'
    )
    "#, &[CheckCode::PDV010]; "PDV010_fail_pivot")]
    #[test_case("result = df.to_array()", &[]; "PDV011_pass_to_array")]
    #[test_case("result = df.array", &[]; "PDV011_pass_array")]
    #[test_case("result = df.values", &[CheckCode::PDV011]; "PDV011_fail_values")]
    // TODO: Check that the attribute access is NOT a method call
    // #[test_case("result = {}.values()", &[]; "PDV011_pass_values_call")]
    #[test_case("result = values", &[]; "PDV011_pass_node_name")]
    #[test_case("employees = pd.read_csv(input_file)", &[]; "PDV012_pass_read_csv")]
    #[test_case("employees = pd.read_table(input_file)", &[CheckCode::PDV012]; "PDV012_fail_read_table")]
    #[test_case("employees = read_table", &[]; "PDV012_node_Name_pass")]
    #[test_case(r#"table = df.melt(
        id_vars='airline',
        value_vars=['ATL', 'DEN', 'DFW'],
        value_name='airline delay'
        )
    "#, &[]; "PDV013_pass")]
    #[test_case("table = df.stack(level=-1, dropna=True)", &[CheckCode::PDV013]; "PDV013_fail_stack")]
    #[test_case("employees = pd.DataFrame(employee_dict)", &[]; "PDV901_pass_non_df")]
    #[test_case("employees_df = pd.DataFrame(employee_dict)", &[]; "PDV901_pass_part_df")]
    #[test_case("my_function(df=data)", &[]; "PDV901_pass_df_param")]
    #[test_case("df = pd.DataFrame()", &[CheckCode::PDV901]; "PDV901_fail_df_var")]
    fn test_pandas_vet(code: &str, expected: &[CheckCode]) -> Result<()> {
        check_code(code, expected)?;
        Ok(())
    }
}
