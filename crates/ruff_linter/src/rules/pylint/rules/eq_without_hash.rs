use std::ops::BitOr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Expr, ExprName, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtFunctionDef,
};
use ruff_python_semantic::analyze::class::any_single_stmt;
use ruff_text_size::Ranged;

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
pub(crate) fn object_without_hash_method(checker: &mut Checker, name: &Identifier, body: &[Stmt]) {
    if matches!(has_eq_hash(body), (HasMethod::Yes | HasMethod::Maybe, HasMethod::No)) {
        let diagnostic = Diagnostic::new(EqWithoutHash, name.range());
        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, is_macro::Is)]
enum HasMethod {
    /// There is no assignment or declaration.
    No,
    /// The assignment or declaration is placed directly within the class body.
    Yes,
    /// The assignment or declaration is placed within an intermediate block
    /// (`if`/`elif`/`else`, `for`/`else`, `while`/`else`, `with`, `case`, `try`/`except`).
    Maybe,
}

impl BitOr for HasMethod {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (HasMethod::No, _) => rhs,
            (_, _) => self,
        }
    }
}

type HasEq = HasMethod;
type HasHash = HasMethod;

fn has_eq_hash(body: &[Stmt]) -> (HasEq, HasHash) {
    let (mut has_eq, mut has_hash) = (HasMethod::No, HasMethod::No);

    any_single_stmt(body, &mut |stmt, nested| {
        let likeliness = if nested {
            HasMethod::Maybe
        } else {
            HasMethod::Yes
        };

        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                name: Identifier { id, .. },
                ..
            }) => match id.as_str() {
                "__eq__" => has_eq = has_eq | likeliness,
                "__hash__" => has_hash = has_hash | likeliness,
                _ => {}
            },

            Stmt::Assign(StmtAssign { targets, .. }) => {
                let [Expr::Name(ExprName { id, .. })] = &targets[..] else {
                    return false;
                };

                match id.as_str() {
                    "__eq__" => has_eq = has_eq | likeliness,
                    "__hash__" => has_hash = has_hash | likeliness,
                    _ => {}
                }
            }

            Stmt::AnnAssign(StmtAnnAssign { target, .. }) => {
                let Expr::Name(ExprName { id, .. }) = target.as_ref() else {
                    return false;
                };

                match id.as_str() {
                    "__eq__" => has_eq = has_eq | likeliness,
                    "__hash__" => has_hash = has_hash | likeliness,
                    _ => {}
                }
            }

            _ => {
                // Technically, a method can be defined using a few more methods:
                //
                // ```python
                // class C1:
                //     # Import
                //     import __eq__  # Callable module
                //     # ImportFrom
                //     from module import __eq__  # Top level callable
                //     # ExprNamed
                //     (__eq__ := lambda self, other: True)
                // ```
                //
                // Those cases are not covered here due to their extreme rarity.
            }
        };

        !has_eq.is_no() && !has_hash.is_no()
    });

    (has_eq, has_hash)
}
