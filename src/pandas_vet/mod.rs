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
    use crate::settings::flags;
    use crate::source_code_locator::SourceCodeLocator;
    use crate::{directives, rustpython_helpers, settings};

    fn check_code(contents: &str, expected: &[CheckCode]) -> Result<()> {
        let contents = dedent(contents);
        let settings = settings::Settings::for_rules(CheckCodePrefix::PD.codes());
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
        let locator = SourceCodeLocator::new(&contents);
        let directives = directives::extract_directives(
            &tokens,
            &locator,
            directives::Flags::from_settings(&settings),
        );
        let mut checks = check_path(
            Path::new("<filename>"),
            None,
            &contents,
            tokens,
            &locator,
            &directives,
            &settings,
            flags::Autofix::Enabled,
            flags::Noqa::Enabled,
        )?;
        checks.sort_by_key(|check| check.location);
        let actual = checks
            .iter()
            .map(|check| check.kind.code().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test_case("df.drop(['a'], axis=1, inplace=False)", &[]; "PD002_pass")]
    #[test_case("df.drop(['a'], axis=1, inplace=True)", &[CheckCode::PD002]; "PD002_fail")]
    #[test_case("nas = pd.isna(val)", &[]; "PD003_pass")]
    #[test_case("nulls = pd.isnull(val)", &[CheckCode::PD003]; "PD003_fail")]
    #[test_case("print('bah humbug')", &[]; "PD003_allows_other_calls")]
    #[test_case("not_nas = pd.notna(val)", &[]; "PD004_pass")]
    #[test_case("not_nulls = pd.notnull(val)", &[CheckCode::PD004]; "PD004_fail")]
    #[test_case("new_df = df.loc['d':, 'A':'C']", &[]; "PD007_pass_loc")]
    #[test_case("new_df = df.iloc[[1, 3, 5], [1, 3]]", &[]; "PD007_pass_iloc")]
    #[test_case("s = df.ix[[0, 2], 'A']", &[CheckCode::PD007]; "PD007_fail")]
    #[test_case("index = df.loc[:, ['B', 'A']]", &[]; "PD008_pass")]
    #[test_case("index = df.at[:, ['B', 'A']]", &[CheckCode::PD008]; "PD008_fail")]
    #[test_case("index = df.iloc[:, 1:3]", &[]; "PD009_pass")]
    #[test_case("index = df.iat[:, 1:3]", &[CheckCode::PD009]; "PD009_fail")]
    #[test_case(r#"table = df.pivot_table(
        df,
        values='D',
        index=['A', 'B'],
        columns=['C'],
        aggfunc=np.sum,
        fill_value=0
    )
    "#, &[]; "PD010_pass")]
    #[test_case(r#"table = pd.pivot(
        df,
        index='foo',
        columns='bar',
        values='baz'
    )
    "#, &[CheckCode::PD010]; "PD010_fail_pivot")]
    #[test_case("result = df.to_array()", &[]; "PD011_pass_to_array")]
    #[test_case("result = df.array", &[]; "PD011_pass_array")]
    #[test_case("result = df.values", &[CheckCode::PD011]; "PD011_fail_values")]
    // TODO(edgarrmondragon): Check that the attribute access is NOT a method call.
    // #[test_case("result = {}.values()", &[]; "PD011_pass_values_call")]
    #[test_case("result = values", &[]; "PD011_pass_node_name")]
    #[test_case("employees = pd.read_csv(input_file)", &[]; "PD012_pass_read_csv")]
    #[test_case("employees = pd.read_table(input_file)", &[CheckCode::PD012]; "PD012_fail_read_table")]
    #[test_case("employees = read_table", &[]; "PD012_node_Name_pass")]
    #[test_case(r#"table = df.melt(
        id_vars='airline',
        value_vars=['ATL', 'DEN', 'DFW'],
        value_name='airline delay'
        )
    "#, &[]; "PD013_pass")]
    #[test_case("table = df.stack(level=-1, dropna=True)", &[CheckCode::PD013]; "PD013_fail_stack")]
    #[test_case("df1.merge(df2)", &[]; "PD015_pass_merge_on_dataframe")]
    #[test_case("df1.merge(df2, 'inner')", &[]; "PD015_pass_merge_on_dataframe_with_multiple_args")]
    #[test_case("pd.merge(df1, df2)", &[CheckCode::PD015]; "PD015_fail_merge_on_pandas_object")]
    #[test_case(
        "pd.to_datetime(timestamp * 10 ** 9).strftime('%Y-%m-%d %H:%M:%S.%f')",
        &[];
        "PD015_pass_other_pd_function"
    )]
    #[test_case("employees = pd.DataFrame(employee_dict)", &[]; "PD901_pass_non_df")]
    #[test_case("employees_df = pd.DataFrame(employee_dict)", &[]; "PD901_pass_part_df")]
    #[test_case("my_function(df=data)", &[]; "PD901_pass_df_param")]
    #[test_case("df = pd.DataFrame()", &[CheckCode::PD901]; "PD901_fail_df_var")]
    fn test_pandas_vet(code: &str, expected: &[CheckCode]) -> Result<()> {
        check_code(code, expected)?;
        Ok(())
    }
}
