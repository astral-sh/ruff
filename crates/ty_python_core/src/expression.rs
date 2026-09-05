use crate::ast_node_ref::AstNodeRef;
use crate::db::Db;
use crate::scope::ScopeId;
use crate::{Program, ProgramFile};
use ruff_db::PythonFile;
use ruff_db::files::File;
use ruff_python_ast as ast;
use salsa;

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
