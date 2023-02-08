use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Located};

use crate::ast::types::{BindingKind, Range};
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::rules::pandas_vet::helpers::is_dataframe_candidate;
use crate::violation::Violation;

define_violation!(
    pub struct UseOfDotIsNull;
);
impl Violation for UseOfDotIsNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.isna` is preferred to `.isnull`; functionality is equivalent")
    }
}

define_violation!(
    pub struct UseOfDotNotNull;
);
impl Violation for UseOfDotNotNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.notna` is preferred to `.notnull`; functionality is equivalent")
    }
}

define_violation!(
    pub struct UseOfDotPivotOrUnstack;
);
impl Violation for UseOfDotPivotOrUnstack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality"
        )
    }
}

define_violation!(
    pub struct UseOfDotReadTable;
);
impl Violation for UseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.read_csv` is preferred to `.read_table`; provides same functionality")
    }
}

define_violation!(
    pub struct UseOfDotStack;
);
impl Violation for UseOfDotStack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.melt` is preferred to `.stack`; provides same functionality")
    }
}

pub fn check_call(checker: &mut Checker, func: &Located<ExprKind>) {
    let rules = &checker.settings.rules;
    let ExprKind::Attribute { value, attr, .. } = &func.node else {return};
    let violation: DiagnosticKind = match attr.as_str() {
        "isnull" if rules.enabled(&Rule::UseOfDotIsNull) => UseOfDotIsNull.into(),
        "notnull" if rules.enabled(&Rule::UseOfDotNotNull) => UseOfDotNotNull.into(),
        "pivot" | "unstack" if rules.enabled(&Rule::UseOfDotPivotOrUnstack) => {
            UseOfDotPivotOrUnstack.into()
        }
        "read_table" if rules.enabled(&Rule::UseOfDotReadTable) => UseOfDotReadTable.into(),
        "stack" if rules.enabled(&Rule::UseOfDotStack) => UseOfDotStack.into(),
        _ => return,
    };

    if !is_dataframe_candidate(value) {
        return;
    }

    // If the target is a named variable, avoid triggering on
    // irrelevant bindings (like non-Pandas imports).
    if let ExprKind::Name { id, .. } = &value.node {
        if checker.find_binding(id).map_or(true, |binding| {
            if let BindingKind::Importation(.., module) = &binding.kind {
                module != &"pandas"
            } else {
                matches!(
                    binding.kind,
                    BindingKind::Builtin
                        | BindingKind::ClassDefinition
                        | BindingKind::FunctionDefinition
                        | BindingKind::Export(..)
                        | BindingKind::FutureImportation
                        | BindingKind::StarImportation(..)
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
        .push(Diagnostic::new(violation, Range::from_located(func)));
}
