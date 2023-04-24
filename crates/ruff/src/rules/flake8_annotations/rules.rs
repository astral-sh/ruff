use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{cast, helpers};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::analyze::visibility::Visibility;
use ruff_python_stdlib::typing::SIMPLE_MAGIC_RETURN_TYPES;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::registry::{AsRule, Rule};

use super::fixes;
use super::helpers::match_function_def;

/// ## What it does
/// Checks that function arguments have type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the types of function arguments. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any provided arguments match expectation.
///
/// ## Example
/// ```python
/// def foo(x):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int):
///     ...
/// ```
#[violation]
pub struct MissingTypeFunctionArgument {
    pub name: String,
}

impl Violation for MissingTypeFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeFunctionArgument { name } = self;
        format!("Missing type annotation for function argument `{name}`")
    }
}

/// ## What it does
/// Checks that function `*args` arguments have type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the types of function arguments. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any provided arguments match expectation.
///
/// ## Example
/// ```python
/// def foo(*args):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(*args: int):
///     ...
/// ```
#[violation]
pub struct MissingTypeArgs {
    pub name: String,
}

impl Violation for MissingTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeArgs { name } = self;
        format!("Missing type annotation for `*{name}`")
    }
}

/// ## What it does
/// Checks that function `**kwargs` arguments have type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the types of function arguments. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any provided arguments match expectation.
///
/// ## Example
/// ```python
/// def foo(**kwargs):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(**kwargs: int):
///     ...
/// ```
#[violation]
pub struct MissingTypeKwargs {
    pub name: String,
}

impl Violation for MissingTypeKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeKwargs { name } = self;
        format!("Missing type annotation for `**{name}`")
    }
}

/// ## What it does
/// Checks that instance method `self` arguments have type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the types of function arguments. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any provided arguments match expectation.
///
/// Note that many type checkers will infer the type of `self` automatically, so this
/// annotation is not strictly necessary.
///
/// ## Example
/// ```python
/// class Foo:
///     def bar(self):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def bar(self: "Foo"):
///         ...
/// ```
#[violation]
pub struct MissingTypeSelf {
    pub name: String,
}

impl Violation for MissingTypeSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeSelf { name } = self;
        format!("Missing type annotation for `{name}` in method")
    }
}

/// ## What it does
/// Checks that class method `cls` arguments have type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the types of function arguments. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any provided arguments match expectation.
///
/// Note that many type checkers will infer the type of `cls` automatically, so this
/// annotation is not strictly necessary.
///
/// ## Example
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls: Type["Foo"]):
///         ...
/// ```
#[violation]
pub struct MissingTypeCls {
    pub name: String,
}

impl Violation for MissingTypeCls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeCls { name } = self;
        format!("Missing type annotation for `{name}` in classmethod")
    }
}

/// ## What it does
/// Checks that public functions and methods have return type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the return types of functions. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any returned values, and the types expected by callers, match expectation.
///
/// ## Example
/// ```python
/// def add(a, b):
///     return a + b
/// ```
///
/// Use instead:
/// ```python
/// def add(a: int, b: int) -> int:
///     return a + b
/// ```
#[violation]
pub struct MissingReturnTypeUndocumentedPublicFunction {
    pub name: String,
}

impl Violation for MissingReturnTypeUndocumentedPublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeUndocumentedPublicFunction { name } = self;
        format!("Missing return type annotation for public function `{name}`")
    }
}

/// ## What it does
/// Checks that private functions and methods have return type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the return types of functions. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any returned values, and the types expected by callers, match expectation.
///
/// ## Example
/// ```python
/// def _add(a, b):
///     return a + b
/// ```
///
/// Use instead:
/// ```python
/// def _add(a: int, b: int) -> int:
///     return a + b
/// ```
#[violation]
pub struct MissingReturnTypePrivateFunction {
    pub name: String,
}

impl Violation for MissingReturnTypePrivateFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePrivateFunction { name } = self;
        format!("Missing return type annotation for private function `{name}`")
    }
}

