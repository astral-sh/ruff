use itertools::Itertools;
use std::ops::BitOr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    ExceptHandler, Expr, ExprName, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtFor,
    StmtFunctionDef, StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith,
};
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
    if matches!(has_eq_hash(body, false), (HasEq::Yes | HasEq::Maybe, HasHash::No)) {
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

fn has_eq_hash(body: &[Stmt], nested: bool) -> (HasEq, HasHash) {
    let mut has_eq = HasMethod::No;
    let mut has_hash = HasMethod::No;

    let likeliness = if nested {
        HasMethod::Maybe
    } else {
        HasMethod::Yes
    };

    for stmt in body {
        if !has_eq.is_no() && !has_hash.is_no() {
            break;
        }

        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                name: Identifier { id, .. },
                ..
            }) => match id.as_str() {
                "__eq__" => has_eq = likeliness,
                "__hash__" => has_hash = likeliness,
                _ => {}
            },

            Stmt::Assign(StmtAssign { targets, .. }) => {
                let [Expr::Name(ExprName { id, .. })] = &targets[..] else {
                    continue;
                };

                match id.as_str() {
                    "__eq__" => has_eq = likeliness,
                    "__hash__" => has_hash = likeliness,
                    _ => {}
                }
            }

            Stmt::AnnAssign(StmtAnnAssign { target, .. }) => {
                let Expr::Name(ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                match id.as_str() {
                    "__eq__" => has_eq = likeliness,
                    "__hash__" => has_hash = likeliness,
                    _ => {}
                }
            }

            Stmt::With(StmtWith { body, .. }) => {
                let bodies = [body];

                (has_eq, has_hash) = any_has_eq_hash(has_eq, has_hash, &bodies[..]);
            }

            Stmt::For(StmtFor { body, orelse, .. })
            | Stmt::While(StmtWhile { body, orelse, .. }) => {
                let bodies = [body, orelse];

                (has_eq, has_hash) = any_has_eq_hash(has_eq, has_hash, &bodies[..]);
            }

            Stmt::If(StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                let mut bodies = vec![body];
                bodies.extend(elif_else_clauses.iter().map(|it| &it.body));

                (has_eq, has_hash) = any_has_eq_hash(has_eq, has_hash, &bodies[..]);
            }

            Stmt::Match(StmtMatch { cases, .. }) => {
                let bodies = cases.iter().map(|it| &it.body).collect_vec();

                (has_eq, has_hash) = any_has_eq_hash(has_eq, has_hash, &bodies[..]);
            }

            Stmt::Try(StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                let mut bodies = vec![body, orelse, finalbody];
                bodies.extend(
                    handlers
                        .iter()
                        .map(|ExceptHandler::ExceptHandler(it)| &it.body),
                );

                (has_eq, has_hash) = any_has_eq_hash(has_eq, has_hash, &bodies[..]);
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
    }

    (has_eq, has_hash)
}

fn any_has_eq_hash(has_eq: HasEq, has_hash: HasHash, bodies: &[&Vec<Stmt>]) -> (HasEq, HasHash) {
    bodies
        .iter()
        .fold((has_eq, has_hash), |(has_eq, has_hash), body| {
            if !has_eq.is_no() && !has_hash.is_no() {
                return (has_eq, has_hash);
            }

            let (body_has_eq, body_has_hash) = has_eq_hash(body, true);

            (has_eq | body_has_eq, has_hash | body_has_hash)
        })
}
