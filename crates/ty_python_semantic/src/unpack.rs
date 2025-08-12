use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};

use crate::Db;
use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::scope::{FileScopeId, ScopeId};

/// This ingredient represents a single unpacking.
///
/// This is required to make use of salsa to cache the complete unpacking of multiple variables
/// involved. It allows us to:
/// 1. Avoid doing structural match multiple times for each definition
/// 2. Avoid highlighting the same error multiple times
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
pub(crate) struct Unpack<'db> {
    pub(crate) file: File,

    pub(crate) value_file_scope: FileScopeId,

    pub(crate) target_file_scope: FileScopeId,

    /// The target expression that is being unpacked. For example, in `(a, b) = (1, 2)`, the target
    /// expression is `(a, b)`.
    #[no_eq]
    #[tracked]
    #[returns(ref)]
    pub(crate) _target: AstNodeRef<ast::Expr>,

    /// The ingredient representing the value expression of the unpacking. For example, in
    /// `(a, b) = (1, 2)`, the value expression is `(1, 2)`.
    pub(crate) value: UnpackValue<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Unpack<'_> {}

impl<'db> Unpack<'db> {
    pub(crate) fn target<'ast>(
        self,
        db: &'db dyn Db,
        parsed: &'ast ParsedModuleRef,
    ) -> &'ast ast::Expr {
        self._target(db).node(parsed)
    }

    /// Returns the scope where the unpack target expression belongs to.
    pub(crate) fn target_scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.target_file_scope(db).to_scope_id(db, self.file(db))
    }

    /// Returns the range of the unpack target expression.
    pub(crate) fn range(self, db: &'db dyn Db, module: &ParsedModuleRef) -> TextRange {
        self.target(db, module).range()
    }
}

/// The expression that is being unpacked.
#[derive(Clone, Copy, Debug, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct UnpackValue<'db> {
    /// The kind of unpack expression
    kind: UnpackKind,
    /// The expression we are unpacking
    expression: Expression<'db>,
}

impl<'db> UnpackValue<'db> {
    pub(crate) fn new(kind: UnpackKind, expression: Expression<'db>) -> Self {
        Self { kind, expression }
    }

    /// Returns the underlying [`Expression`] that is being unpacked.
    pub(crate) const fn expression(self) -> Expression<'db> {
        self.expression
    }

    /// Returns the expression as an [`AnyNodeRef`].
    pub(crate) fn as_any_node_ref<'ast>(
        self,
        db: &'db dyn Db,
        module: &'ast ParsedModuleRef,
    ) -> AnyNodeRef<'ast> {
        self.expression().node_ref(db, module).into()
    }

    pub(crate) const fn kind(self) -> UnpackKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum EvaluationMode {
    Sync,
    Async,
}

impl EvaluationMode {
    pub(crate) const fn from_is_async(is_async: bool) -> Self {
        if is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        }
    }

    pub(crate) const fn is_async(self) -> bool {
        matches!(self, EvaluationMode::Async)
    }
}

#[derive(Clone, Copy, Debug, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum UnpackKind {
    /// An iterable expression like the one in a `for` loop or a comprehension.
    Iterable { mode: EvaluationMode },
    /// An context manager expression like the one in a `with` statement.
    ContextManager { mode: EvaluationMode },
    /// An expression that is being assigned to a target.
    Assign,
}

/// The position of the target element in an unpacking.
#[derive(Clone, Copy, Debug, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) enum UnpackPosition {
    /// The target element is in the first position of the unpacking.
    First,
    /// The target element is in the position other than the first position of the unpacking.
    Other,
}
