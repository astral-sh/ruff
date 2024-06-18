//! Rules from [pandas-vet](https://pypi.org/project/pandas-vet/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::{Linter, Rule};
    use crate::test::{test_path, test_snippet};
    use crate::{assert_messages, settings};

    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        x.drop(["a"], axis=1, inplace=False)
    "#,
        "PD002_pass"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        x.drop(["a"], axis=1, inplace=True)
    "#,
        "PD002_fail"
    )]
    #[test_case(
        r"
        import pandas as pd
        nas = pd.isna(val)
    ",
        "PD003_pass"
    )]
    #[test_case(
        r"
        import pandas as pd
        nulls = pd.isnull(val)
    ",
        "PD003_fail"
    )]
    #[test_case(
        r#"
        import pandas as pd
        print("bah humbug")
    "#,
        "PD003_allows_other_calls"
    )]
    #[test_case(
        r"
        import pandas as pd
        not_nas = pd.notna(val)
    ",
        "PD004_pass"
    )]
    #[test_case(
        r"
        import pandas as pd
        not_nulls = pd.notnull(val)
    ",
        "PD004_fail"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        new_x = x.loc["d":, "A":"C"]
    "#,
        "PD007_pass_loc"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        new_x = x.iloc[[1, 3, 5], [1, 3]]
    ",
        "PD007_pass_iloc"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        y = x.ix[[0, 2], "A"]
    "#,
        "PD007_fail"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.loc[:, ["B", "A"]]
    "#,
        "PD008_pass"
    )]
    #[test_case(
        r#"
        import io
        import zipfile


        class MockBinaryFile(io.BytesIO):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, **kwargs)

            def close(self):
                pass  # Don"t allow closing the file, it would clear the buffer


        zip_buffer = MockBinaryFile()

        with zipfile.ZipFile(zip_buffer, "w") as zf:
            zf.writestr("dir/file.txt", "This is a test")

        # Reset the BytesIO object"s cursor to the start.
        zip_buffer.seek(0)

        with zipfile.ZipFile(zip_buffer, "r") as zf:
            zpath = zipfile.Path(zf, "/")

        dir_name, file_name = zpath.at.split("/")
    "#,
        "PD008_pass_on_attr"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        index = x.at[:, ["B", "A"]]
    "#,
        "PD008_fail"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        index = x.iloc[:, 1:3]
    ",
        "PD009_pass"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        index = x.iat[:, 1:3]
    ",
        "PD009_fail"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        table = x.pivot_table(
            x,
            values="D",
            index=["A", "B"],
            columns=["C"],
            aggfunc=np.sum,
            fill_value=0
        )
    "#,
        "PD010_pass"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        table = pd.pivot(
            x,
            index="foo",
            columns="bar",
            values="baz"
        )
    "#,
        "PD010_fail_pivot"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        result = x.to_array()
    ",
        "PD011_pass_to_array"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        result = x.array
    ",
        "PD011_pass_array"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        result = x.values
    ",
        "PD011_fail_values"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        result = x.values()
    ",
        "PD011_pass_values_call"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        x.values = 1
    ",
        "PD011_pass_values_store"
    )]
    #[test_case(
        r"
        class Class:
            def __init__(self, values: str) -> None:
                self.values = values
                print(self.values)
    ",
        "PD011_pass_values_instance"
    )]
    #[test_case(
        r"
        import pandas as pd
        result = {}.values
    ",
        "PD011_pass_values_dict"
    )]
    #[test_case(
        r"
        import pandas as pd
        result = pd.values
    ",
        "PD011_pass_values_import"
    )]
    #[test_case(
        r"
        import pandas as pd
        result = x.values
    ",
        "PD011_pass_values_unbound"
    )]
    #[test_case(
        r"
        import pandas as pd
        result = values
    ",
        "PD011_pass_node_name"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        y = x.melt(
            id_vars="airline",
            value_vars=["ATL", "DEN", "DFW"],
            value_name="airline delay"
        )
    "#,
        "PD013_pass"
    )]
    #[test_case(
        r"
        import numpy as np
        arrays = [np.random.randn(3, 4) for _ in range(10)]
        np.stack(arrays, axis=0).shape
    ",
        "PD013_pass_numpy"
    )]
    #[test_case(
        r"
        import pandas as pd
        y = x.stack(level=-1, dropna=True)
    ",
        "PD013_pass_unbound"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        y = x.stack(level=-1, dropna=True)
    ",
        "PD013_fail_stack"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        y = pd.DataFrame()
        x.merge(y)
    ",
        "PD015_pass_merge_on_dataframe"
    )]
    #[test_case(
        r#"
        import pandas as pd
        x = pd.DataFrame()
        y = pd.DataFrame()
        x.merge(y, "inner")
    "#,
        "PD015_pass_merge_on_dataframe_with_multiple_args"
    )]
    #[test_case(
        r"
        import pandas as pd
        x = pd.DataFrame()
        y = pd.DataFrame()
        pd.merge(x, y)
    ",
        "PD015_fail_merge_on_pandas_object"
    )]
    #[test_case(
        r#"
        pd.to_datetime(timestamp * 10 ** 9).strftime("%Y-%m-%d %H:%M:%S.%f")
    "#,
        "PD015_pass_other_pd_function"
    )]
    #[test_case(
        r"
        import pandas as pd
        employees = pd.DataFrame(employee_dict)
    ",
        "PD901_pass_non_df"
    )]
    #[test_case(
        r"
        import pandas as pd
        employees_df = pd.DataFrame(employee_dict)
    ",
        "PD901_pass_part_df"
    )]
    #[test_case(
        r"
        import pandas as pd
        my_function(df=data)
    ",
        "PD901_pass_df_param"
    )]
    #[test_case(
        r"
        import pandas as pd
        df = pd.DataFrame()
    ",
        "PD901_fail_df_var"
    )]
    fn contents(contents: &str, snapshot: &str) {
        let diagnostics = test_snippet(
            contents,
            &settings::LinterSettings::for_rules(Linter::PandasVet.rules()),
        );
        assert_messages!(snapshot, diagnostics);
    }

    #[test_case(
        Rule::PandasUseOfDotReadTable,
        Path::new("pandas_use_of_dot_read_table.py")
    )]
    #[test_case(Rule::PandasUseOfInplaceArgument, Path::new("PD002.py"))]
    #[test_case(Rule::PandasNuniqueConstantSeriesCheck, Path::new("PD101.py"))]
    fn paths(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pandas_vet").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
