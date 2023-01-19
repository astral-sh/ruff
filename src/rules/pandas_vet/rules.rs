use rustpython_ast::{Constant, Expr, ExprKind, Keyword, Located};

use crate::ast::types::{BindingKind, Range};
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::violations;

/// PD002
pub fn inplace_argument(keywords: &[Keyword]) -> Option<Diagnostic> {
    for keyword in keywords {
        let arg = keyword.node.arg.as_ref()?;

        if arg == "inplace" {
            let is_true_literal = match &keyword.node.value.node {
                ExprKind::Constant {
                    value: Constant::Bool(boolean),
                    ..
                } => *boolean,
                _ => false,
            };
            if is_true_literal {
                return Some(Diagnostic::new(
                    violations::UseOfInplaceArgument,
                    Range::from_located(keyword),
                ));
            }
        }
    }
    None
}

/// PD015
pub fn use_of_pd_merge(func: &Expr) -> Option<Diagnostic> {
    if let ExprKind::Attribute { attr, value, .. } = &func.node {
        if let ExprKind::Name { id, .. } = &value.node {
            if id == "pd" && attr == "merge" {
                return Some(Diagnostic::new(
                    violations::UseOfPdMerge,
                    Range::from_located(func),
                ));
            }
        }
    }
    None
}

/// PD901
pub fn assignment_to_df(targets: &[Expr]) -> Option<Diagnostic> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    let ExprKind::Name { id, .. } = &target.node else {
        return None;
    };
    if id != "df" {
        return None;
    }
    Some(Diagnostic::new(
        violations::DfIsABadVariableName,
        Range::from_located(target),
    ))
}

pub fn check_attr(
    checker: &mut Checker,
    attr: &str,
    value: &Located<ExprKind>,
    attr_expr: &Located<ExprKind>,
) {
    let rules = &checker.settings.rules;
    let violation: DiagnosticKind = match attr {
        "ix" if rules.enabled(&Rule::UseOfDotIx) => violations::UseOfDotIx.into(),
        "at" if rules.enabled(&Rule::UseOfDotAt) => violations::UseOfDotAt.into(),
        "iat" if rules.enabled(&Rule::UseOfDotIat) => violations::UseOfDotIat.into(),
        "values" if rules.enabled(&Rule::UseOfDotValues) => violations::UseOfDotValues.into(),
        _ => return,
    };

    // Avoid flagging on function calls (e.g., `df.values()`).
    if let Some(parent) = checker.current_expr_parent() {
        if matches!(parent.node, ExprKind::Call { .. }) {
            return;
        }
    }
    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`).
    if !super::helpers::is_dataframe_candidate(value) {
        return;
    }

    // If the target is a named variable, avoid triggering on
    // irrelevant bindings (like imports).
    if let ExprKind::Name { id, .. } = &value.node {
        if checker.find_binding(id).map_or(true, |binding| {
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
        }) {
            return;
        }
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, Range::from_located(attr_expr)));
}

pub fn check_call(checker: &mut Checker, func: &Located<ExprKind>) {
    let rules = &checker.settings.rules;
    let ExprKind::Attribute { value, attr, .. } = &func.node else {return};
    let violation: DiagnosticKind = match attr.as_str() {
        "isnull" if rules.enabled(&Rule::UseOfDotIsNull) => violations::UseOfDotIsNull.into(),
        "notnull" if rules.enabled(&Rule::UseOfDotNotNull) => violations::UseOfDotNotNull.into(),
        "pivot" | "unstack" if rules.enabled(&Rule::UseOfDotPivotOrUnstack) => {
            violations::UseOfDotPivotOrUnstack.into()
        }
        "read_table" if rules.enabled(&Rule::UseOfDotReadTable) => {
            violations::UseOfDotReadTable.into()
        }
        "stack" if rules.enabled(&Rule::UseOfDotStack) => violations::UseOfDotStack.into(),
        _ => return,
    };

    if !super::helpers::is_dataframe_candidate(value) {
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
