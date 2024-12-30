use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Expr, ExprName, Identifier, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef,
};
use ruff_python_semantic::analyze::class::{
    any_member_declaration, ClassMemberBoundness, ClassMemberKind,
};
use ruff_text_size::Ranged;
use std::ops::BitOr;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes that implement `__eq__` but not `__hash__`.
///
/// ## Why is this bad?
/// A class that implements `__eq__` but not `__hash__` will have its hash
/// method implicitly set to `None`. This will cause the class to be
/// unhashable, will in turn cause issues when using the class as a key in a
/// dictionary or a member of a set.
///
/// ## Known problems
/// Does not check for `__hash__` implementations in superclasses.
///
/// ## Example
/// ```python
/// class Person:
///     def __init__(self):
///         self.name = "monty"
///
///     def __eq__(self, other):
///         return isinstance(other, Person) and other.name == self.name
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     def __init__(self):
///         self.name = "monty"
///
///     def __eq__(self, other):
///         return isinstance(other, Person) and other.name == self.name
///
///     def __hash__(self):
///         return hash(self.name)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct EqWithoutHash;

impl Violation for EqWithoutHash {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Object does not implement `__hash__` method".to_string()
    }
}

/// W1641
pub(crate) fn object_without_hash_method(checker: &mut Checker, class: &StmtClassDef) {
    let eq_hash = EqHash::from_class(class);
    if matches!(
        eq_hash,
        EqHash {
            eq: HasMethod::Yes | HasMethod::Maybe,
            hash: HasMethod::No
        }
    ) {
        let diagnostic = Diagnostic::new(EqWithoutHash, class.name.range());
        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug)]
struct EqHash {
    hash: HasMethod,
    eq: HasMethod,
}

impl EqHash {
    fn from_class(class: &StmtClassDef) -> Self {
        let (mut has_eq, mut has_hash) = (HasMethod::No, HasMethod::No);

        any_member_declaration(class, &mut |declaration| {
            let id = match declaration.kind() {
                ClassMemberKind::Assign(StmtAssign { targets, .. }) => {
                    let [Expr::Name(ExprName { id, .. })] = &targets[..] else {
                        return false;
                    };

                    id
                }
                ClassMemberKind::AnnAssign(StmtAnnAssign { target, .. }) => {
                    let Expr::Name(ExprName { id, .. }) = target.as_ref() else {
                        return false;
                    };

                    id
                }
                ClassMemberKind::FunctionDef(StmtFunctionDef {
                    name: Identifier { id, .. },
                    ..
                }) => id.as_str(),
            };

            match id {
                "__eq__" => has_eq = has_eq | declaration.boundness().into(),
                "__hash__" => has_hash = has_hash | declaration.boundness().into(),
                _ => {}
            }

            !has_eq.is_no() && !has_hash.is_no()
        });

        Self {
            eq: has_eq,
            hash: has_hash,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, is_macro::Is, Default)]
enum HasMethod {
    /// There is no assignment or declaration.
    #[default]
    No,
    /// The assignment or declaration is placed directly within the class body.
    Yes,
    /// The assignment or declaration is placed within an intermediate block
    /// (`if`/`elif`/`else`, `for`/`else`, `while`/`else`, `with`, `case`, `try`/`except`).
    Maybe,
}

impl From<ClassMemberBoundness> for HasMethod {
    fn from(value: ClassMemberBoundness) -> Self {
        match value {
            ClassMemberBoundness::PossiblyUnbound => Self::Maybe,
            ClassMemberBoundness::Bound => Self::Yes,
        }
    }
}

impl BitOr<HasMethod> for HasMethod {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (HasMethod::No, _) => rhs,
            (_, _) => self,
        }
    }
}
