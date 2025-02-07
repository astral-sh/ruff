use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

/// ## What it does
/// Checks for uses of `.ix` on Pandas objects.
///
/// ## Why is this bad?
/// The `.ix` method is deprecated as its behavior is ambiguous. Specifically,
/// it's often unclear whether `.ix` is indexing by label or by ordinal position.
///
/// Instead, prefer the `.loc` method for label-based indexing, and `.iloc` for
/// ordinal indexing.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.ix[0]  # 0th row or row with label 0?
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.iloc[0]  # 0th row.
/// ```
///
/// ## References
/// - [Pandas release notes: Deprecate `.ix`](https://pandas.pydata.org/pandas-docs/version/0.20/whatsnew.html#deprecate-ix)
/// - [Pandas documentation: `loc`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.loc.html)
/// - [Pandas documentation: `iloc`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.iloc.html)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotIx;

impl Violation for PandasUseOfDotIx {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.ix` is deprecated; use more explicit `.loc` or `.iloc`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `.at` on Pandas objects.
///
/// ## Why is this bad?
/// The `.at` method selects a single value from a `DataFrame` or Series based on
/// a label index, and is slightly faster than using `.loc`. However, `.loc` is
/// more idiomatic and versatile, as it can be used to select multiple values at
/// once.
///
/// If performance is an important consideration, convert the object to a NumPy
/// array, which will provide a much greater performance boost than using `.at`
/// over `.loc`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.at["Maria"]
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.loc["Maria"]
/// ```
///
/// ## References
/// - [Pandas documentation: `loc`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.loc.html)
/// - [Pandas documentation: `at`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.at.html)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotAt;

impl Violation for PandasUseOfDotAt {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.loc` instead of `.at`. If speed is important, use NumPy.".to_string()
    }
}

/// ## What it does
/// Checks for uses of `.iat` on Pandas objects.
///
/// ## Why is this bad?
/// The `.iat` method selects a single value from a `DataFrame` or Series based
/// on an ordinal index, and is slightly faster than using `.iloc`. However,
/// `.iloc` is more idiomatic and versatile, as it can be used to select
/// multiple values at once.
///
/// If performance is an important consideration, convert the object to a NumPy
/// array, which will provide a much greater performance boost than using `.iat`
/// over `.iloc`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.iat[0]
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.iloc[0]
/// ```
///
/// Or, using NumPy:
/// ```python
/// import numpy as np
/// import pandas as pd
///
/// students_df = pd.read_csv("students.csv")
/// students_df.to_numpy()[0]
/// ```
///
/// ## References
/// - [Pandas documentation: `iloc`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.iloc.html)
/// - [Pandas documentation: `iat`](https://pandas.pydata.org/docs/reference/api/pandas.DataFrame.iat.html)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotIat;

impl Violation for PandasUseOfDotIat {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.iloc` instead of `.iat`. If speed is important, use NumPy.".to_string()
    }
}

pub(crate) fn subscript(checker: &Checker, value: &Expr, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::PANDAS) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = value else {
        return;
    };

    let violation: DiagnosticKind = match attr.as_str() {
        "ix" if checker.settings.rules.enabled(Rule::PandasUseOfDotIx) => PandasUseOfDotIx.into(),
        "at" if checker.settings.rules.enabled(Rule::PandasUseOfDotAt) => PandasUseOfDotAt.into(),
        "iat" if checker.settings.rules.enabled(Rule::PandasUseOfDotIat) => {
            PandasUseOfDotIat.into()
        }
        _ => return,
    };

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.at[0]`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(violation, expr.range()));
}
