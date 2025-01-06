use regex::Regex;
use ruff_python_ast as ast;
use ruff_python_ast::{Parameter, Parameters, Stmt, StmtExpr, StmtFunctionDef, StmtRaise};

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::analyze::{function_type, visibility};
use ruff_python_semantic::{Scope, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for the presence of unused arguments in function definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// def foo(bar, baz):
///     return bar * 2
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     return bar * 2
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedFunctionArgument {
    name: String,
}

impl Violation for UnusedFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedFunctionArgument { name } = self;
        format!("Unused function argument: `{name}`")
    }
}

/// ## What it does
/// Checks for the presence of unused arguments in instance method definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// class Class:
///     def foo(self, arg1, arg2):
///         print(arg1)
/// ```
///
/// Use instead:
/// ```python
/// class Class:
///     def foo(self, arg1):
///         print(arg1)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedMethodArgument {
    name: String,
}

impl Violation for UnusedMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedMethodArgument { name } = self;
        format!("Unused method argument: `{name}`")
    }
}

/// ## What it does
/// Checks for the presence of unused arguments in class method definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// class Class:
///     @classmethod
///     def foo(cls, arg1, arg2):
///         print(arg1)
/// ```
///
/// Use instead:
/// ```python
/// class Class:
///     @classmethod
///     def foo(cls, arg1):
///         print(arg1)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedClassMethodArgument {
    name: String,
}

impl Violation for UnusedClassMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedClassMethodArgument { name } = self;
        format!("Unused class method argument: `{name}`")
    }
}

/// ## What it does
/// Checks for the presence of unused arguments in static method definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// class Class:
///     @staticmethod
///     def foo(arg1, arg2):
///         print(arg1)
/// ```
///
/// Use instead:
/// ```python
/// class Class:
///     @staticmethod
///     def foo(arg1):
///         print(arg1)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedStaticMethodArgument {
    name: String,
}

impl Violation for UnusedStaticMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedStaticMethodArgument { name } = self;
        format!("Unused static method argument: `{name}`")
    }
}

/// ## What it does
/// Checks for the presence of unused arguments in lambda expression
/// definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// my_list = [1, 2, 3, 4, 5]
/// squares = map(lambda x, y: x**2, my_list)
/// ```
///
/// Use instead:
/// ```python
/// my_list = [1, 2, 3, 4, 5]
/// squares = map(lambda x: x**2, my_list)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedLambdaArgument {
    name: String,
}

impl Violation for UnusedLambdaArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLambdaArgument { name } = self;
        format!("Unused lambda argument: `{name}`")
    }
}

/// An AST node that can contain arguments.
#[derive(Debug, Copy, Clone)]
enum Argumentable {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
    Lambda,
}

impl Argumentable {
    fn check_for(self, name: String) -> DiagnosticKind {
        match self {
            Self::Function => UnusedFunctionArgument { name }.into(),
            Self::Method => UnusedMethodArgument { name }.into(),
            Self::ClassMethod => UnusedClassMethodArgument { name }.into(),
            Self::StaticMethod => UnusedStaticMethodArgument { name }.into(),
            Self::Lambda => UnusedLambdaArgument { name }.into(),
        }
    }

    const fn rule_code(self) -> Rule {
        match self {
            Self::Function => Rule::UnusedFunctionArgument,
            Self::Method => Rule::UnusedMethodArgument,
            Self::ClassMethod => Rule::UnusedClassMethodArgument,
            Self::StaticMethod => Rule::UnusedStaticMethodArgument,
            Self::Lambda => Rule::UnusedLambdaArgument,
        }
    }
}

/// Check a plain function for unused arguments.
fn function(
    argumentable: Argumentable,
    parameters: &Parameters,
    scope: &Scope,
    semantic: &SemanticModel,
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let args = parameters
        .iter_non_variadic_params()
        .map(|parameter_with_default| &parameter_with_default.parameter)
        .chain(
            parameters
                .vararg
                .as_deref()
                .into_iter()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            parameters
                .kwarg
                .as_deref()
                .into_iter()
                .skip(usize::from(ignore_variadic_names)),
        );
    call(
        argumentable,
        args,
        scope,
        semantic,
        dummy_variable_rgx,
        diagnostics,
    );
}

/// Check a method for unused arguments.
fn method(
    argumentable: Argumentable,
    parameters: &Parameters,
    scope: &Scope,
    semantic: &SemanticModel,
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let args = parameters
        .iter_non_variadic_params()
        .skip(1)
        .map(|parameter_with_default| &parameter_with_default.parameter)
        .chain(
            parameters
                .vararg
                .as_deref()
                .into_iter()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            parameters
                .kwarg
                .as_deref()
                .into_iter()
                .skip(usize::from(ignore_variadic_names)),
        );
    call(
        argumentable,
        args,
        scope,
        semantic,
        dummy_variable_rgx,
        diagnostics,
    );
}

fn call<'a>(
    argumentable: Argumentable,
    parameters: impl Iterator<Item = &'a Parameter>,
    scope: &Scope,
    semantic: &SemanticModel,
    dummy_variable_rgx: &Regex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.extend(parameters.filter_map(|arg| {
        let binding = scope
            .get(arg.name.as_str())
            .map(|binding_id| semantic.binding(binding_id))?;
        if binding.kind.is_argument()
            && binding.is_unused()
            && !dummy_variable_rgx.is_match(arg.name.as_str())
        {
            Some(Diagnostic::new(
                argumentable.check_for(arg.name.to_string()),
                binding.range(),
            ))
        } else {
            None
        }
    }));
}

