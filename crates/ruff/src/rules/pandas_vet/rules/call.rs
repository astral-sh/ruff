use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

/// ## What it does
/// Checks for uses of `.isnull`.
///
/// ## Why is this bad?
/// `.isna` and `.isnull` are equivalent. For consistency, use `.isna` over
/// `.isnull`.
///
/// Further, `.isna` is a more accurate name for the functionality, as it
/// checks not only for `None` values, but also for `NaN` and `NaT` values.
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
#[violation]
pub struct PandasUseOfDotIsNull;

impl Violation for PandasUseOfDotIsNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.isna` is preferred to `.isnull`; functionality is equivalent")
    }
}

/// ## What it does
/// Checks for uses of `.notnull`.
///
/// ## Why is this bad?
/// `.notna` and `.notnull` are equivalent. For consistency, use `.notna` over
/// `.notnull`.
///
/// Further, `.notna` is a more accurate name for the functionality, as it
/// checks not only for `None` values, but also for `NaN` and `NaT` values.
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
#[violation]
pub struct PandasUseOfDotNotNull;

impl Violation for PandasUseOfDotNotNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.notna` is preferred to `.notnull`; functionality is equivalent")
    }
}

/// ## What it does
/// Checks for uses of `.pivot` or `.unstack`.
///
/// ## Why is this bad?
/// Prefer `.pivot_table` to `.pivot` or `.unstack`, which is a more general
/// method that can be used to implement `.pivot` and `.unstack`, and
/// provides the same functionality.
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
#[violation]
pub struct PandasUseOfDotPivotOrUnstack;

impl Violation for PandasUseOfDotPivotOrUnstack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality"
        )
    }
}

// TODO(tjkuson): Add documentation for this rule once clarified.
// https://github.com/astral-sh/ruff/issues/5628
#[violation]
pub struct PandasUseOfDotReadTable;

impl Violation for PandasUseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.read_csv` is preferred to `.read_table`; provides same functionality")
    }
}

/// ## What it does
/// Checks for uses of `.stack`.
///
/// ## Why is this bad?
/// Prefer `.melt` to `.stack`, which has the same functionality whilst
/// supporting direct column renaming and avoiding a `MultiIndex`, which is
/// generally more difficult to work with.
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
#[violation]
pub struct PandasUseOfDotStack;

impl Violation for PandasUseOfDotStack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.melt` is preferred to `.stack`; provides same functionality")
    }
}

pub(crate) fn call(checker: &mut Checker, func: &Expr) {
    let rules = &checker.settings.rules;
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func else {
        return;
    };
    let violation: DiagnosticKind = match attr.as_str() {
        "isnull" if rules.enabled(Rule::PandasUseOfDotIsNull) => PandasUseOfDotIsNull.into(),
        "notnull" if rules.enabled(Rule::PandasUseOfDotNotNull) => PandasUseOfDotNotNull.into(),
        "pivot" | "unstack" if rules.enabled(Rule::PandasUseOfDotPivotOrUnstack) => {
            PandasUseOfDotPivotOrUnstack.into()
        }
        "read_table" if rules.enabled(Rule::PandasUseOfDotReadTable) => {
            PandasUseOfDotReadTable.into()
        }
        "stack" if rules.enabled(Rule::PandasUseOfDotStack) => PandasUseOfDotStack.into(),
        _ => return,
    };

    // Ignore irrelevant bindings (like imports).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal | Resolution::PandasModule
    ) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, func.range()));
}