/// ## What it does
/// Checks that "special" methods, like `__init__`, `__new__`, and `__call__`, have
/// return type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the return types of functions. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any returned values, and the types expected by callers, match expectation.
///
/// Note that type checkers often allow you to omit the return type annotation for
/// `__init__` methods, as long as at least one argument has a type annotation. To
/// opt-in to this behavior, use the `mypy-init-return` setting in your `pyproject.toml`
/// or `ruff.toml` file:
///
/// ```toml
/// [tool.ruff.flake8-annotations]
/// mypy-init-return = true
/// ```
///
/// ## Example
/// ```python
/// class Foo:
///     def __init__(self, x: int):
///         self.x = x
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __init__(self, x: int) -> None:
///         self.x = x
/// ```
#[violation]
pub struct MissingReturnTypeSpecialMethod {
    pub name: String,
}

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

/// ## What it does
/// Checks that static methods have return type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the return types of functions. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any returned values, and the types expected by callers, match expectation.
///
/// ## Example
/// ```python
/// class Foo:
///     @staticmethod
///     def bar():
///         return 1
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     @staticmethod
///     def bar() -> int:
///         return 1
/// ```
#[violation]
pub struct MissingReturnTypeStaticMethod {
    pub name: String,
}

impl Violation for MissingReturnTypeStaticMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeStaticMethod { name } = self;
        format!("Missing return type annotation for staticmethod `{name}`")
    }
}

/// ## What it does
/// Checks that class methods have return type annotations.
///
/// ## Why is this bad?
/// Type annotations are a good way to document the return types of functions. They also
/// help catch bugs, when used alongside a type checker, by ensuring that the types of
/// any returned values, and the types expected by callers, match expectation.
///
/// ## Example
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls):
///         return 1
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls) -> int:
///         return 1
/// ```
#[violation]
pub struct MissingReturnTypeClassMethod {
    pub name: String,
}

impl Violation for MissingReturnTypeClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeClassMethod { name } = self;
        format!("Missing return type annotation for classmethod `{name}`")
    }
}

/// ## What it does
/// Checks that an expression is annotated with a more specific type than
/// `Any`.
///
/// ## Why is this bad?
/// `Any` is a special type indicating an unconstrained type. When an
/// expression is annotated with type `Any`, type checkers will allow all
/// operations on it.
///
/// It's better to be explicit about the type of an expression, and to use
/// `Any` as an "escape hatch" only when it is really needed.
///
/// ## Example
/// ```python
/// def foo(x: Any):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int):
///     ...
/// ```
///
/// ## References
/// - [PEP 484](https://www.python.org/dev/peps/pep-0484/#the-any-type)
/// - [`typing.Any`](https://docs.python.org/3/library/typing.html#typing.Any)
/// - [Mypy: The Any type](https://mypy.readthedocs.io/en/stable/kinds_of_types.html#the-any-type)
#[violation]
pub struct AnyType {
    pub name: String,
}

impl Violation for AnyType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AnyType { name } = self;
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
    if checker.ctx.match_typing_expr(annotation, "Any") {
        diagnostics.push(Diagnostic::new(
            AnyType { name: func() },
            Range::from(annotation),
        ));
    };
}