/// Returns `true` if a function appears to be a base class stub. In other
/// words, if it matches the following syntax:
///
/// ```text
/// variable = <string | f-string>
/// raise NotImplementedError(variable)
/// ```
///
/// See also [`is_stub`]. We're a bit more generous in what is considered a
/// stub in this rule to avoid clashing with [`EM101`].
///
/// [`is_stub`]: function_type::is_stub
/// [`EM101`]: https://docs.astral.sh/ruff/rules/raw-string-in-exception/
fn is_not_implemented_stub_with_variable(
    function_def: &StmtFunctionDef,
    semantic: &SemanticModel,
) -> bool {
    // Ignore doc-strings.
    let statements = match function_def.body.as_slice() {
        [Stmt::Expr(StmtExpr { value, .. }), rest @ ..] if value.is_string_literal_expr() => rest,
        _ => &function_def.body,
    };

    let [Stmt::Assign(ast::StmtAssign { targets, value, .. }), Stmt::Raise(StmtRaise {
        exc: Some(exception),
        ..
    })] = statements
    else {
        return false;
    };

    if !matches!(**value, ast::Expr::StringLiteral(_) | ast::Expr::FString(_)) {
        return false;
    }

    let ast::Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = &**exception
    else {
        return false;
    };

    if !semantic.match_builtin_expr(func, "NotImplementedError") {
        return false;
    }

    let [argument] = &*arguments.args else {
        return false;
    };

    let [target] = targets.as_slice() else {
        return false;
    };

    argument.as_name_expr().map(ast::ExprName::id) == target.as_name_expr().map(ast::ExprName::id)
}

/// ARG001, ARG002, ARG003, ARG004, ARG005
pub(crate) fn unused_arguments(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if scope.uses_locals() {
        return;
    }

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    match &scope.kind {
        ScopeKind::Function(
            function_def @ ast::StmtFunctionDef {
                name,
                parameters,
                decorator_list,
                ..
            },
        ) => {
            match function_type::classify(
                name,
                decorator_list,
                parent,
                checker.semantic(),
                &checker.settings.pep8_naming.classmethod_decorators,
                &checker.settings.pep8_naming.staticmethod_decorators,
            ) {
                function_type::FunctionType::Function => {
                    if checker.enabled(Argumentable::Function.rule_code())
                        && !function_type::is_stub(function_def, checker.semantic())
                        && !is_not_implemented_stub_with_variable(function_def, checker.semantic())
                        && !visibility::is_overload(decorator_list, checker.semantic())
                    {
                        function(
                            Argumentable::Function,
                            parameters,
                            scope,
                            checker.semantic(),
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                            diagnostics,
                        );
                    }
                }
                function_type::FunctionType::Method => {
                    if checker.enabled(Argumentable::Method.rule_code())
                        && !function_type::is_stub(function_def, checker.semantic())
                        && !is_not_implemented_stub_with_variable(function_def, checker.semantic())
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(decorator_list, checker.semantic())
                        && !visibility::is_override(decorator_list, checker.semantic())
                        && !visibility::is_overload(decorator_list, checker.semantic())
                    {
                        method(
                            Argumentable::Method,
                            parameters,
                            scope,
                            checker.semantic(),
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                            diagnostics,
                        );
                    }
                }
                function_type::FunctionType::ClassMethod => {
                    if checker.enabled(Argumentable::ClassMethod.rule_code())
                        && !function_type::is_stub(function_def, checker.semantic())
                        && !is_not_implemented_stub_with_variable(function_def, checker.semantic())
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(decorator_list, checker.semantic())
                        && !visibility::is_override(decorator_list, checker.semantic())
                        && !visibility::is_overload(decorator_list, checker.semantic())
                    {
                        method(
                            Argumentable::ClassMethod,
                            parameters,
                            scope,
                            checker.semantic(),
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                            diagnostics,
                        );
                    }
                }
                function_type::FunctionType::StaticMethod => {
                    if checker.enabled(Argumentable::StaticMethod.rule_code())
                        && !function_type::is_stub(function_def, checker.semantic())
                        && !is_not_implemented_stub_with_variable(function_def, checker.semantic())
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(decorator_list, checker.semantic())
                        && !visibility::is_override(decorator_list, checker.semantic())
                        && !visibility::is_overload(decorator_list, checker.semantic())
                    {
                        function(
                            Argumentable::StaticMethod,
                            parameters,
                            scope,
                            checker.semantic(),
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                            diagnostics,
                        );
                    }
                }
            }
        }
        ScopeKind::Lambda(ast::ExprLambda { parameters, .. }) => {
            if let Some(parameters) = parameters {
                if checker.enabled(Argumentable::Lambda.rule_code()) {
                    function(
                        Argumentable::Lambda,
                        parameters,
                        scope,
                        checker.semantic(),
                        &checker.settings.dummy_variable_rgx,
                        checker
                            .settings
                            .flake8_unused_arguments
                            .ignore_variadic_names,
                        diagnostics,
                    );
                }
            }
        }
        _ => panic!("Expected ScopeKind::Function | ScopeKind::Lambda"),
    }
}
