use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_semantic::analyze::visibility::{abstract_decorator, AbstractDecoratorKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for methods decorated with at least one of `abc`'s
/// `abstractmethod`, `abstractclassmethod`, `abstractstaticmethod` and `abstractproperty`
/// but defined within a normal class's body.
///
/// ## Why is this bad?
/// Such abstract method decorators are only effective in an abstract class.
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

    let Some((_, decorator_kind)) = abstract_decorator(decorator_list, checker.semantic()) else {
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
