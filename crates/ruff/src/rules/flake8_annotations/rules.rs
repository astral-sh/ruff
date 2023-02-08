use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt};

use super::fixes;
use super::helpers::match_function_def;
use crate::ast::helpers::ReturnStatementVisitor;
use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::ast::{cast, helpers};
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::registry::{Diagnostic, Rule};
use crate::violation::{AlwaysAutofixableViolation, Violation};
use crate::visibility;
use crate::visibility::Visibility;

define_violation!(
    pub struct MissingTypeFunctionArgument {
        pub name: String,
    }
);
impl Violation for MissingTypeFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeFunctionArgument { name } = self;
        format!("Missing type annotation for function argument `{name}`")
    }
}

define_violation!(
    pub struct MissingTypeArgs {
        pub name: String,
    }
);
impl Violation for MissingTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeArgs { name } = self;
        format!("Missing type annotation for `*{name}`")
    }
}

define_violation!(
    pub struct MissingTypeKwargs {
        pub name: String,
    }
);
impl Violation for MissingTypeKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeKwargs { name } = self;
        format!("Missing type annotation for `**{name}`")
    }
}

define_violation!(
    pub struct MissingTypeSelf {
        pub name: String,
    }
);
impl Violation for MissingTypeSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeSelf { name } = self;
        format!("Missing type annotation for `{name}` in method")
    }
}

define_violation!(
    pub struct MissingTypeCls {
        pub name: String,
    }
);
impl Violation for MissingTypeCls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeCls { name } = self;
        format!("Missing type annotation for `{name}` in classmethod")
    }
}

define_violation!(
    pub struct MissingReturnTypePublicFunction {
        pub name: String,
    }
);
impl Violation for MissingReturnTypePublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePublicFunction { name } = self;
        format!("Missing return type annotation for public function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypePrivateFunction {
        pub name: String,
    }
);
impl Violation for MissingReturnTypePrivateFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePrivateFunction { name } = self;
        format!("Missing return type annotation for private function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeSpecialMethod {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for MissingReturnTypeSpecialMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeSpecialMethod { name } = self;
        format!("Missing return type annotation for special method `{name}`")
    }

    fn autofix_title(&self) -> String {
        "Add `None` return type".to_string()
    }
}

define_violation!(
    pub struct MissingReturnTypeStaticMethod {
        pub name: String,
    }
);
impl Violation for MissingReturnTypeStaticMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeStaticMethod { name } = self;
        format!("Missing return type annotation for staticmethod `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeClassMethod {
        pub name: String,
    }
);
impl Violation for MissingReturnTypeClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeClassMethod { name } = self;
        format!("Missing return type annotation for classmethod `{name}`")
    }
}

define_violation!(
    pub struct DynamicallyTypedExpression {
        pub name: String,
    }
);
impl Violation for DynamicallyTypedExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DynamicallyTypedExpression { name } = self;
        format!("Dynamically typed expressions (typing.Any) are disallowed in `{name}`")
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
fn check_dynamically_typed<F>(
    checker: &Checker,
    annotation: &Expr,
    func: F,
    diagnostics: &mut Vec<Diagnostic>,
) where
    F: FnOnce() -> String,
{
    if checker.match_typing_expr(annotation, "Any") {
        diagnostics.push(Diagnostic::new(
            DynamicallyTypedExpression { name: func() },
            Range::from_located(annotation),
        ));
    };
}

