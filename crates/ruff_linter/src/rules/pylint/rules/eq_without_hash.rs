use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes that implement `__eq__` but not `__hash__`.
///
/// ## Why is this bad?
/// A class that implements `__eq__` but not `__hash__` will have its hash
/// method implicitly set to `None`, regardless of if a super class defines
/// `__hash__`. This will cause the class to be unhashable, will in turn
/// cause issues when using the class as a key in a dictionary or a member
/// of a set.
///
/// ## Example
///
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
///
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
///
/// ### When inheriting
///
/// The mere presence of an `__eq__` function will cause the classes `__hash__` to be `None` and must be explicitly
/// implemented:
///
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
///
///
/// class Developer(Person):
///     def __init__(self):
///         super().__init__()
///         self.language = "python"
///
///     def __eq__(self, other):
///         return (
///             super().__eq__(other)
///             and isinstance(other, Developer)
///             and self.language == other.language
///         )
///
///
/// hash(Developer())  # TypeError: unhashable type: 'Developer'
/// ```
///
/// It is idiomatic to write a hash function using a tuple of members required for the `__eq__` function to operate:
///
/// ```python
/// class Developer(Person):
///     ...
///     def __hash__(self):
///         return hash((self.name, self.other))
/// ```
///
/// ### When the hash logic is identical
///
/// ```python
/// class Derived(Base):
///     def __eq__(self, other): ...
///
///     def __hash__(self):
///         return super().__hash__()
/// ```
///
/// or:
///
/// ```python
/// class Derived(Base):
///     def __eq__(self, other): ...
///
///     __hash__ = Base.__hash__
/// ```
/// ## References
/// - [Python documentation: `object.__hash__`](https://docs.python.org/3/reference/datamodel.html#object.__hash__)
#[derive(ViolationMetadata)]
pub(crate) struct EqWithoutHash;

impl Violation for EqWithoutHash {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Object does not implement `__hash__` method".to_string()
    }
}

/// W1641
pub(crate) fn object_without_hash_method(
    checker: &mut Checker,
    ast::StmtClassDef { name, body, .. }: &ast::StmtClassDef,
) {
    if has_eq_without_hash(body) {
        checker
            .diagnostics
            .push(Diagnostic::new(EqWithoutHash, name.range()));
    }
}

fn has_eq_without_hash(body: &[Stmt]) -> bool {
    let mut has_hash = false;
    let mut has_eq = false;
    for statement in body {
        match statement {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                // Check if `__hash__` was explicitly set, as in:
                // ```python
                // class Class(SuperClass):
                //     def __eq__(self, other):
                //         return True
                //
                //     __hash__ = SuperClass.__hash__
                // ```

                if id == "__hash__" {
                    has_hash = true;
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => match name.as_str() {
                "__hash__" => has_hash = true,
                "__eq__" => has_eq = true,
                _ => {}
            },
            _ => {}
        }
    }
    has_eq && !has_hash
}
