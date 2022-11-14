use num_bigint::BigInt;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind, Located};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

fn is_sys(checker: &Checker, expr: &Expr, target: &str) -> bool {
    match_module_member(
        expr,
        "sys",
        target,
        &checker.from_imports,
        &checker.import_aliases,
    )
}

/// YTT101, YTT102, YTT301, YTT303
pub fn subscript(checker: &mut Checker, value: &Expr, slice: &Expr) {
    if is_sys(checker, value, "version") {
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
                        && checker.settings.enabled.contains(&CheckCode::YTT303)
                    {
                        checker.add_check(Check::new(
                            CheckKind::SysVersionSlice1Referenced,
                            Range::from_located(value),
                        ));
                    } else if *i == BigInt::from(3)
                        && checker.settings.enabled.contains(&CheckCode::YTT101)
                    {
                        checker.add_check(Check::new(
                            CheckKind::SysVersionSlice3Referenced,
                            Range::from_located(value),
                        ));
                    }
                }
            }

            ExprKind::Constant {
                value: Constant::Int(i),
                ..
            } => {
                if *i == BigInt::from(2) && checker.settings.enabled.contains(&CheckCode::YTT102) {
                    checker.add_check(Check::new(
                        CheckKind::SysVersion2Referenced,
                        Range::from_located(value),
                    ));
                } else if *i == BigInt::from(0)
                    && checker.settings.enabled.contains(&CheckCode::YTT301)
                {
                    checker.add_check(Check::new(
                        CheckKind::SysVersion0Referenced,
                        Range::from_located(value),
                    ));
                }
            }

            _ => {}
        }
    }
}

/// YTT103, YTT201, YTT203, YTT204, YTT302
pub fn compare(checker: &mut Checker, left: &Expr, ops: &[Cmpop], comparators: &[Expr]) {
    match &left.node {
        ExprKind::Subscript { value, slice, .. } if is_sys(checker, value, "version_info") => {
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
                            && checker.settings.enabled.contains(&CheckCode::YTT201)
                        {
                            checker.add_check(Check::new(
                                CheckKind::SysVersionInfo0Eq3Referenced,
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
                        if checker.settings.enabled.contains(&CheckCode::YTT203) {
                            checker.add_check(Check::new(
                                CheckKind::SysVersionInfo1CmpInt,
                                Range::from_located(left),
                            ));
                        }
                    }
                }
            }
        }

        ExprKind::Attribute { value, attr, .. }
            if is_sys(checker, value, "version_info") && attr == "minor" =>
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
                if checker.settings.enabled.contains(&CheckCode::YTT204) {
                    checker.add_check(Check::new(
                        CheckKind::SysVersionInfoMinorCmpInt,
                        Range::from_located(left),
                    ));
                }
            }
        }

        _ => {}
    }

    if is_sys(checker, left, "version") {
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
                if checker.settings.enabled.contains(&CheckCode::YTT302) {
                    checker.add_check(Check::new(
                        CheckKind::SysVersionCmpStr10,
                        Range::from_located(left),
                    ));
                }
            } else if checker.settings.enabled.contains(&CheckCode::YTT103) {
                checker.add_check(Check::new(
                    CheckKind::SysVersionCmpStr3,
                    Range::from_located(left),
                ));
            }
        }
    }
}

/// YTT202
pub fn name_or_attribute(checker: &mut Checker, expr: &Expr) {
    if match_module_member(
        expr,
        "six",
        "PY3",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        checker.add_check(Check::new(
            CheckKind::SixPY3Referenced,
            Range::from_located(expr),
        ));
    }
}
