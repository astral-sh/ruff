use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_semantic::analyze::class::any_base_class;
use ruff_python_semantic::analyze::visibility::{abstract_decorator_kind, AbstractDecoratorKind};
use ruff_python_semantic::{BindingKind, NodeRef, SemanticModel};

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
/// from abc import abstractmethod
///
///
/// class C:
///     @abstractmethod
///     def m(self) -> None:
///         pass
/// ```
///
/// Use instead:
///
/// ```python
/// from abc import ABC, abstractmethod
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

    if might_be_abstract(class, checker.semantic()) {
        return;
    }

    let class_name = class.name.as_str();

    for stmt in &class.body {
        check_class_stmt(checker, class_name, stmt);
    }
}

/// Returns true if a class is definitely not an abstract class.
///
/// A class is considered abstract when it inherits from a class
/// created by `abc.ABCMeta` without implementing all abstract methods.
///
/// Thus, a class is *not* abstract when all of its bases are inspectable
/// and none of them inherits from `abc.ABC` or has `abc.ABCMeta` as the metaclass.
fn might_be_abstract(class: &StmtClassDef, semantic: &SemanticModel) -> bool {
    any_base_class(class, semantic, &mut |base| {
        if is_abc(base, semantic) {
            // `abc.ABC` is an explicit base
            // -> Might be abstract
            return true;
        }

        let Some(base_def) = find_class_def(base, semantic) else {
            // Class definition is dynamic or not presented in the same file
            // -> Might be abstract
            return true;
        };

        let Some(arguments) = base_def.arguments.as_ref() else {
            // No extra arguments
            // -> Continue processing
            return false;
        };
        let Some(metaclass) = arguments.find_keyword("metaclass") else {
            // No metaclass
            // -> Continue processing
            return false;
        };
        let metaclass = &metaclass.value;

        if is_abcmeta(metaclass, semantic) {
            // Metaclass is `abc.ABCMeta`
            // -> Might be abstract
            return true;
        }

        let Some(metaclass_def) = find_class_def(metaclass, semantic) else {
            // Has metaclass but its definition is not found
            // -> Might be abstract
            return true;
        };

        any_base_class(metaclass_def, semantic, &mut |base| {
            is_abcmeta(base, semantic)
        })
    })
}

fn is_abc(base: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(base) else {
        return false;
    };

    matches!(qualified_name.segments(), ["abc", "ABC"])
}

fn is_abcmeta(base: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(base) else {
        return false;
    };

    matches!(qualified_name.segments(), ["abc", "ABCMeta"])
}

fn find_class_def<'a>(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a StmtClassDef> {
    let name = expr.as_name_expr()?;
    let binding_id = semantic.only_binding(name)?;

    let binding = semantic.binding(binding_id);

    if !matches!(binding.kind, BindingKind::ClassDefinition(_)) {
        return None;
    }

    let node_id = binding.source?;
    let node = semantic.node(node_id);

    let NodeRef::Stmt(Stmt::ClassDef(base_def)) = node else {
        return None;
    };

    Some(base_def)
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
