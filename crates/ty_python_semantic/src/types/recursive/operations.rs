//! Deferred operations on closed inference references.

use ruff_python_ast::ExprContext;

use super::RecursiveType;
use crate::types::{PromotionKind, PromotionMode, Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

/// Operations apply to the referenced type in order, without changing its defining query.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveOperations<'db> {
    #[returns(ref)]
    steps: Box<[RecursiveOperation<'db>]>,
}

impl get_size2::GetSize for RecursiveOperations<'_> {}

/// A deferred operation with the same parameters as its immediate type operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursiveOperation<'db> {
    Promote(PromotionMode, PromotionKind),
    Subscript(Type<'db>),
}

impl RecursiveOperation<'_> {
    /// Whether this step can make an earlier operation effective again.
    /// Unknown combinations reset the earlier operation's idempotence guarantee.
    fn invalidates(self, earlier: Self) -> bool {
        match (self, earlier) {
            (Self::Promote(mode, kind), Self::Promote(earlier_mode, earlier_kind))
                if mode == earlier_mode =>
            {
                // Regular promotion introduces no class literals. Singleton promotion
                // introduces only unions with Unknown, which regular promotion traverses.
                // Class promotion can expose default type arguments, so the reverse
                // ordering must still allow regular promotion to run again.
                kind != earlier_kind
                    && !matches!(
                        (kind, earlier_kind),
                        (PromotionKind::Regular, PromotionKind::ClassLiteralsOnly)
                            | (PromotionKind::SingletonsOnly, PromotionKind::Regular)
                    )
            }
            _ => true,
        }
    }
}

impl<'db> RecursiveOperations<'db> {
    /// Apply each step, deferring it again when the body contains query references.
    pub(super) fn apply(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut ty: Type<'db>,
    ) -> Type<'db> {
        for step in self.steps(db) {
            ty = match *step {
                RecursiveOperation::Promote(mode, kind) => ty.apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::Promote(mode, kind),
                    TypeContext::default(),
                ),
                RecursiveOperation::Subscript(index) => ty
                    .subscript_impl(db, env, index, ExprContext::Load)
                    .unwrap_or_else(|error| error.result_type()),
            };
        }
        ty
    }
}

impl<'db> RecursiveType<'db> {
    /// Defer an operation while retaining the input query and all earlier operations.
    pub(in crate::types) fn with_operation(
        self,
        db: &'db dyn Db,
        step: RecursiveOperation<'db>,
    ) -> Self {
        let mut steps = self
            .operations(db)
            .map_or_else(Vec::new, |operations| operations.steps(db).to_vec());
        if steps
            .iter()
            .rev()
            .take_while(|previous| !previous.invalidates(step))
            .any(|previous| *previous == step)
        {
            return self;
        }
        steps.push(step);
        Self::new_internal(
            db,
            self.origin(db),
            self.graph(db),
            self.entry(db),
            self.arguments(db),
            self.materialization_kind(db),
            Some(RecursiveOperations::new(db, steps.into_boxed_slice())),
        )
    }
}
