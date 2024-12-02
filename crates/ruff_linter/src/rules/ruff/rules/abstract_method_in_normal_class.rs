use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_semantic::analyze::visibility::{abstract_decorator_kind, AbstractDecoratorKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for methods decorated with at least one of `abc`'s
/// `abstractmethod`, `abstractclassmethod`, `abstractstaticmethod` and `abstractproperty`
/// but defined within a normal class's body.
///
/// ## Why is this bad?
/// The abstract method decorators prevent users from instantiating abstract classes,
/// or inheriting from abstract classes without implementing all abstract methods
/// by throwing an exception. Such abstract method decorators are only effective
/// in an abstract class.
///
/// For a mixin class, it is not enough that `abc.ABC` is included in the eventual MRO.
/// The mixin class must also inherit directly from `ABC` for the decorators to take effect.
///
/// ```python
/// from abc import ABC, abstractmethod
///
///
/// class Base(ABC):
///     @abstractmethod
///     def hello(self) -> None: ...
///
///     def __repr__(self) -> str:
///         return f"message={self.msg!r}"
///
///
/// class Mixin:  # should be: `Mixin(ABC)`:
///     @abstractmethod
///     def world(self) -> None:
///         self.msg += " goodbye"
///
///
/// class FooBar(Mixin, Base):
///     def __init__(self):
///         self.msg = ""
///
///     def hello(self) -> None:
///         self.msg += "hello"
///
///     # without `Mixin(ABC)`, omitting this does not raise an exception
///     # def world(self) -> None:
///     #     self.msg += " world"
///
///
/// # `ABC` is part of the MRO
/// print(FooBar.mro())  # [FooBar, Mixin, Base, ABC, object]
///
/// fb = FooBar()
/// fb.hello()
/// fb.world()
/// print(str(fb))  # message='hello goodbye'
/// ```
///
/// ## Example
///
/// ```python
/// class C:
///     @abstractmethod
///     def m(self) -> None:
///         pass
/// ```
///
/// Use instead:
///
/// ```python
/// from abc import ABC
///
///
/// class C(ABC):
///     @abstractmethod
///     def m(self) -> None:
///         pass
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AbstractMethodInNormalClass {
    decorator_kind: AbstractDecoratorKind,
    class_name: String,
}

impl Violation for AbstractMethodInNormalClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Abstract method defined in non-abstract class".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let (decorator, class) = (&self.decorator_kind, &self.class_name);

        Some(format!(
            "Remove `@{decorator}` or make `{class}` inherit from `abc.ABC`"
        ))
    }
}

/// RUF044
pub(crate) fn abstract_method_in_normal_class(checker: &mut Checker, class: &StmtClassDef) {
    if checker.source_type.is_stub() {
        return;
    }

    if !class.bases().is_empty() {
        return;
    }

    if let Some(arguments) = &class.arguments {
        if arguments.find_keyword("metaclass").is_some() {
            return;
        }
    }

    let class_name = class.name.as_str();

    for stmt in &class.body {
        check_class_stmt(checker, class_name, stmt);
    }
}

fn check_class_stmt(checker: &mut Checker, class_name: &str, stmt: &Stmt) {
    let Stmt::FunctionDef(StmtFunctionDef {
        decorator_list,
        name,
        ..
    }) = stmt
    else {
        return;
    };

    let Some(decorator_kind) = abstract_decorator_kind(decorator_list, checker.semantic()) else {
        return;
    };

    let class_name = class_name.to_string();
    let kind = AbstractMethodInNormalClass {
        decorator_kind,
        class_name,
    };
    let diagnostic = Diagnostic::new(kind, name.range);

    checker.diagnostics.push(diagnostic);
}
