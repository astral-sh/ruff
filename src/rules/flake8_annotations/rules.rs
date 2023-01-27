use log::error;
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use super::fixes;
use super::helpers::match_function_def;
use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::ast::{cast, helpers, visitor};
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::registry::{Diagnostic, Rule};
use crate::visibility::Visibility;
use crate::{violations, visibility};

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
        checker.diagnostics.push(Diagnostic::new(
            violations::DynamicallyTypedExpression(func()),
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
        DefinitionKind::Function(stmt)
        | DefinitionKind::NestedFunction(stmt)
        | DefinitionKind::Method(stmt) => {
            let is_method = matches!(definition.kind, DefinitionKind::Method(_));
            let (name, args, returns, body) = match_function_def(stmt);
            let mut has_any_typed_arg = false;

            // ANN001, ANN401
            for arg in args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
                .skip(
                    // If this is a non-static method, skip `cls` or `self`.
                    usize::from(
                        is_method
                            && !visibility::is_staticmethod(checker, cast::decorator_list(stmt)),
                    ),
                )
            {
                // ANN401 for dynamically typed arguments
                if let Some(annotation) = &arg.node.annotation {
                    has_any_typed_arg = true;
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::DynamicallyTypedExpression)
                    {
                        check_dynamically_typed(checker, annotation, || arg.node.arg.to_string());
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker
                            .settings
                            .rules
                            .enabled(&Rule::MissingTypeFunctionArgument)
                        {
                            checker.diagnostics.push(Diagnostic::new(
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
                    has_any_typed_arg = true;
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker
                            .settings
                            .rules
                            .enabled(&Rule::DynamicallyTypedExpression)
                        {
                            let name = &arg.node.arg;
                            check_dynamically_typed(checker, expr, || format!("*{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.rules.enabled(&Rule::MissingTypeArgs) {
                            checker.diagnostics.push(Diagnostic::new(
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
                    has_any_typed_arg = true;
                    if !checker.settings.flake8_annotations.allow_star_arg_any {
                        if checker
                            .settings
                            .rules
                            .enabled(&Rule::DynamicallyTypedExpression)
                        {
                            let name = &arg.node.arg;
                            check_dynamically_typed(checker, expr, || format!("**{name}"));
                        }
                    }
                } else {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.rules.enabled(&Rule::MissingTypeKwargs) {
                            checker.diagnostics.push(Diagnostic::new(
                                violations::MissingTypeKwargs(arg.node.arg.to_string()),
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN101, ANN102
            if is_method && !visibility::is_staticmethod(checker, cast::decorator_list(stmt)) {
                if let Some(arg) = args.args.first() {
                    if arg.node.annotation.is_none() {
                        if visibility::is_classmethod(checker, cast::decorator_list(stmt)) {
                            if checker.settings.rules.enabled(&Rule::MissingTypeCls) {
                                checker.diagnostics.push(Diagnostic::new(
                                    violations::MissingTypeCls(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        } else {
                            if checker.settings.rules.enabled(&Rule::MissingTypeSelf) {
                                checker.diagnostics.push(Diagnostic::new(
                                    violations::MissingTypeSelf(arg.node.arg.to_string()),
                                    Range::from_located(arg),
                                ));
                            }
                        }
                    }
                }
            }

            // ANN201, ANN202, ANN401
            if let Some(expr) = &returns {
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::DynamicallyTypedExpression)
                {
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

                if is_method && visibility::is_classmethod(checker, cast::decorator_list(stmt)) {
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::MissingReturnTypeClassMethod)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeClassMethod(name.to_string()),
                            helpers::identifier_range(stmt, checker.locator),
                        ));
                    }
                } else if is_method
                    && visibility::is_staticmethod(checker, cast::decorator_list(stmt))
                {
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::MissingReturnTypeStaticMethod)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeStaticMethod(name.to_string()),
                            helpers::identifier_range(stmt, checker.locator),
                        ));
                    }
                } else if is_method && visibility::is_init(cast::name(stmt)) {
                    // Allow omission of return annotation in `__init__` functions, as long as at
                    // least one argument is typed.
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::MissingReturnTypeSpecialMethod)
                    {
                        if !(checker.settings.flake8_annotations.mypy_init_return
                            && has_any_typed_arg)
                        {
                            let mut diagnostic = Diagnostic::new(
                                violations::MissingReturnTypeSpecialMethod(name.to_string()),
                                helpers::identifier_range(stmt, checker.locator),
                            );
                            if checker.patch(diagnostic.kind.rule()) {
                                match fixes::add_return_none_annotation(checker.locator, stmt) {
                                    Ok(fix) => {
                                        diagnostic.amend(fix);
                                    }
                                    Err(e) => error!("Failed to generate fix: {e}"),
                                }
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                } else if is_method && visibility::is_magic(cast::name(stmt)) {
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::MissingReturnTypeSpecialMethod)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::MissingReturnTypeSpecialMethod(name.to_string()),
                            helpers::identifier_range(stmt, checker.locator),
                        ));
                    }
                } else {
                    match visibility {
                        Visibility::Public => {
                            if checker
                                .settings
                                .rules
                                .enabled(&Rule::MissingReturnTypePublicFunction)
                            {
                                checker.diagnostics.push(Diagnostic::new(
                                    violations::MissingReturnTypePublicFunction(name.to_string()),
                                    helpers::identifier_range(stmt, checker.locator),
                                ));
                            }
                        }
                        Visibility::Private => {
                            if checker
                                .settings
                                .rules
                                .enabled(&Rule::MissingReturnTypePrivateFunction)
                            {
                                checker.diagnostics.push(Diagnostic::new(
                                    violations::MissingReturnTypePrivateFunction(name.to_string()),
                                    helpers::identifier_range(stmt, checker.locator),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
