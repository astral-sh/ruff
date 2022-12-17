use log::error;
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::ast::{cast, visitor};
use crate::checkers::ast::Checker;
use crate::checks::{CheckCode, CheckKind};
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::flake8_annotations::fixes;
use crate::flake8_annotations::helpers::match_function_def;
use crate::visibility::Visibility;
use crate::{visibility, Check};

#[derive(Default)]
struct ReturnStatementVisitor<'a> {
    returns: Vec<Option<&'a Expr>>,
}

impl<'a, 'b> Visitor<'b> for ReturnStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                // Don't recurse.
            }
            StmtKind::Return { value } => self.returns.push(value.as_ref().map(|expr| &**expr)),
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

fn is_none_returning(body: &[Stmt]) -> bool {
    let mut visitor = ReturnStatementVisitor::default();
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    for expr in visitor.returns.into_iter().flatten() {
        if !matches!(
            expr.node,
            ExprKind::Constant {
                value: Constant::None,
                ..
            }
        ) {
            return false;
        }
    }
    true
}

/// ANN401
fn check_dynamically_typed<F>(checker: &mut Checker, annotation: &Expr, func: F)
where
    F: FnOnce() -> String,
{
    if checker.match_typing_expr(annotation, "Any") {
        checker.add_check(Check::new(
            CheckKind::DynamicallyTypedExpression(func()),
            Range::from_located(annotation),
        ));
    };
}

