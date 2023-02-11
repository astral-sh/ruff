use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprContext, ExprKind};

use super::super::types;
use super::helpers::{is_pytest_parametrize, split_names};
use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::source_code::Generator;
use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct ParametrizeNamesWrongType {
        pub expected: types::ParametrizeNameType,
    }
);
impl AlwaysAutofixableViolation for ParametrizeNamesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ParametrizeNamesWrongType { expected } = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn autofix_title(&self) -> String {
        let ParametrizeNamesWrongType { expected } = self;
        format!("Use a `{expected}` for parameter names")
    }
}

define_violation!(
    pub struct ParametrizeValuesWrongType {
        pub values: types::ParametrizeValuesType,
        pub row: types::ParametrizeValuesRowType,
    }
);
impl Violation for ParametrizeValuesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ParametrizeValuesWrongType { values, row } = self;
        format!("Wrong values type in `@pytest.mark.parametrize` expected `{values}` of `{row}`")
    }
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

    Some(unparse_expr(
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
        checker.stylist,
    ))
}

/// PT006
fn check_names(checker: &mut Checker, expr: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => {
            let names = split_names(string);
            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let mut generator: Generator = checker.stylist.into();
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
                            diagnostic.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::List => {
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::replacement(
                                unparse_expr(
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
                                    checker.stylist,
                                ),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {}
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
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::replacement(
                                unparse_expr(
                                    &create_expr(ExprKind::List {
                                        elts: elts.clone(),
                                        ctx: ExprContext::Load,
                                    }),
                                    checker.stylist,
                                ),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                diagnostic.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
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
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let mut generator: Generator = checker.stylist.into();
                            generator.unparse_expr(
                                &create_expr(ExprKind::Tuple {
                                    elts: elts.clone(),
                                    ctx: ExprContext::Load,
                                }),
                                1, // so tuple is generated with parentheses
                            );
                            diagnostic.amend(Fix::replacement(
                                generator.generate(),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            ParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                diagnostic.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            };
        }
        _ => {}
    }
}

/// PT007
fn check_values(checker: &mut Checker, names: &Expr, values: &Expr) {
    let values_type = checker.settings.flake8_pytest_style.parametrize_values_type;

    let values_row_type = checker
        .settings
        .flake8_pytest_style
        .parametrize_values_row_type;

    let is_multi_named = if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &names.node
    {
        split_names(string).len() > 1
    } else {
        true
    };

    match &values.node {
        ExprKind::List { elts, .. } => {
            if values_type != types::ParametrizeValuesType::List {
                checker.diagnostics.push(Diagnostic::new(
                    ParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    Range::from_located(values),
                ));
            }
            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        ExprKind::Tuple { elts, .. } => {
            if values_type != types::ParametrizeValuesType::Tuple {
                checker.diagnostics.push(Diagnostic::new(
                    ParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    Range::from_located(values),
                ));
            }
            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        _ => {}
    }
}

fn handle_single_name(checker: &mut Checker, expr: &Expr, value: &Expr) {
    let mut diagnostic = Diagnostic::new(
        ParametrizeNamesWrongType {
            expected: types::ParametrizeNameType::Csv,
        },
        Range::from_located(expr),
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            unparse_expr(&create_expr(value.node.clone()), checker.stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
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
                    checker.diagnostics.push(Diagnostic::new(
                        ParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        Range::from_located(elt),
                    ));
                }
            }
            ExprKind::List { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    checker.diagnostics.push(Diagnostic::new(
                        ParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        Range::from_located(elt),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub fn parametrize(checker: &mut Checker, decorators: &[Expr]) {
    for decorator in decorators {
        if is_pytest_parametrize(decorator, checker) {
            if let ExprKind::Call { args, .. } = &decorator.node {
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::ParametrizeNamesWrongType)
                {
                    if let Some(names) = args.get(0) {
                        check_names(checker, names);
                    }
                }
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::ParametrizeValuesWrongType)
                {
                    if let Some(names) = args.get(0) {
                        if let Some(values) = args.get(1) {
                            check_values(checker, names, values);
                        }
                    }
                }
            }
        }
    }
}
