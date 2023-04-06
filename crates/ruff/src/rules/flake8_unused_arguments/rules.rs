use std::iter;

use regex::Regex;
use rustpython_parser::ast::{Arg, Arguments};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::binding::Bindings;
use ruff_python_semantic::scope::{FunctionDef, Lambda, Scope, ScopeKind};

use crate::checkers::ast::Checker;

use super::helpers;
use super::types::Argumentable;

#[violation]
pub struct UnusedFunctionArgument {
    pub name: String,
}

impl Violation for UnusedFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedFunctionArgument { name } = self;
        format!("Unused function argument: `{name}`")
    }
}

#[violation]
pub struct UnusedMethodArgument {
    pub name: String,
}

impl Violation for UnusedMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedMethodArgument { name } = self;
        format!("Unused method argument: `{name}`")
    }
}

#[violation]
pub struct UnusedClassMethodArgument {
    pub name: String,
}

impl Violation for UnusedClassMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedClassMethodArgument { name } = self;
        format!("Unused class method argument: `{name}`")
    }
}

#[violation]
pub struct UnusedStaticMethodArgument {
    pub name: String,
}

impl Violation for UnusedStaticMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedStaticMethodArgument { name } = self;
        format!("Unused static method argument: `{name}`")
    }
}

#[violation]
pub struct UnusedLambdaArgument {
    pub name: String,
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
            .get(arg.node.arg.as_str())
            .map(|index| &bindings[*index])
        {
            if !binding.used()
                && binding.kind.is_argument()
                && !dummy_variable_rgx.is_match(arg.node.arg.as_str())
            {
                diagnostics.push(Diagnostic::new(
                    argumentable.check_for(arg.node.arg.to_string()),
                    binding.range,
                ));
            }
        }
    }
    diagnostics
}

/// ARG001, ARG002, ARG003, ARG004, ARG005
pub fn unused_arguments(
    checker: &Checker,
    parent: &Scope,
    scope: &Scope,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    match &scope.kind {
        ScopeKind::Function(FunctionDef {
            name,
            args,
            body,
            decorator_list,
            ..
        }) => {
            match function_type::classify(
                &checker.ctx,
                parent,
                name,
                decorator_list,
                &checker.settings.pep8_naming.classmethod_decorators,
                &checker.settings.pep8_naming.staticmethod_decorators,
            ) {
                FunctionType::Function => {
                    if checker
                        .settings
                        .rules
                        .enabled(Argumentable::Function.rule_code())
                        && !visibility::is_overload(&checker.ctx, decorator_list)
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
                    if checker
                        .settings
                        .rules
                        .enabled(Argumentable::Method.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(&checker.ctx, decorator_list)
                        && !visibility::is_override(&checker.ctx, decorator_list)
                        && !visibility::is_overload(&checker.ctx, decorator_list)
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
                    if checker
                        .settings
                        .rules
                        .enabled(Argumentable::ClassMethod.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(&checker.ctx, decorator_list)
                        && !visibility::is_override(&checker.ctx, decorator_list)
                        && !visibility::is_overload(&checker.ctx, decorator_list)
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
                    if checker
                        .settings
                        .rules
                        .enabled(Argumentable::StaticMethod.rule_code())
                        && !helpers::is_empty(body)
                        && (!visibility::is_magic(name)
                            || visibility::is_init(name)
                            || visibility::is_new(name)
                            || visibility::is_call(name))
                        && !visibility::is_abstract(&checker.ctx, decorator_list)
                        && !visibility::is_override(&checker.ctx, decorator_list)
                        && !visibility::is_overload(&checker.ctx, decorator_list)
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
        ScopeKind::Lambda(Lambda { args, .. }) => {
            if checker
                .settings
                .rules
                .enabled(Argumentable::Lambda.rule_code())
            {
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
