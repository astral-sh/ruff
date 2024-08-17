use ruff_python_ast::{self as ast, Arguments, Expr, Keyword, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for abstract classes without abstract methods.
///
/// ## Why is this bad?
/// Abstract base classes are used to define interfaces. If an abstract base
/// class has no abstract methods, you may have forgotten to add an abstract
/// method to the class or omitted an `@abstractmethod` decorator.
///
/// If the class is _not_ meant to be used as an interface, consider removing
/// the `ABC` base class from the class definition.
///
/// ## Example
/// ```python
/// from abc import ABC
///
///
/// class Foo(ABC):
///     def method(self):
///         bar()
/// ```
///
/// Use instead:
/// ```python
/// from abc import ABC, abstractmethod
///
///
/// class Foo(ABC):
///     @abstractmethod
///     def method(self):
///         bar()
/// ```
///
/// ## References
/// - [Python documentation: `abc`](https://docs.python.org/3/library/abc.html)
#[violation]
pub struct AbstractBaseClassWithoutAbstractMethod {
    name: String,
}

impl Violation for AbstractBaseClassWithoutAbstractMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod { name } = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }
}

/// ## What it does
/// Checks for empty methods in abstract base classes without an abstract
/// decorator.
///
/// ## Why is this bad?
/// Empty methods in abstract base classes without an abstract decorator may be
/// be indicative of a mistake. If the method is meant to be abstract, add an
/// `@abstractmethod` decorator to the method.
///
/// ## Example
///
/// ```python
/// from abc import ABC
///
///
/// class Foo(ABC):
///     def method(self): ...
/// ```
///
/// Use instead:
///
/// ```python
/// from abc import ABC, abstractmethod
///
///
/// class Foo(ABC):
///     @abstractmethod
///     def method(self): ...
/// ```
///
/// ## References
/// - [Python documentation: abc](https://docs.python.org/3/library/abc.html)
#[violation]
pub struct EmptyMethodWithoutAbstractDecorator {
    name: String,
}

impl Violation for EmptyMethodWithoutAbstractDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator { name } = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }
}

fn is_abc_class(bases: &[Expr], keywords: &[Keyword], semantic: &SemanticModel) -> bool {
    keywords.iter().any(|keyword| {
        keyword.arg.as_ref().is_some_and(|arg| arg == "metaclass")
            && semantic
                .resolve_qualified_name(&keyword.value)
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["abc", "ABCMeta"])
                })
    }) || bases.iter().any(|base| {
        semantic
            .resolve_qualified_name(base)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["abc", "ABC"]))
    })
}

fn is_empty_body(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match stmt {
        Stmt::Pass(_) => true,
        Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
            matches!(
                value.as_ref(),
                Expr::StringLiteral(_) | Expr::EllipsisLiteral(_)
            )
        }
        _ => false,
    })
}

/// B024
/// B027
pub(crate) fn abstract_base_class(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    arguments: Option<&Arguments>,
    body: &[Stmt],
) {
    let Some(Arguments { args, keywords, .. }) = arguments else {
        return;
    };

    if args.len() + keywords.len() != 1 {
        return;
    }
    if !is_abc_class(args, keywords, checker.semantic()) {
        return;
    }

    let mut has_abstract_method = false;
    for stmt in body {
        // https://github.com/PyCQA/flake8-bugbear/issues/293
        // If an ABC declares an attribute by providing a type annotation
        // but does not actually assign a value for that attribute,
        // assume it is intended to be an "abstract attribute"
        if matches!(
            stmt,
            Stmt::AnnAssign(ast::StmtAnnAssign { value: None, .. })
        ) {
            has_abstract_method = true;
            continue;
        }

        let Stmt::FunctionDef(ast::StmtFunctionDef {
            decorator_list,
            body,
            name: method_name,
            ..
        }) = stmt
        else {
            continue;
        };

        let has_abstract_decorator = is_abstract(decorator_list, checker.semantic());
        has_abstract_method |= has_abstract_decorator;

        if !checker.enabled(Rule::EmptyMethodWithoutAbstractDecorator) {
            continue;
        }

        if !has_abstract_decorator
            && is_empty_body(body)
            && !is_overload(decorator_list, checker.semantic())
        {
            checker.diagnostics.push(Diagnostic::new(
                EmptyMethodWithoutAbstractDecorator {
                    name: format!("{name}.{method_name}"),
                },
                stmt.range(),
            ));
        }
    }
    if checker.enabled(Rule::AbstractBaseClassWithoutAbstractMethod) {
        if !has_abstract_method {
            checker.diagnostics.push(Diagnostic::new(
                AbstractBaseClassWithoutAbstractMethod {
                    name: name.to_string(),
                },
                stmt.identifier(),
            ));
        }
    }
}
