use std::iter;

use regex::Regex;
use rustpython_parser::ast;
use rustpython_parser::ast::{Arg, Arguments};

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::binding::Bindings;
use ruff_python_semantic::scope::{Scope, ScopeKind};

use super::super::helpers;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// An AST node that can contain arguments.
#[derive(Copy, Clone)]
enum Argumentable {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
    Lambda,
}

impl Argumentable {
    pub(crate) fn check_for(self, name: String) -> DiagnosticKind {
        match self {
            Self::Function => UnusedFunctionArgument { name }.into(),
            Self::Method => UnusedMethodArgument { name }.into(),
            Self::ClassMethod => UnusedClassMethodArgument { name }.into(),
            Self::StaticMethod => UnusedStaticMethodArgument { name }.into(),
            Self::Lambda => UnusedLambdaArgument { name }.into(),
        }
    }

    pub(crate) const fn rule_code(self) -> Rule {
        match self {
            Self::Function => Rule::UnusedFunctionArgument,
            Self::Method => Rule::UnusedMethodArgument,
            Self::ClassMethod => Rule::UnusedClassMethodArgument,
            Self::StaticMethod => Rule::UnusedStaticMethodArgument,
            Self::Lambda => Rule::UnusedLambdaArgument,
        }
    }
}

/// ## What it does
/// Checks for the presence of unused arguments in function definitions.
///
/// ## Why is this bad?
/// An argument that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
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
#[violation]
pub struct UnusedFunctionArgument {
    pub(super) name: String,
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
/// ## Example
/// ```python
/// class MyClass:
///     def my_method(self, arg1, arg2):
///         print(arg1)
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     def my_method(self, arg1):
///         print(arg1)
/// ```
#[violation]
pub struct UnusedMethodArgument {
    pub(super) name: String,
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
/// ## Example
/// ```python
/// class MyClass:
///     @classmethod
///     def my_method(self, arg1, arg2):
///         print(arg1)
///
///     def other_method(self):
///         self.my_method("foo", "bar")
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     @classmethod
///     def my_method(self, arg1):
///         print(arg1)
///
///     def other_method(self):
///         self.my_method("foo", "bar")
/// ```
#[violation]
pub struct UnusedClassMethodArgument {
    pub(super) name: String,
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
/// ## Example
/// ```python
/// class MyClass:
///     @staticmethod
///     def my_static_method(self, arg1, arg2):
///         print(arg1)
///
///     def other_method(self):
///         self.my_static_method("foo", "bar")
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     @static
///     def my_static_method(self, arg1):
///         print(arg1)
///
///     def other_method(self):
///         self.my_static_method("foo", "bar")
/// ```
#[violation]
pub struct UnusedStaticMethodArgument {
    pub(super) name: String,
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
#[violation]
pub struct UnusedLambdaArgument {
    pub(super) name: String,
}

impl Violation for UnusedLambdaArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLambdaArgument { name } = self;
        format!("Unused lambda argument: `{name}`")
    }
}

/// Check a plain function for unused arguments.
fn function(
    argumentable: Argumentable,
    args: &Arguments,
    values: &Scope,
    bindings: &Bindings,
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
) -> Vec<Diagnostic> {
    let args = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .chain(
            iter::once::<Option<&Arg>>(args.vararg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            iter::once::<Option<&Arg>>(args.kwarg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        );
    call(argumentable, args, values, bindings, dummy_variable_rgx)
}

/// Check a method for unused arguments.
fn method(
    argumentable: Argumentable,
    args: &Arguments,
    values: &Scope,
    bindings: &Bindings,
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
) -> Vec<Diagnostic> {
    let args = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .skip(1)
        .chain(args.kwonlyargs.iter())
        .chain(
            iter::once::<Option<&Arg>>(args.vararg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            iter::once::<Option<&Arg>>(args.kwarg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        );
    call(argumentable, args, values, bindings, dummy_variable_rgx)
}

fn call<'a>(
    argumentable: Argumentable,
    args: impl Iterator<Item = &'a Arg>,
    values: &Scope,
    bindings: &Bindings,
    dummy_variable_rgx: &Regex,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];
    for arg in args {
        if let Some(binding) = values
            .get(arg.arg.as_str())
            .map(|binding_id| &bindings[binding_id])
        {
            if binding.kind.is_argument()
                && !binding.is_used()
                && !dummy_variable_rgx.is_match(arg.arg.as_str())
            {
                diagnostics.push(Diagnostic::new(
                    argumentable.check_for(arg.arg.to_string()),
                    binding.range,
                ));
            }
        }
    }
    diagnostics
}

/// ARG001, ARG002, ARG003, ARG004, ARG005
pub(crate) fn unused_arguments(
    checker: &Checker,
    parent: &Scope,
    scope: &Scope,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    match &scope.kind {
        ScopeKind::Function(ast::StmtFunctionDef {
            name,
            args,
            body,
            decorator_list,
            ..
        })
        | ScopeKind::AsyncFunction(ast::StmtAsyncFunctionDef {
            name,
            args,
            body,
            decorator_list,
            ..
        }) => {
            match function_type::classify(
                checker.semantic_model(),
                parent,
                name,
                decorator_list,
                &checker.settings.pep8_naming.classmethod_decorators,
                &checker.settings.pep8_naming.staticmethod_decorators,
            ) {
                FunctionType::Function => {
                    if checker.enabled(Argumentable::Function.rule_code())
                        && !visibility::is_overload(checker.semantic_model(), decorator_list)
                    {
                        function(
                            Argumentable::Function,
                            args,
                            scope,
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::Method => {
                    if checker.enabled(Argumentable::Method.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(checker.semantic_model(), decorator_list)
                        && !visibility::is_override(checker.semantic_model(), decorator_list)
                        && !visibility::is_overload(checker.semantic_model(), decorator_list)
                    {
                        method(
                            Argumentable::Method,
                            args,
                            scope,
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::ClassMethod => {
                    if checker.enabled(Argumentable::ClassMethod.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(checker.semantic_model(), decorator_list)
                        && !visibility::is_override(checker.semantic_model(), decorator_list)
                        && !visibility::is_overload(checker.semantic_model(), decorator_list)
                    {
                        method(
                            Argumentable::ClassMethod,
                            args,
                            scope,
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::StaticMethod => {
                    if checker.enabled(Argumentable::StaticMethod.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(checker.semantic_model(), decorator_list)
                        && !visibility::is_override(checker.semantic_model(), decorator_list)
                        && !visibility::is_overload(checker.semantic_model(), decorator_list)
                    {
                        function(
                            Argumentable::StaticMethod,
                            args,
                            scope,
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
            }
        }
        ScopeKind::Lambda(ast::ExprLambda { args, .. }) => {
            if checker.enabled(Argumentable::Lambda.rule_code()) {
                function(
                    Argumentable::Lambda,
                    args,
                    scope,
                    bindings,
                    &checker.settings.dummy_variable_rgx,
                    checker
                        .settings
                        .flake8_unused_arguments
                        .ignore_variadic_names,
                )
            } else {
                vec![]
            }
        }
        _ => panic!("Expected ScopeKind::Function | ScopeKind::Lambda"),
    }
}
