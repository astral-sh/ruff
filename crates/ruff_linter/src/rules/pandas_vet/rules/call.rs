use ruff_python_ast::{self as ast, Expr};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{Resolution, test_expression};

/// ## What it does
/// Checks for uses of `.isnull` on Pandas objects.
///
/// ## Why is this bad?
/// In the Pandas API, `.isna` and `.isnull` are equivalent. For consistency,
/// prefer `.isna` over `.isnull`.
///
/// As a name, `.isna` more accurately reflects the behavior of the method,
/// since these methods check for `NaN` and `NaT` values in addition to `None`
/// values.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// animals_df = pd.read_csv("animals.csv")
/// pd.isnull(animals_df)
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// animals_df = pd.read_csv("animals.csv")
/// pd.isna(animals_df)
/// ```
///
/// ## References
/// - [Pandas documentation: `isnull`](https://pandas.pydata.org/docs/reference/api/pandas.isnull.html#pandas.isnull)
/// - [Pandas documentation: `isna`](https://pandas.pydata.org/docs/reference/api/pandas.isna.html#pandas.isna)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotIsNull;

impl Violation for PandasUseOfDotIsNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.isna` is preferred to `.isnull`; functionality is equivalent".to_string()
    }
}

/// ## What it does
/// Checks for uses of `.notnull` on Pandas objects.
///
/// ## Why is this bad?
/// In the Pandas API, `.notna` and `.notnull` are equivalent. For consistency,
/// prefer `.notna` over `.notnull`.
///
/// As a name, `.notna` more accurately reflects the behavior of the method,
/// since these methods check for `NaN` and `NaT` values in addition to `None`
/// values.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// animals_df = pd.read_csv("animals.csv")
/// pd.notnull(animals_df)
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// animals_df = pd.read_csv("animals.csv")
/// pd.notna(animals_df)
/// ```
///
/// ## References
/// - [Pandas documentation: `notnull`](https://pandas.pydata.org/docs/reference/api/pandas.notnull.html#pandas.notnull)
/// - [Pandas documentation: `notna`](https://pandas.pydata.org/docs/reference/api/pandas.notna.html#pandas.notna)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotNotNull;

impl Violation for PandasUseOfDotNotNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.notna` is preferred to `.notnull`; functionality is equivalent".to_string()
    }
}

/// ## What it does
/// Checks for uses of `.pivot` or `.unstack` on Pandas objects.
///
/// ## Why is this bad?
/// Prefer `.pivot_table` to `.pivot` or `.unstack`. `.pivot_table` is more general
/// and can be used to implement the same behavior as `.pivot` and `.unstack`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// df = pd.read_csv("cities.csv")
/// df.pivot(index="city", columns="year", values="population")
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// df = pd.read_csv("cities.csv")
/// df.pivot_table(index="city", columns="year", values="population")
/// ```
///
/// ## References
/// - [Pandas documentation: Reshaping and pivot tables](https://pandas.pydata.org/docs/user_guide/reshaping.html)
/// - [Pandas documentation: `pivot_table`](https://pandas.pydata.org/docs/reference/api/pandas.pivot_table.html#pandas.pivot_table)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotPivotOrUnstack;

impl Violation for PandasUseOfDotPivotOrUnstack {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality"
            .to_string()
    }
}

/// ## What it does
/// Checks for uses of `.stack` on Pandas objects.
///
/// ## Why is this bad?
/// Prefer `.melt` to `.stack`, which has the same functionality but with
/// support for direct column renaming and no dependence on `MultiIndex`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// cities_df = pd.read_csv("cities.csv")
/// cities_df.set_index("city").stack()
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// cities_df = pd.read_csv("cities.csv")
/// cities_df.melt(id_vars="city")
/// ```
///
/// ## References
/// - [Pandas documentation: `melt`](https://pandas.pydata.org/docs/reference/api/pandas.melt.html)
/// - [Pandas documentation: `stack`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.stack.html)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotStack;

impl Violation for PandasUseOfDotStack {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.melt` is preferred to `.stack`; provides same functionality".to_string()
    }
}

pub(crate) fn call(checker: &Checker, func: &Expr) {
    if !checker.semantic().seen_module(Modules::PANDAS) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func else {
        return;
    };

    // Ignore irrelevant bindings (like imports).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal | Resolution::PandasModule
    ) {
        return;
    }

    let range = func.range();
    match attr.as_str() {
        // PD003
        "isnull" if checker.is_rule_enabled(Rule::PandasUseOfDotIsNull) => {
            checker.report_diagnostic(PandasUseOfDotIsNull, range);
        }
        // PD004
        "notnull" if checker.is_rule_enabled(Rule::PandasUseOfDotNotNull) => {
            checker.report_diagnostic(PandasUseOfDotNotNull, range);
        }
        // PD010
        "pivot" | "unstack" if checker.is_rule_enabled(Rule::PandasUseOfDotPivotOrUnstack) => {
            checker.report_diagnostic(PandasUseOfDotPivotOrUnstack, range);
        }
        // PD013
        "stack" if checker.is_rule_enabled(Rule::PandasUseOfDotStack) => {
            checker.report_diagnostic(PandasUseOfDotStack, range);
        }
        _ => {}
    }
}