/// Generate flake8-annotation checks for a given `Definition`.
pub fn definition(
    checker: &Checker,
    definition: &Definition,
    visibility: &Visibility,
) -> Vec<Diagnostic> {
    // TODO(charlie): Consider using the AST directly here rather than `Definition`.
    // We could adhere more closely to `flake8-annotations` by defining public
    // vs. secret vs. protected.
    if let DefinitionKind::Function(stmt)
    | DefinitionKind::NestedFunction(stmt)
    | DefinitionKind::Method(stmt) = &definition.kind
    {
        let is_method = matches!(definition.kind, DefinitionKind::Method(_));
        let (name, args, returns, body) = match_function_def(stmt);
        // Keep track of whether we've seen any typed arguments or return values.
        let mut has_any_typed_arg = false; // Any argument has been typed?
        let mut has_typed_return = false; // Return value has been typed?
        let mut has_typed_self_or_cls = false; // Has a typed `self` or `cls` argument?

        // Temporary storage for diagnostics; we emit them at the end
        // unless configured to suppress ANN* for declarations that are fully untyped.
        let mut diagnostics = Vec::new();

        // ANN001, ANN401
        for arg in args
            .args
            .iter()
            .chain(args.posonlyargs.iter())
            .chain(args.kwonlyargs.iter())
            .skip(
                // If this is a non-static method, skip `cls` or `self`.
                usize::from(
                    is_method && !visibility::is_staticmethod(checker, cast::decorator_list(stmt)),
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
                    check_dynamically_typed(
                        checker,
                        annotation,
                        || arg.node.arg.to_string(),
                        &mut diagnostics,
                    );
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
                        diagnostics.push(Diagnostic::new(
                            MissingTypeFunctionArgument {
                                name: arg.node.arg.to_string(),
                            },
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
                        check_dynamically_typed(
                            checker,
                            expr,
                            || format!("*{name}"),
                            &mut diagnostics,
                        );
                    }
                }
            } else {
                if !(checker.settings.flake8_annotations.suppress_dummy_args
                    && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                {
                    if checker.settings.rules.enabled(&Rule::MissingTypeArgs) {
                        diagnostics.push(Diagnostic::new(
                            MissingTypeArgs {
                                name: arg.node.arg.to_string(),
                            },
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
                        check_dynamically_typed(
                            checker,
                            expr,
                            || format!("**{name}"),
                            &mut diagnostics,
                        );
                    }
                }
            } else {
                if !(checker.settings.flake8_annotations.suppress_dummy_args
                    && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                {
                    if checker.settings.rules.enabled(&Rule::MissingTypeKwargs) {
                        diagnostics.push(Diagnostic::new(
                            MissingTypeKwargs {
                                name: arg.node.arg.to_string(),
                            },
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
                            diagnostics.push(Diagnostic::new(
                                MissingTypeCls {
                                    name: arg.node.arg.to_string(),
                                },
                                Range::from_located(arg),
                            ));
                        }
                    } else {
                        if checker.settings.rules.enabled(&Rule::MissingTypeSelf) {
                            diagnostics.push(Diagnostic::new(
                                MissingTypeSelf {
                                    name: arg.node.arg.to_string(),
                                },
                                Range::from_located(arg),
                            ));
                        }
                    }
                } else {
                    has_typed_self_or_cls = true;
                }
            }
        }

        // ANN201, ANN202, ANN401
        if let Some(expr) = &returns {
            has_typed_return = true;
            if checker
                .settings
                .rules
                .enabled(&Rule::DynamicallyTypedExpression)
            {
                check_dynamically_typed(checker, expr, || name.to_string(), &mut diagnostics);
            }
        } else if !(
            // Allow omission of return annotation if the function only returns `None`
            // (explicitly or implicitly).
            checker.settings.flake8_annotations.suppress_none_returning && is_none_returning(body)
        ) {
            if is_method && visibility::is_classmethod(checker, cast::decorator_list(stmt)) {
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::MissingReturnTypeClassMethod)
                {
                    diagnostics.push(Diagnostic::new(
                        MissingReturnTypeClassMethod {
                            name: name.to_string(),
                        },
                        helpers::identifier_range(stmt, checker.locator),
                    ));
                }
            } else if is_method && visibility::is_staticmethod(checker, cast::decorator_list(stmt))
            {
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::MissingReturnTypeStaticMethod)
                {
                    diagnostics.push(Diagnostic::new(
                        MissingReturnTypeStaticMethod {
                            name: name.to_string(),
                        },
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
                    if !(checker.settings.flake8_annotations.mypy_init_return && has_any_typed_arg)
                    {
                        let mut diagnostic = Diagnostic::new(
                            MissingReturnTypeSpecialMethod {
                                name: name.to_string(),
                            },
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
                        diagnostics.push(diagnostic);
                    }
                }
            } else if is_method && visibility::is_magic(cast::name(stmt)) {
                if checker
                    .settings
                    .rules
                    .enabled(&Rule::MissingReturnTypeSpecialMethod)
                {
                    diagnostics.push(Diagnostic::new(
                        MissingReturnTypeSpecialMethod {
                            name: name.to_string(),
                        },
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
                            diagnostics.push(Diagnostic::new(
                                MissingReturnTypePublicFunction {
                                    name: name.to_string(),
                                },
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
                            diagnostics.push(Diagnostic::new(
                                MissingReturnTypePrivateFunction {
                                    name: name.to_string(),
                                },
                                helpers::identifier_range(stmt, checker.locator),
                            ));
                        }
                    }
                }
            }
        }
        // If settings say so, don't report any of the
        // diagnostics gathered here if there were no type annotations at all.
        if checker.settings.flake8_annotations.ignore_fully_untyped
            && !(has_any_typed_arg || has_typed_self_or_cls || has_typed_return)
        {
            vec![]
        } else {
            diagnostics
        }
    } else {
        vec![]
    }
}
