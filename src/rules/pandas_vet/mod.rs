//! Rules from [pandas-vet](https://pypi.org/project/pandas-vet/).
pub(crate) mod fixes;
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::lexer::LexResult;
    use test_case::test_case;
    use textwrap::dedent;

    use crate::linter::{check_path, LinterResult};
    use crate::registry::{Rule, RuleCodePrefix};
    use crate::settings::flags;
    use crate::source_code::{Indexer, Locator, Stylist};
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, directives, rustpython_helpers, settings};

    fn rule_code(contents: &str, expected: &[Rule]) {
        let contents = dedent(contents);
        let settings = settings::Settings::for_rules(&RuleCodePrefix::PD);
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
        let locator = Locator::new(&contents);
        let stylist = Stylist::from_contents(&contents, &locator);
        let indexer: Indexer = tokens.as_slice().into();
        let directives =
            directives::extract_directives(&tokens, directives::Flags::from_settings(&settings));
        let LinterResult {
            data: diagnostics, ..
        } = check_path(
            Path::new("<filename>"),
            None,
            &contents,
            tokens,
            &locator,
            &stylist,
            &indexer,
            &directives,
            &settings,
            flags::Autofix::Enabled,
            flags::Noqa::Enabled,
        );
        let actual = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.kind.rule().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        x.drop(['a'], axis=1, inplace=False)
    "#, &[]; "PD002_pass")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        x.drop(['a'], axis=1, inplace=True)
    "#, &[Rule::UseOfInplaceArgument]; "PD002_fail")]
    #[test_case(r#"
        import pandas as pd
        nas = pd.isna(val)
    "#, &[]; "PD003_pass")]
    #[test_case(r#"
        import pandas as pd
        nulls = pd.isnull(val)
    "#, &[Rule::UseOfDotIsNull]; "PD003_fail")]
    #[test_case(r#"
        import pandas as pd
        print('bah humbug')
    "#, &[]; "PD003_allows_other_calls")]
    #[test_case(r#"
        import pandas as pd
        not_nas = pd.notna(val)
    "#, &[]; "PD004_pass")]
    #[test_case(r#"
        import pandas as pd
        not_nulls = pd.notnull(val)
    "#, &[Rule::UseOfDotNotNull]; "PD004_fail")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        new_x = x.loc['d':, 'A':'C']
    "#, &[]; "PD007_pass_loc")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        new_x = x.iloc[[1, 3, 5], [1, 3]]
    "#, &[]; "PD007_pass_iloc")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        y = x.ix[[0, 2], 'A']
    "#, &[Rule::UseOfDotIx]; "PD007_fail")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.loc[:, ['B', 'A']]
    "#, &[]; "PD008_pass")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.at[:, ['B', 'A']]
    "#, &[Rule::UseOfDotAt]; "PD008_fail")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.iloc[:, 1:3]
    "#, &[]; "PD009_pass")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.iat[:, 1:3]
    "#, &[Rule::UseOfDotIat]; "PD009_fail")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        table = x.pivot_table(
            x,
            values='D',
            index=['A', 'B'],
            columns=['C'],
            aggfunc=np.sum,
            fill_value=0
        )
    "#, &[]; "PD010_pass")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        table = pd.pivot(
            x,
            index='foo',
            columns='bar',
            values='baz'
        )
    "#, &[Rule::UseOfDotPivotOrUnstack]; "PD010_fail_pivot")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        result = x.to_array()
    "#, &[]; "PD011_pass_to_array")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        result = x.array
    "#, &[]; "PD011_pass_array")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        result = x.values
    "#, &[Rule::UseOfDotValues]; "PD011_fail_values")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        result = x.values()
    "#, &[]; "PD011_pass_values_call")]
    #[test_case(r#"
        import pandas as pd
        result = {}.values
    "#, &[]; "PD011_pass_values_dict")]
    #[test_case(r#"
        import pandas as pd
        result = pd.values
    "#, &[]; "PD011_pass_values_import")]
    #[test_case(r#"
        import pandas as pd
        result = x.values
    "#, &[]; "PD011_pass_values_unbound")]
    #[test_case(r#"
        import pandas as pd
        result = values
    "#, &[]; "PD011_pass_node_name")]
    #[test_case(r#"
        import pandas as pd
        employees = pd.read_csv(input_file)
    "#, &[]; "PD012_pass_read_csv")]
    #[test_case(r#"
        import pandas as pd
        employees = pd.read_table(input_file)
    "#, &[Rule::UseOfDotReadTable]; "PD012_fail_read_table")]
    #[test_case(r#"
        import pandas as pd
        employees = read_table
    "#, &[]; "PD012_node_Name_pass")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        y = x.melt(
            id_vars='airline',
            value_vars=['ATL', 'DEN', 'DFW'],
            value_name='airline delay'
        )
    "#, &[]; "PD013_pass")]
    #[test_case(r#"
        import numpy as np
        arrays = [np.random.randn(3, 4) for _ in range(10)]
        np.stack(arrays, axis=0).shape
    "#, &[]; "PD013_pass_numpy")]
    #[test_case(r#"
        import pandas as pd
        y = x.stack(level=-1, dropna=True)
    "#, &[]; "PD013_pass_unbound")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        y = x.stack(level=-1, dropna=True)
    "#, &[Rule::UseOfDotStack]; "PD013_fail_stack")]
    #[test_case(r#"
        import pandas as pd
        pd.stack(
    "#, &[]; "PD015_pass_merge_on_dataframe")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        y = pd.DataFrame()
        x.merge(y, 'inner')
    "#, &[]; "PD015_pass_merge_on_dataframe_with_multiple_args")]
    #[test_case(r#"
        import pandas as pd
        x = pd.DataFrame()
        y = pd.DataFrame()
        pd.merge(x, y)
    "#, &[Rule::UseOfPdMerge]; "PD015_fail_merge_on_pandas_object")]
    #[test_case(
        "pd.to_datetime(timestamp * 10 ** 9).strftime('%Y-%m-%d %H:%M:%S.%f')",
        &[];
        "PD015_pass_other_pd_function"
    )]
    #[test_case(r#"
        import pandas as pd
        employees = pd.DataFrame(employee_dict)
    "#, &[]; "PD901_pass_non_df")]
    #[test_case(r#"
        import pandas as pd
        employees_df = pd.DataFrame(employee_dict)
    "#, &[]; "PD901_pass_part_df")]
    #[test_case(r#"
        import pandas as pd
        my_function(df=data)
    "#, &[]; "PD901_pass_df_param")]
    #[test_case(r#"
        import pandas as pd
        df = pd.DataFrame()
    "#, &[Rule::DfIsABadVariableName]; "PD901_fail_df_var")]
    fn test_pandas_vet(code: &str, expected: &[Rule]) {
        rule_code(code, expected);
    }

    #[test_case(Rule::UseOfInplaceArgument, Path::new("PD002.py"); "PD002")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pandas_vet").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
