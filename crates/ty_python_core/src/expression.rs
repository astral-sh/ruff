use crate::ast_node_ref::AstNodeRef;
use crate::db::Db;
use crate::scope::ScopeId;
use crate::{Program, ProgramFile};
use ruff_db::PythonFile;
use ruff_db::files::File;
use ruff_python_ast as ast;
use salsa;

/// Whether evaluation produces a result object or chooses a control-flow path.
///
/// The semantic-index builder uses this context when creating reachability predicates. The
/// redundant-condition checker uses it to choose between short-circuit analysis and testing
/// the resulting value type.
///
/// In `Value` context, the enclosing code receives the expression's result object. For example,
/// `result = x and y` produces `x` if `x` is falsy, or `y` otherwise. This also applies to expressions
/// that return `bool`: the comparison in `result = x > 0` has value context.
///
/// In `Condition` context, the enclosing code only needs to know which branch to take. For example,
/// CPython evaluates `if x and y` by testing `x` and, only if `x` is truthy, testing `y`. If `x` tests
/// falsy, that one truthiness check is enough to skip the body: `x` is not tested again as the
/// result of `x and y`.
///
/// This distinction matters when an operand's `__bool__` can change between calls:
///
/// ```python
/// if x and False:      # A falsy x skips the body; a truthy x reaches False.
///     ...              # Unreachable in either case.
/// saved = x and False  # Can produce x after checking that it is falsy.
/// if saved:            # Can call x.__bool__ again, which may now return True.
///     ...              # Reachable.
/// ```
///
/// Chained comparisons have the same distinction. Consider a class whose comparison method has
/// an `object` return type:
///
/// ```py
/// from typing_extensions import reveal_type
///
///
/// class Comparable:
///     def __lt__(self, other: int) -> object: ...
///
///
/// def check(value: Comparable):
///     reveal_type(value < 1 < 0)  # revealed: ~AlwaysTruthy
///
///     if value < 1 < 0:  # error: [redundant-condition-strict] "always false"
///         pass
/// ```
///
/// Outside the context of an `if` test, the revealed type of the condition here is `~AlwaysTruthy`:
/// in other words, ty knows that this expression is not *always true*, but cannot guarantee that it is
/// definitely *always false*. It could be an object that is sometimes true and sometimes false -- for
/// example, a `list` (which is falsy when it is empty, and truthy otherwise).
///
/// Nonetheless, when `value < 1 < 0` is used directly as a condition, ty knows that the condition will
/// always be falsy and the `if` branch will never be taken. Python tests the truthiness of the object
/// returned by `Comparable.__lt__` once: if it is falsy, the condition fails immediately. If it is
/// truthy, Python evaluates `1 < 0`, which is false. There is no second truthiness test of the object
/// returned by `__lt__`.
///
/// If the chained comparison is saved as a variable first, its value can be the object returned by
/// `__lt__`, if that object was falsy when first tested. The `if result` statement then tests that
/// object's truthiness again. A user-defined `__bool__` method can return a different result on that
/// second call, so ty cannot guarantee that the saved value is still falsy, and no diagnostic is
/// emitted:
///
/// ```py
/// def check_saved(value: Comparable):
///     result = value < 1 < 0
///     if result:  # no diagnostic
///         pass
/// ```
///
/// The context propagates through `and`, `or`, `not`, and the branches of conditional expressions.
/// Condition context does not propagate through calls or assignment expressions: in
/// `if f(x and False)`, the call's result controls the branch, but its argument is evaluated in
/// value context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpressionContext {
    /// Produce the expression's result object for the enclosing code to use.
    Value,
    /// Choose the truthy or falsy control-flow path without preserving the result object.
    Condition,
}

/// The context used to infer an independently tracked expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum ExpressionKind {
    /// An ordinary value expression, such as `1` in `self.x: int = 1`.
    Normal,
    /// The callable part of a call, such as `list[T]` in `list[T]()`.
    ///
    /// Type variables used to specialize the callable must already be bound. A constructor call
    /// cannot introduce type variable bindings as a generic alias definition can:
    ///
    /// ```python
    /// from typing import TypeVar
    ///
    /// T = TypeVar("T")
    /// Items = list[T]  # Valid: defines a generic alias.
    /// list[T]()  # Error: no generic context binds T.
    /// ```
    Callee,
    /// An expression interpreted as a type, such as `int` in `self.x: int = 1`.
    TypeExpression,
}

/// An independently type-inferable expression.
///
/// Includes constraint expressions (e.g. if tests) and the RHS of an unpacking assignment.
///
/// ## Module-local type
/// This type should not be used as part of any cross-module API because
/// it holds a reference to the AST node. Range-offset changes
/// then propagate through all usages, and deserialization requires
/// reparsing the entire module.
///
/// E.g. don't use this type in:
///
/// * a return type of a cross-module query
/// * a field of a type that is a return type of a cross-module query
/// * an argument of a cross-module query
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct Expression<'db> {
    /// The scope in which the expression occurs.
    ///
    /// Storing the interned scope avoids retaining the file and file-local scope separately, at
    /// the cost of database lookups when either of those values is needed.
    #[returns(copy)]
    pub scope_id: ScopeId<'db>,

    /// The expression node.
    #[no_eq]
    #[tracked]
    #[returns(ref)]
    pub node_ref: AstNodeRef<ast::Expr>,

    /// An assignment statement, if this expression is immediately used as the rhs of that
    /// assignment.
    ///
    /// (Note that this is the _immediately_ containing assignment — if a complex expression is
    /// assigned to some target, only the outermost expression node has this set. The inner
    /// expressions are used to build up the assignment result, and are not "immediately assigned"
    /// to the target, and so have `None` for this field.)
    #[no_eq]
    #[tracked]
    #[returns(clone)]
    pub assigned_to: Option<AstNodeRef<ast::StmtAssign>>,

    /// The inference context for this expression.
    #[returns(copy)]
    pub kind: ExpressionKind,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Expression<'_> {}

impl<'db> Expression<'db> {
    pub fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.scope_id(db)
    }

    pub fn file(self, db: &'db dyn Db) -> File {
        self.scope_id(db).file(db)
    }

    pub fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.scope_id(db).python_file(db)
    }

    pub fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.scope_id(db).program_file(db)
    }

    pub fn program(self, db: &'db dyn Db) -> Program<'db> {
        self.scope_id(db).program(db)
    }
}
