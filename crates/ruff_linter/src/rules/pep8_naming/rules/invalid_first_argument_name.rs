use ruff_python_ast as ast;
use ruff_python_ast::ParameterWithDefault;

use ruff_diagnostics::{Diagnostic, Edit, Violation};
use ruff_diagnostics::{DiagnosticKind, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{Scope, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for instance methods that use a name other than `self` for their
/// first argument.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of `self` as first argument for all instance
/// methods:
///
/// > Always use self for the first argument to instance methods.
/// >
/// > If a function argument’s name clashes with a reserved keyword, it is generally better to
/// > append a single trailing underscore rather than use an abbreviation or spelling corruption.
/// > Thus `class_` is better than `clss`. (Perhaps better is to avoid such clashes by using a synonym.)
///
/// Names can be excluded from this rule using the [`lint.pep8-naming.ignore-names`]
/// or [`lint.pep8-naming.extend-ignore-names`] configuration options. For example,
/// to allow the use of `this` as the first argument to instance methods, set
/// the [`lint.pep8-naming.extend-ignore-names`] option to `["this"]`.
///
/// ## Example
/// ```python
/// class Example:
///     def function(cls, data):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     def function(self, data):
///         ...
/// ```
///
/// ## Options
/// - `lint.pep8-naming.classmethod-decorators`
/// - `lint.pep8-naming.staticmethod-decorators`
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct InvalidFirstArgumentNameForMethod {
    argument_name: String,
}

impl Violation for InvalidFirstArgumentNameForMethod {
    const FIX_AVAILABILITY: ruff_diagnostics::FixAvailability =
        ruff_diagnostics::FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a method should be named `self`")
    }

    fn fix_title(&self) -> Option<String> {
        let Self { argument_name } = self;
        Some(format!("Rename `{argument_name}` to `self`"));
        None
    }
}

/// ## What it does
/// Checks for class methods that use a name other than `cls` for their
/// first argument.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of `cls` as the first argument for all class
/// methods:
///
/// > Always use `cls` for the first argument to class methods.
/// >
/// > If a function argument’s name clashes with a reserved keyword, it is generally better to
/// > append a single trailing underscore rather than use an abbreviation or spelling corruption.
/// > Thus `class_` is better than `clss`. (Perhaps better is to avoid such clashes by using a synonym.)
///
/// Names can be excluded from this rule using the [`lint.pep8-naming.ignore-names`]
/// or [`lint.pep8-naming.extend-ignore-names`] configuration options. For example,
/// to allow the use of `klass` as the first argument to class methods, set
/// the [`lint.pep8-naming.extend-ignore-names`] option to `["klass"]`.
///
/// ## Example
/// ```python
/// class Example:
///     @classmethod
///     def function(self, data):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     @classmethod
///     def function(cls, data):
///         ...
/// ```
///
/// ## Options
/// - `lint.pep8-naming.classmethod-decorators`
/// - `lint.pep8-naming.staticmethod-decorators`
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct InvalidFirstArgumentNameForClassMethod {
    argument_name: String,
}

impl Violation for InvalidFirstArgumentNameForClassMethod {
    const FIX_AVAILABILITY: ruff_diagnostics::FixAvailability =
        ruff_diagnostics::FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a class method should be named `cls`")
    }

    fn fix_title(&self) -> Option<String> {
        let Self { argument_name } = self;
        Some(format!("Rename `{argument_name}` to `cls`"));
        None
    }
}

/// An AST node that can contain arguments.
#[derive(Debug, Copy, Clone)]
enum Argumentable {
    Method,
    ClassMethod,
}

impl Argumentable {
    fn check_for(self, argument_name: String) -> DiagnosticKind {
        match self {
            Self::Method => InvalidFirstArgumentNameForMethod { argument_name }.into(),
            Self::ClassMethod => InvalidFirstArgumentNameForClassMethod { argument_name }.into(),
        }
    }

    fn valid_argument_name(self) -> &'static str {
        match self {
            Self::Method => "self",
            Self::ClassMethod => "cls",
        }
    }

    const fn rule_code(self) -> Rule {
        match self {
            Self::Method => Rule::InvalidFirstArgumentNameForMethod,
            Self::ClassMethod => Rule::InvalidFirstArgumentNameForClassMethod,
        }
    }
}

/// N804, N805
pub(crate) fn invalid_first_argument_name(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        parameters,
        // body,
        decorator_list,
        ..
    }) = &scope.kind
    else {
        panic!("Expected ScopeKind::Function")
    };

    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    let argumentable = match function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        function_type::FunctionType::Function | function_type::FunctionType::StaticMethod => {
            return;
        }
        function_type::FunctionType::Method => Argumentable::Method,
        function_type::FunctionType::ClassMethod => Argumentable::ClassMethod,
    };
    if !checker.enabled(argumentable.rule_code()) {
        return;
    }

    let Some(ParameterWithDefault { parameter, .. }) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    else {
        return;
    };

    if &parameter.name == argumentable.valid_argument_name() {
        return;
    }
    if checker.settings.pep8_naming.ignore_names.matches(name) {
        return;
    }

    let fix = if let Some(bid) = scope.get(&parameter.name) {
        let binding = checker.semantic().binding(bid);
        let replacement = argumentable.valid_argument_name();
        let fix = Fix::unsafe_edits(
            Edit::range_replacement(replacement.to_string(), binding.range()),
            binding
                .references()
                .map(|rid| checker.semantic().reference(rid))
                .map(|reference| {
                    Edit::range_replacement(replacement.to_string(), reference.range())
                }),
        );
        Some(fix)
    } else {
        None
    };

    let mut diagnostic = Diagnostic::new(
        argumentable.check_for(parameter.name.to_string()),
        parameter.range(),
    );
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
    diagnostics.push(diagnostic);
}
