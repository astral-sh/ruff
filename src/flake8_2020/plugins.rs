use num_bigint::BigInt;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind, Located};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn is_sys(xxxxxxxx: &xxxxxxxx, expr: &Expr, target: &str) -> bool {
    match_module_member(
        expr,
        "sys",
        target,
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    )
}

/// YTT101, YTT102, YTT301, YTT303
pub fn subscript(xxxxxxxx: &mut xxxxxxxx, value: &Expr, slice: &Expr) {
    if is_sys(xxxxxxxx, value, "version") {
        match &slice.node {
            ExprKind::Slice {
                lower: None,
                upper: Some(upper),
                step: None,
                ..
            } => {
                if let ExprKind::Constant {
                    value: Constant::Int(i),
                    ..
                } = &upper.node
                {
                    if *i == BigInt::from(1)
                        && xxxxxxxx.settings.enabled.contains(&RuleCode::YTT303)
                    {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::SysVersionSlice1Referenced,
                            Range::from_located(value),
                        ));
                    } else if *i == BigInt::from(3)
                        && xxxxxxxx.settings.enabled.contains(&RuleCode::YTT101)
                    {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::SysVersionSlice3Referenced,
                            Range::from_located(value),
                        ));
                    }
                }
            }

            ExprKind::Constant {
                value: Constant::Int(i),
                ..
            } => {
                if *i == BigInt::from(2) && xxxxxxxx.settings.enabled.contains(&RuleCode::YTT102) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::SysVersion2Referenced,
                        Range::from_located(value),
                    ));
                } else if *i == BigInt::from(0)
                    && xxxxxxxx.settings.enabled.contains(&RuleCode::YTT301)
                {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::SysVersion0Referenced,
                        Range::from_located(value),
                    ));
                }
            }

            _ => {}
        }
    }
}

/// YTT103, YTT201, YTT203, YTT204, YTT302
pub fn compare(xxxxxxxx: &mut xxxxxxxx, left: &Expr, ops: &[Cmpop], comparators: &[Expr]) {
    match &left.node {
        ExprKind::Subscript { value, slice, .. } if is_sys(xxxxxxxx, value, "version_info") => {
            if let ExprKind::Constant {
                value: Constant::Int(i),
                ..
            } = &slice.node
            {
                if *i == BigInt::from(0) {
                    if let (
                        [Cmpop::Eq | Cmpop::NotEq],
                        [Located {
                            node:
                                ExprKind::Constant {
                                    value: Constant::Int(n),
                                    ..
                                },
                            ..
                        }],
                    ) = (ops, comparators)
                    {
                        if *n == BigInt::from(3)
                            && xxxxxxxx.settings.enabled.contains(&RuleCode::YTT201)
                        {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::SysVersionInfo0Eq3Referenced,
                                Range::from_located(left),
                            ));
                        }
                    }
                } else if *i == BigInt::from(1) {
                    if let (
                        [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                        [Located {
                            node:
                                ExprKind::Constant {
                                    value: Constant::Int(_),
                                    ..
                                },
                            ..
                        }],
                    ) = (ops, comparators)
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::YTT203) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::SysVersionInfo1CmpInt,
                                Range::from_located(left),
                            ));
                        }
                    }
                }
            }
        }

        ExprKind::Attribute { value, attr, .. }
            if is_sys(xxxxxxxx, value, "version_info") && attr == "minor" =>
        {
            if let (
                [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                [Located {
                    node:
                        ExprKind::Constant {
                            value: Constant::Int(_),
                            ..
                        },
                    ..
                }],
            ) = (ops, comparators)
            {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::YTT204) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::SysVersionInfoMinorCmpInt,
                        Range::from_located(left),
                    ));
                }
            }
        }

        _ => {}
    }

    if is_sys(xxxxxxxx, left, "version") {
        if let (
            [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
            [Located {
                node:
                    ExprKind::Constant {
                        value: Constant::Str(s),
                        ..
                    },
                ..
            }],
        ) = (ops, comparators)
        {
            if s.len() == 1 {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::YTT302) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::SysVersionCmpStr10,
                        Range::from_located(left),
                    ));
                }
            } else if xxxxxxxx.settings.enabled.contains(&RuleCode::YTT103) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::SysVersionCmpStr3,
                    Range::from_located(left),
                ));
            }
        }
    }
}

/// YTT202
pub fn name_or_attribute(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    if match_module_member(
        expr,
        "six",
        "PY3",
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    ) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::SixPY3Referenced,
            Range::from_located(expr),
        ));
    }
}
