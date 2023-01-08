use log::error;
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::ast::{cast, helpers, visitor};
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::flake8_annotations::fixes;
use crate::flake8_annotations::helpers::match_function_def;
use crate::registry::RuleCode;
use crate::visibility::Visibility;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, visibility, Diagnostic};

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
fn check_dynamically_typed<F>(xxxxxxxx: &mut xxxxxxxx, annotation: &Expr, func: F)
where
    F: FnOnce() -> String,
{
    if xxxxxxxx.match_typing_expr(annotation, "Any") {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::DynamicallyTypedExpression(func()),
            Range::from_located(annotation),
        ));
    };
}

/// Generate flake8-annotation checks for a given `Definition`.
pub fn definition(xxxxxxxx: &mut xxxxxxxx, definition: &Definition, visibility: &Visibility) {
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
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                        check_dynamically_typed(xxxxxxxx, expr, || arg.node.arg.to_string());
                    };
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN001) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeFunctionArgument(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN002, ANN401
            if let Some(arg) = &args.vararg {
                if let Some(expr) = &arg.node.annotation {
                    if !xxxxxxxx.settings.flake8_annotations.allow_star_arg_any {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(xxxxxxxx, expr, || format!("*{name}"));
                        }
                    }
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN002) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeArgs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN003, ANN401
            if let Some(arg) = &args.kwarg {
                if let Some(expr) = &arg.node.annotation {
                    if !xxxxxxxx.settings.flake8_annotations.allow_star_arg_any {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(xxxxxxxx, expr, || format!("**{name}"));
                        }
                    }
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN003) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeKwargs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN201, ANN202, ANN401
            if let Some(expr) = &returns {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                    check_dynamically_typed(xxxxxxxx, expr, || name.to_string());
                };
            } else {
                // Allow omission of return annotation in `__init__` functions, if the function
                // only returns `None` (explicitly or implicitly).
                if xxxxxxxx.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(body)
                {
                    return;
                }

                match visibility {
                    Visibility::Public => {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN201) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingReturnTypePublicFunction(name.to_string()),
                                helpers::identifier_range(stmt, xxxxxxxx.locator),
                            ));
                        }
                    }
                    Visibility::Private => {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN202) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingReturnTypePrivateFunction(name.to_string()),
                                helpers::identifier_range(stmt, xxxxxxxx.locator),
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
                        xxxxxxxx,
                        cast::decorator_list(stmt),
                    )),
                )
            {
                // ANN401 for dynamically typed arguments
                if let Some(annotation) = &arg.node.annotation {
                    has_any_typed_arg = true;
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                        check_dynamically_typed(xxxxxxxx, annotation, || arg.node.arg.to_string());
                    }
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN001) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeFunctionArgument(arg.node.arg.to_string()),
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
                    if !xxxxxxxx.settings.flake8_annotations.allow_star_arg_any {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(xxxxxxxx, expr, || format!("*{name}"));
                        }
                    }
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN002) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeArgs(arg.node.arg.to_string()),
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
                    if !xxxxxxxx.settings.flake8_annotations.allow_star_arg_any {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                            let name = arg.node.arg.to_string();
                            check_dynamically_typed(xxxxxxxx, expr, || format!("**{name}"));
                        }
                    }
                } else {
                    if !(xxxxxxxx.settings.flake8_annotations.suppress_dummy_args
                        && xxxxxxxx.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN003) {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeKwargs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN101, ANN102
            if !visibility::is_staticmethod(xxxxxxxx, cast::decorator_list(stmt)) {
                if let Some(arg) = args.args.first() {
                    if arg.node.annotation.is_none() {
                        if visibility::is_classmethod(xxxxxxxx, cast::decorator_list(stmt)) {
                            if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN102) {
                                xxxxxxxx.diagnostics.push(Diagnostic::new(
                                    violations::MissingTypeCls(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        } else {
                            if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN101) {
                                xxxxxxxx.diagnostics.push(Diagnostic::new(
                                    violations::MissingTypeSelf(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        }
                    }
                }
            }

            // ANN201, ANN202
            if let Some(expr) = &returns {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN401) {
                    check_dynamically_typed(xxxxxxxx, expr, || name.to_string());
                }
            } else {
                // Allow omission of return annotation if the function only returns `None`
                // (explicitly or implicitly).
                if xxxxxxxx.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(body)
                {
                    return;
                }

                if visibility::is_classmethod(xxxxxxxx, cast::decorator_list(stmt)) {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN206) {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeClassMethod(name.to_string()),
                            helpers::identifier_range(stmt, xxxxxxxx.locator),
                        ));
                    }
                } else if visibility::is_staticmethod(xxxxxxxx, cast::decorator_list(stmt)) {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN205) {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeStaticMethod(name.to_string()),
                            helpers::identifier_range(stmt, xxxxxxxx.locator),
                        ));
                    }
                } else if visibility::is_init(stmt) {
                    // Allow omission of return annotation in `__init__` functions, as long as at
                    // least one argument is typed.
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN204) {
                        if !(xxxxxxxx.settings.flake8_annotations.mypy_init_return
                            && has_any_typed_arg)
                        {
                            let mut check = Diagnostic::new(
                                violations::MissingReturnTypeSpecialMethod(name.to_string()),
                                helpers::identifier_range(stmt, xxxxxxxx.locator),
                            );
                            if xxxxxxxx.patch(check.kind.code()) {
                                match fixes::add_return_none_annotation(xxxxxxxx.locator, stmt) {
                                    Ok(fix) => {
                                        check.amend(fix);
                                    }
                                    Err(e) => error!("Failed to generate fix: {e}"),
                                }
                            }
                            xxxxxxxx.diagnostics.push(check);
                        }
                    }
                } else if visibility::is_magic(stmt) {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN204) {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeSpecialMethod(name.to_string()),
                            helpers::identifier_range(stmt, xxxxxxxx.locator),
                        ));
                    }
                } else {
                    match visibility {
                        Visibility::Public => {
                            if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN201) {
                                xxxxxxxx.diagnostics.push(Diagnostic::new(
                                    violations::MissingReturnTypePublicFunction(name.to_string()),
                                    helpers::identifier_range(stmt, xxxxxxxx.locator),
                                ));
                            }
                        }
                        Visibility::Private => {
                            if xxxxxxxx.settings.enabled.contains(&RuleCode::ANN202) {
                                xxxxxxxx.diagnostics.push(Diagnostic::new(
                                    violations::MissingReturnTypePrivateFunction(name.to_string()),
                                    helpers::identifier_range(stmt, xxxxxxxx.locator),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