/// Generate flake8-annotation checks for a given `Definition`.
pub fn definition(checker: &mut Checker, definition: &Definition, visibility: &Visibility) {
    // TODO(charlie): Consider using the AST directly here rather than `Definition`.
    // We could adhere more closely to `flake8-annotations` by defining public
    // vs. secret vs. protected.
    match &definition.kind {
        DefinitionKind::Module => {}
        DefinitionKind::Package => {}
        DefinitionKind::Class(_) => {}
        DefinitionKind::NestedClass(_) => {}
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            let (name, args, returns, body) = match_function_def(stmt);

            // ANN001, ANN401
            for arg in args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
            {
                if let Some(expr) = &arg.node.annotation {
                    if checker.settings.enabled.contains(&CheckCode::ANN401) {
                        check_dynamically_typed(checker, expr, || arg.node.arg.to_string());
                    };
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN001) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeFunctionArgument(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN002, ANN401
            if let Some(arg) = &args.vararg {
                if let Some(expr) = &arg.node.annotation {
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker.settings.enabled.contains(&CheckCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(checker, expr, || format!("*{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN002) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeArgs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN003, ANN401
            if let Some(arg) = &args.kwarg {
                if let Some(expr) = &arg.node.annotation {
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker.settings.enabled.contains(&CheckCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(checker, expr, || format!("**{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN003) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeKwargs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN201, ANN202, ANN401
            if let Some(expr) = &returns {
                if checker.settings.enabled.contains(&CheckCode::ANN401) {
                    check_dynamically_typed(checker, expr, || name.to_string());
                };
            } else {
                // Allow omission of return annotation in `__init__` functions, if the function
                // only returns `None` (explicitly or implicitly).
                if checker.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(body)
                {
                    return;
                }

                match visibility {
                    Visibility::Public => {
                        if checker.settings.enabled.contains(&CheckCode::ANN201) {
                            checker.add_check(Check::new(
                                CheckKind::MissingReturnTypePublicFunction(name.to_string()),
                                Range::from_located(stmt),
                            ));
                        }
                    }
                    Visibility::Private => {
                        if checker.settings.enabled.contains(&CheckCode::ANN202) {
                            checker.add_check(Check::new(
                                CheckKind::MissingReturnTypePrivateFunction(name.to_string()),
                                Range::from_located(stmt),
                            ));
                        }
                    }
                }
            }
        }
        DefinitionKind::Method(stmt) => {
            let (name, args, returns, body) = match_function_def(stmt);
            let mut has_any_typed_arg = false;

            // ANN001
            for arg in args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
                .skip(
                    // If this is a non-static method, skip `cls` or `self`.
                    usize::from(!visibility::is_staticmethod(
                        checker,
                        cast::decorator_list(stmt),
                    )),
                )
            {
                // ANN401 for dynamically typed arguments
                if let Some(annotation) = &arg.node.annotation {
                    has_any_typed_arg = true;
                    if checker.settings.enabled.contains(&CheckCode::ANN401) {
                        check_dynamically_typed(checker, annotation, || arg.node.arg.to_string());
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN001) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeFunctionArgument(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN002, ANN401
            if let Some(arg) = &args.vararg {
                has_any_typed_arg = true;
                if let Some(expr) = &arg.node.annotation {
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker.settings.enabled.contains(&CheckCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(checker, expr, || format!("*{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN002) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeArgs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN003, ANN401
            if let Some(arg) = &args.kwarg {
                has_any_typed_arg = true;
                if let Some(expr) = &arg.node.annotation {
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker.settings.enabled.contains(&CheckCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(checker, expr, || format!("**{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN003) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeKwargs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN101, ANN102
            if !visibility::is_staticmethod(checker, cast::decorator_list(stmt)) {
                if let Some(arg) = args.args.first() {
                    if arg.node.annotation.is_none() {
                        if visibility::is_classmethod(checker, cast::decorator_list(stmt)) {
                            if checker.settings.enabled.contains(&CheckCode::ANN102) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingTypeCls(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        } else {
                            if checker.settings.enabled.contains(&CheckCode::ANN101) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingTypeSelf(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        }
                    }
                }
            }

            // ANN201, ANN202
            if let Some(expr) = &returns {
                if checker.settings.enabled.contains(&CheckCode::ANN401) {
                    check_dynamically_typed(checker, expr, || name.to_string());
                }
            } else {
                // Allow omission of return annotation if the function only returns `None`
                // (explicitly or implicitly).
                if checker.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(body)
                {
                    return;
                }

                if visibility::is_classmethod(checker, cast::decorator_list(stmt)) {
                    if checker.settings.enabled.contains(&CheckCode::ANN206) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeClassMethod(name.to_string()),
                            Range::from_located(stmt),
                        ));
                    }
                } else if visibility::is_staticmethod(checker, cast::decorator_list(stmt)) {
                    if checker.settings.enabled.contains(&CheckCode::ANN205) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeStaticMethod(name.to_string()),
                            Range::from_located(stmt),
                        ));
                    }
                } else if visibility::is_init(stmt) {
                    // Allow omission of return annotation in `__init__` functions, as long as at
                    // least one argument is typed.
                    if checker.settings.enabled.contains(&CheckCode::ANN204) {
                        if !(checker.settings.flake8_annotations.mypy_init_return
                            && has_any_typed_arg)
                        {
                            let mut check = Check::new(
                                CheckKind::MissingReturnTypeSpecialMethod(name.to_string()),
                                Range::from_located(stmt),
                            );
                            if checker.patch(check.kind.code()) {
                                match fixes::add_return_none_annotation(checker.locator, stmt) {
                                    Ok(fix) => check.amend(fix),
                                    Err(e) => error!("Failed to generate fix: {e}"),
                                }
                            }
                            checker.add_check(check);
                        }
                    }
                } else if visibility::is_magic(stmt) {
                    if checker.settings.enabled.contains(&CheckCode::ANN204) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeSpecialMethod(name.to_string()),
                            Range::from_located(stmt),
                        ));
                    }
                } else {
                    match visibility {
                        Visibility::Public => {
                            if checker.settings.enabled.contains(&CheckCode::ANN201) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingReturnTypePublicFunction(name.to_string()),
                                    Range::from_located(stmt),
                                ));
                            }
                        }
                        Visibility::Private => {
                            if checker.settings.enabled.contains(&CheckCode::ANN202) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingReturnTypePrivateFunction(name.to_string()),
                                    Range::from_located(stmt),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