/// Generate flake8-annotation checks for a given `Definition`.
pub fn definition(
    checker: &Checker,
    definition: &Definition,
    visibility: Visibility,
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
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .chain(args.kwonlyargs.iter())
            .skip(
                // If this is a non-static method, skip `cls` or `self`.
                usize::from(
                    is_method
                        && !visibility::is_staticmethod(&checker.ctx, cast::decorator_list(stmt)),
                ),
            )
        {
            // ANN401 for dynamically typed arguments
            if let Some(annotation) = &arg.node.annotation {
                has_any_typed_arg = true;
                if checker.settings.rules.enabled(Rule::AnyType) {
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
                        .enabled(Rule::MissingTypeFunctionArgument)
                    {
                        diagnostics.push(Diagnostic::new(
                            MissingTypeFunctionArgument {
                                name: arg.node.arg.to_string(),
                            },
                            Range::from(arg),
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
                    if checker.settings.rules.enabled(Rule::AnyType) {
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
                    if checker.settings.rules.enabled(Rule::MissingTypeArgs) {
                        diagnostics.push(Diagnostic::new(
                            MissingTypeArgs {
                                name: arg.node.arg.to_string(),
                            },
                            Range::from(arg),
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
                    if checker.settings.rules.enabled(Rule::AnyType) {
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
                    if checker.settings.rules.enabled(Rule::MissingTypeKwargs) {
                        diagnostics.push(Diagnostic::new(
                            MissingTypeKwargs {
                                name: arg.node.arg.to_string(),
                            },
                            Range::from(arg),
                        ));
                    }
                }
            }
        }

        // ANN101, ANN102
        if is_method && !visibility::is_staticmethod(&checker.ctx, cast::decorator_list(stmt)) {
            if let Some(arg) = args.posonlyargs.first().or_else(|| args.args.first()) {
                if arg.node.annotation.is_none() {
                    if visibility::is_classmethod(&checker.ctx, cast::decorator_list(stmt)) {
                        if checker.settings.rules.enabled(Rule::MissingTypeCls) {
                            diagnostics.push(Diagnostic::new(
                                MissingTypeCls {
                                    name: arg.node.arg.to_string(),
                                },
                                Range::from(arg),
                            ));
                        }
                    } else {
                        if checker.settings.rules.enabled(Rule::MissingTypeSelf) {
                            diagnostics.push(Diagnostic::new(
                                MissingTypeSelf {
                                    name: arg.node.arg.to_string(),
                                },
                                Range::from(arg),
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
            if checker.settings.rules.enabled(Rule::AnyType) {
                check_dynamically_typed(checker, expr, || name.to_string(), &mut diagnostics);
            }
        } else if !(
            // Allow omission of return annotation if the function only returns `None`
            // (explicitly or implicitly).
            checker.settings.flake8_annotations.suppress_none_returning && is_none_returning(body)
        ) {
            if is_method && visibility::is_classmethod(&checker.ctx, cast::decorator_list(stmt)) {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::MissingReturnTypeClassMethod)
                {
                    diagnostics.push(Diagnostic::new(
                        MissingReturnTypeClassMethod {
                            name: name.to_string(),
                        },
                        helpers::identifier_range(stmt, checker.locator),
                    ));
                }
            } else if is_method
                && visibility::is_staticmethod(&checker.ctx, cast::decorator_list(stmt))
            {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::MissingReturnTypeStaticMethod)
                {
                    diagnostics.push(Diagnostic::new(
                        MissingReturnTypeStaticMethod {
                            name: name.to_string(),
                        },
                        helpers::identifier_range(stmt, checker.locator),
                    ));
                }
            } else if is_method && visibility::is_init(name) {
                // Allow omission of return annotation in `__init__` functions, as long as at
                // least one argument is typed.
                if checker
                    .settings
                    .rules
                    .enabled(Rule::MissingReturnTypeSpecialMethod)
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
                            diagnostic.try_set_fix(|| {
                                fixes::add_return_annotation(checker.locator, stmt, "None")
                            });
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            } else if is_method && visibility::is_magic(name) {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::MissingReturnTypeSpecialMethod)
                {
                    let mut diagnostic = Diagnostic::new(
                        MissingReturnTypeSpecialMethod {
                            name: name.to_string(),
                        },
                        helpers::identifier_range(stmt, checker.locator),
                    );
                    let return_type = SIMPLE_MAGIC_RETURN_TYPES.get(name);
                    if let Some(return_type) = return_type {
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.try_set_fix(|| {
                                fixes::add_return_annotation(checker.locator, stmt, return_type)
                            });
                        }
                    }
                    diagnostics.push(diagnostic);
                }
            } else {
                match visibility {
                    Visibility::Public => {
                        if checker
                            .settings
                            .rules
                            .enabled(Rule::MissingReturnTypeUndocumentedPublicFunction)
                        {
                            diagnostics.push(Diagnostic::new(
                                MissingReturnTypeUndocumentedPublicFunction {
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
                            .enabled(Rule::MissingReturnTypePrivateFunction)
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
