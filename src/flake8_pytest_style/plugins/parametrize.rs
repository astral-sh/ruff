use rustpython_ast::{Constant, Expr, ExprKind};

use super::helpers::is_pytest_parametrize;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::flake8_pytest_style::types;
use crate::registry::{Check, CheckCode, CheckKind};

fn get_parametrize_decorator<'a>(checker: &Checker, decorators: &'a [Expr]) -> Option<&'a Expr> {
    decorators
        .iter()
        .find(|decorator| is_pytest_parametrize(decorator, checker))
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
                            check.amend(Fix::replacement(
                                strings_to_python_tuple(&names),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.add_check(check);
                    }
                    types::ParametrizeNameType::List => {
                        let mut check = Check::new(
                            CheckKind::ParametrizeNamesWrongType(names_type),
                            Range::from_located(expr),
                        );
                        if checker.patch(check.kind.code()) {
                            check.amend(Fix::replacement(
                                strings_to_python_list(&names),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
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
            } else if names_type != types::ParametrizeNameType::Tuple {
                checker.add_check(Check::new(
                    CheckKind::ParametrizeNamesWrongType(names_type),
                    Range::from_located(expr),
                ));
            };
        }
        ExprKind::List { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(checker, expr, first);
                }
            } else if names_type != types::ParametrizeNameType::List {
                checker.add_check(Check::new(
                    CheckKind::ParametrizeNamesWrongType(names_type),
                    Range::from_located(expr),
                ));
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
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &value.node
    {
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                format!("\"{string}\""),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.add_check(check);
}

fn strings_to_python_tuple(strings: &[&str]) -> String {
    let result = strings
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<String>>()
        .join(", ");

    format!("({result})")
}

fn strings_to_python_list(strings: &[&str]) -> String {
    let result = strings
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<String>>()
        .join(", ");

    format!("[{result}]")
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
                let first = args.first().unwrap();
                check_names(checker, first);
            }

            if checker.settings.enabled.contains(&CheckCode::PT007) {
                let second = args.get(1).unwrap();
                check_values(checker, second);
            }
        }
    }
}
