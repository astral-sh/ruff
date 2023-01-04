use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind};

use super::helpers::is_pytest_parametrize;
use crate::ast::helpers::create_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::flake8_pytest_style::types;
use crate::registry::{Check, CheckCode, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

fn get_parametrize_decorator<'a>(checker: &Checker, decorators: &'a [Expr]) -> Option<&'a Expr> {
    decorators
        .iter()
        .find(|decorator| is_pytest_parametrize(decorator, checker))
}

fn elts_to_csv(elts: &[Expr], checker: &Checker) -> Option<String> {
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

    let mut generator = SourceCodeGenerator::new(
        checker.style.indentation(),
        checker.style.quote(),
        checker.style.line_ending(),
    );

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

    match generator.generate() {
        Ok(s) => Some(s),
        Err(e) => {
            error!(
                "Failed to generate CSV string from sequence of names: {}",
                e
            );
            None
        }
    }
}

/// PT006
fn check_names(checker: &mut Checker, expr: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

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
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            let mut generator = SourceCodeGenerator::new(
                                checker.style.indentation(),
                                checker.style.quote(),
                                checker.style.line_ending(),
                            );
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
                            match generator.generate() {
                                Ok(content) => {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                                Err(e) => error!(
                                    "Failed to fix wrong name(s) type in \
                                     `@pytest.mark.parametrize`: {e}"
                                ),
                            };
                        }
                        checker.add_check(check);
                    }
                    types::ParametrizeNameType::List => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            let mut generator = SourceCodeGenerator::new(
                                checker.style.indentation(),
                                checker.style.quote(),
                                checker.style.line_ending(),
                            );
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
                            match generator.generate() {
                                Ok(content) => {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                                Err(e) => error!(
                                    "Failed to fix wrong name(s) type in \
                                     `@pytest.mark.parametrize`: {e}"
                                ),
                            };
                        }
                        checker.add_check(check);
                    }
                    types::ParametrizeNameType::CSV => {}
                }
            }
        }
        ExprKind::Tuple { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(checker, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::Tuple => {}
                    types::ParametrizeNameType::List => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            let mut generator = SourceCodeGenerator::new(
                                checker.style.indentation(),
                                checker.style.quote(),
                                checker.style.line_ending(),
                            );
                            generator.unparse_expr(
                                &create_expr(ExprKind::List {
                                    elts: elts.clone(),
                                    ctx: ExprContext::Load,
                                }),
                                0,
                            );
                            match generator.generate() {
                                Ok(content) => {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                                Err(e) => error!(
                                    "Failed to fix wrong name(s) type in \
                                     `@pytest.mark.parametrize`: {e}"
                                ),
                            };
                        }
                        checker.add_check(check);
                    }
                    types::ParametrizeNameType::CSV => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                check.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.add_check(check);
                    }
                }
            };
        }
        ExprKind::List { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(checker, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::List => {}
                    types::ParametrizeNameType::Tuple => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            let mut generator = SourceCodeGenerator::new(
                                checker.style.indentation(),
                                checker.style.quote(),
                                checker.style.line_ending(),
                            );
                            generator.unparse_expr(
                                &create_expr(ExprKind::Tuple {
                                    elts: elts.clone(),
                                    ctx: ExprContext::Load,
                                }),
                                1, // so tuple is generated with parentheses
                            );
                            match generator.generate() {
                                Ok(content) => {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                                Err(e) => error!(
                                    "Failed to fix wrong name(s) type in \
                                     `@pytest.mark.parametrize`: {e}"
                                ),
                            };
                        }
                        checker.add_check(check);
                    }
                    types::ParametrizeNameType::CSV => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                check.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.add_check(check);
                    }
                }
            };
        }
        _ => {}
    }
}

/// PT007
fn check_values(checker: &mut Checker, expr: &Expr) {
    let values_type = checker.settings.flake8_pytest_style.parametrize_values_type;

    let values_row_type = checker
        .settings
        .flake8_pytest_style
        .parametrize_values_row_type;

    match &expr.node {
        ExprKind::List { elts, .. } => {
            if values_type != types::ParametrizeValuesType::List {
                checker.add_check(Check::new(
                    CheckKind::ParametrizeValuesWrongType(values_type, values_row_type),
                    Range::from_located(expr),
                ));
            }
            handle_value_rows(checker, elts, values_type, values_row_type);
        }
        ExprKind::Tuple { elts, .. } => {
            if values_type != types::ParametrizeValuesType::Tuple {
                checker.add_check(Check::new(
                    CheckKind::ParametrizeValuesWrongType(values_type, values_row_type),
                    Range::from_located(expr),
                ));
            }
            handle_value_rows(checker, elts, values_type, values_row_type);
        }
        _ => {}
    }
}

fn handle_single_name(checker: &mut Checker, expr: &Expr, value: &Expr) {
    let mut check = Check::new(
        CheckKind::ParametrizeNamesWrongType(types::ParametrizeNameType::CSV),
        Range::from_located(expr),
    );

    if checker.patch(check.kind.code()) {
        let mut generator = SourceCodeGenerator::new(
            checker.style.indentation(),
            checker.style.quote(),
            checker.style.line_ending(),
        );
        generator.unparse_expr(&create_expr(value.node.clone()), 0);
        match generator.generate() {
            Ok(content) => {
                check.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            Err(e) => error!("Failed to fix wrong name(s) type in `@pytest.mark.parametrize`: {e}"),
        };
    }
    checker.add_check(check);
}

fn handle_value_rows(
    checker: &mut Checker,
    elts: &[Expr],
    values_type: types::ParametrizeValuesType,
    values_row_type: types::ParametrizeValuesRowType,
) {
    for elt in elts {
        match &elt.node {
            ExprKind::Tuple { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::Tuple {
                    checker.add_check(Check::new(
                        CheckKind::ParametrizeValuesWrongType(values_type, values_row_type),
                        Range::from_located(elt),
                    ));
                }
            }
            ExprKind::List { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    checker.add_check(Check::new(
                        CheckKind::ParametrizeValuesWrongType(values_type, values_row_type),
                        Range::from_located(elt),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub fn parametrize(checker: &mut Checker, decorators: &[Expr]) {
    let decorator = get_parametrize_decorator(checker, decorators);
    if let Some(decorator) = decorator {
        if let ExprKind::Call { args, .. } = &decorator.node {
            if checker.settings.enabled.contains(&CheckCode::PT006) {
                if let Some(arg) = args.get(0) {
                    check_names(checker, arg);
                }
            }
            if checker.settings.enabled.contains(&CheckCode::PT007) {
                if let Some(arg) = args.get(1) {
                    check_values(checker, arg);
                }
            }
        }
    }
}
