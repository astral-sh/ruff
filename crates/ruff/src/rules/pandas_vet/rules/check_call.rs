use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::binding::{BindingKind, Importation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::is_dataframe_candidate;

#[violation]
pub struct PandasUseOfDotIsNull;

impl Violation for PandasUseOfDotIsNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.isna` is preferred to `.isnull`; functionality is equivalent")
    }
}

#[violation]
pub struct PandasUseOfDotNotNull;

impl Violation for PandasUseOfDotNotNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.notna` is preferred to `.notnull`; functionality is equivalent")
    }
}

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

#[violation]
pub struct PandasUseOfDotReadTable;

impl Violation for PandasUseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.read_csv` is preferred to `.read_table`; provides same functionality")
    }
}

#[violation]
pub struct PandasUseOfDotStack;

impl Violation for PandasUseOfDotStack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.melt` is preferred to `.stack`; provides same functionality")
    }
}

pub fn check_call(checker: &mut Checker, func: &Expr) {
    let rules = &checker.settings.rules;
    let ExprKind::Attribute { value, attr, .. } = &func.node else {return};
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

    if !is_dataframe_candidate(value) {
        return;
    }

    // If the target is a named variable, avoid triggering on
    // irrelevant bindings (like non-Pandas imports).
    if let ExprKind::Name { id, .. } = &value.node {
        if checker.ctx.find_binding(id).map_or(true, |binding| {
            if let BindingKind::Importation(Importation {
                full_name: module, ..
            }) = &binding.kind
            {
                module != &"pandas"
            } else {
                matches!(
                    binding.kind,
                    BindingKind::Builtin
                        | BindingKind::ClassDefinition
                        | BindingKind::FunctionDefinition
                        | BindingKind::Export(..)
                        | BindingKind::FutureImportation
                        | BindingKind::Importation(..)
                        | BindingKind::FromImportation(..)
                        | BindingKind::SubmoduleImportation(..)
                )
            }
        }) {
            return;
        }
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, Range::from(func)));
}
