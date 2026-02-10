use crate::Db;
use crate::types::{CallableType, NominalInstanceType, NormalizedVisitor, Type, visitor};

/// Represents the result of a `functools.partial(func, ...)` call where we could
/// determine the remaining callable signature after binding some arguments.
///
/// This type carries both the `partial[T]` nominal instance (for attribute access
/// like `.func`, `.args`, `.keywords` and assignability to `partial[T]` return
/// annotations) and the refined callable type (for call-site invocation).
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct PartialCallableType<'db> {
    /// The `partial[T]` nominal instance type (e.g., `partial[bool]`).
    pub(crate) instance: NominalInstanceType<'db>,
    /// The refined callable type after binding some arguments.
    pub(crate) callable: CallableType<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PartialCallableType<'_> {}

pub(super) fn walk_partial_callable_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    partial: PartialCallableType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, Type::NominalInstance(partial.instance(db)));
    visitor.visit_type(db, Type::Callable(partial.callable(db)));
}

impl<'db> PartialCallableType<'db> {
    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.instance(db)
                .normalized_impl(db, visitor)
                .as_nominal_instance()
                .unwrap_or(self.instance(db)),
            self.callable(db).normalized_impl(db, visitor),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.instance(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.callable(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }
}
