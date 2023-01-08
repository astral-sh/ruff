use rustpython_ast::{Constant, Expr, ExprContext, ExprKind};

use super::helpers::is_pytest_parametrize;
use crate::ast::helpers::create_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::flake8_pytest_style::types;
use crate::registry::{Diagnostic, RuleCode};
use crate::source_code_generator::SourceCodeGenerator;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn get_parametrize_decorator<'a>(xxxxxxxx: &xxxxxxxx, decorators: &'a [Expr]) -> Option<&'a Expr> {
    decorators
        .iter()
        .find(|decorator| is_pytest_parametrize(decorator, xxxxxxxx))
}

fn elts_to_csv(elts: &[Expr], xxxxxxxx: &xxxxxxxx) -> Option<String> {
    let all_literals = elts.iter().all(|e| {
        matches!(
            e.node,
            ExprKind::Constant {
                value: Constant::Str(_),
                ..
            }
        )
    });

    if !all_literals {
        return None;
    }

    let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
    generator.unparse_expr(
        &create_expr(ExprKind::Constant {
            value: Constant::Str(elts.iter().fold(String::new(), |mut acc, elt| {
                if let ExprKind::Constant {
                    value: Constant::Str(ref s),
                    ..
                } = elt.node
                {
                    if !acc.is_empty() {
                        acc.push(',');
                    }
                    acc.push_str(s);
                }
                acc
            })),
            kind: None,
        }),
        0,
    );
    Some(generator.generate())
}

/// PT006
fn check_names(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    let names_type = xxxxxxxx.settings.flake8_pytest_style.parametrize_names_type;

    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => {
            // Match the following pytest code:
            //    [x.strip() for x in argnames.split(",") if x.strip()]
            let names = string
                .split(',')
                .filter_map(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                })
                .collect::<Vec<&str>>();

            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
                            generator.unparse_expr(
                                &create_expr(ExprKind::Tuple {
                                    elts: names
                                        .iter()
                                        .map(|&name| {
                                            create_expr(ExprKind::Constant {
                                                value: Constant::Str(name.to_string()),
                                                kind: None,
                                            })
                                        })
                                        .collect(),
                                    ctx: ExprContext::Load,
                                }),
                                1,
                            );
                            check.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                    types::ParametrizeNameType::List => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
                            generator.unparse_expr(
                                &create_expr(ExprKind::List {
                                    elts: names
                                        .iter()
                                        .map(|&name| {
                                            create_expr(ExprKind::Constant {
                                                value: Constant::Str(name.to_string()),
                                                kind: None,
                                            })
                                        })
                                        .collect(),
                                    ctx: ExprContext::Load,
                                }),
                                0,
                            );
                            check.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                    types::ParametrizeNameType::CSV => {}
                }
            }
        }
        ExprKind::Tuple { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(xxxxxxxx, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::Tuple => {}
                    types::ParametrizeNameType::List => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
                            generator.unparse_expr(
                                &create_expr(ExprKind::List {
                                    elts: elts.clone(),
                                    ctx: ExprContext::Load,
                                }),
                                0,
                            );
                            check.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                    types::ParametrizeNameType::CSV => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            if let Some(content) = elts_to_csv(elts, xxxxxxxx) {
                                check.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                }
            };
        }
        ExprKind::List { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(xxxxxxxx, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::List => {}
                    types::ParametrizeNameType::Tuple => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
                            generator.unparse_expr(
                                &create_expr(ExprKind::Tuple {
                                    elts: elts.clone(),
                                    ctx: ExprContext::Load,
                                }),
                                1, // so tuple is generated with parentheses
                            );
                            check.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                    types::ParametrizeNameType::CSV => {
                        let mut check = Diagnostic::new(
                            violations::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            if let Some(content) = elts_to_csv(elts, xxxxxxxx) {
                                check.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                }
            };
        }
        _ => {}
    }
}

/// PT007
fn check_values(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    let values_type = xxxxxxxx
        .settings
        .flake8_pytest_style
        .parametrize_values_type;

    let values_row_type = xxxxxxxx
        .settings
        .flake8_pytest_style
        .parametrize_values_row_type;

    match &expr.node {
        ExprKind::List { elts, .. } => {
            if values_type != types::ParametrizeValuesType::List {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::ParametrizeValuesWrongType(values_type, values_row_type),
                    Range::from_located(expr),
                ));
            }
            handle_value_rows(xxxxxxxx, elts, values_type, values_row_type);
        }
        ExprKind::Tuple { elts, .. } => {
            if values_type != types::ParametrizeValuesType::Tuple {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::ParametrizeValuesWrongType(values_type, values_row_type),
                    Range::from_located(expr),
                ));
            }
            handle_value_rows(xxxxxxxx, elts, values_type, values_row_type);
        }
        _ => {}
    }
}

fn handle_single_name(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, value: &Expr) {
    let mut check = Diagnostic::new(
        violations::ParametrizeNamesWrongType(types::ParametrizeNameType::CSV),
        Range::from_located(expr),
    );

    if xxxxxxxx.patch(check.kind.code()) {
        let mut generator: SourceCodeGenerator = xxxxxxxx.style.into();
        generator.unparse_expr(&create_expr(value.node.clone()), 0);
        check.amend(Fix::replacement(
            generator.generate(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

fn handle_value_rows(
    xxxxxxxx: &mut xxxxxxxx,
    elts: &[Expr],
    values_type: types::ParametrizeValuesType,
    values_row_type: types::ParametrizeValuesRowType,
) {
    for elt in elts {
        match &elt.node {
            ExprKind::Tuple { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::Tuple {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::ParametrizeValuesWrongType(values_type, values_row_type),
                        Range::from_located(elt),
                    ));
                }
            }
            ExprKind::List { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::ParametrizeValuesWrongType(values_type, values_row_type),
                        Range::from_located(elt),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub fn parametrize(xxxxxxxx: &mut xxxxxxxx, decorators: &[Expr]) {
    let decorator = get_parametrize_decorator(xxxxxxxx, decorators);
    if let Some(decorator) = decorator {
        if let ExprKind::Call { args, .. } = &decorator.node {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT006) {
                if let Some(arg) = args.get(0) {
                    check_names(xxxxxxxx, arg);
                }
            }
            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT007) {
                if let Some(arg) = args.get(1) {
                    check_values(xxxxxxxx, arg);
                }
            }
        }
    }
}
