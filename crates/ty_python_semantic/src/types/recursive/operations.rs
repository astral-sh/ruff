//! Deferred operations on closed inference references.

use ruff_python_ast::ExprContext;
use ty_python_core::EvaluationMode;

use crate::types::iteration::IterationProjection;

use super::RecursiveType;
use crate::types::{KnownClass, PromotionKind, PromotionMode, Type, TypeContext, TypeMapping};
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
    Iterate(EvaluationMode, IterationProjection),
}

impl<'db> RecursiveOperation<'db> {
    /// Whether this operation leaves its input unchanged, using its stored operation history
    /// or known type. Inference references are not solved to obtain their input shape.
    pub(in crate::types) fn is_identity_for(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        input: Type<'db>,
    ) -> bool {
        match (self, input) {
            (Self::Promote(_, _), Type::Recursive(recursive)) => {
                recursive.operations(db).is_some_and(|operations| {
                    operations
                        .steps(db)
                        .iter()
                        .rev()
                        .take_while(|previous| !previous.invalidates(self))
                        .any(|previous| *previous == self)
                })
            }
            (Self::Subscript(index), _) => {
                let Type::NominalInstance(index) = index else {
                    return false;
                };
                let Some(slice) = index.slice_literal(db) else {
                    return false;
                };
                if !matches!(slice.start, None | Some(0))
                    || slice.stop.is_some()
                    || !matches!(slice.step, None | Some(1))
                    || matches!(input, Type::Recursive(recursive) if recursive.inference_key(db).is_some())
                {
                    return false;
                }
                let input = match input.resolve_type_alias(db) {
                    Type::Union(union) => union.expand_aliases(db, env),
                    ty => ty,
                };
                let elements = match input {
                    Type::Union(union) => union.elements(db),
                    _ => std::slice::from_ref(&input),
                };
                // User-defined classes, including subclasses, can change the type in `__getitem__`.
                elements.iter().all(|ty| match ty {
                    Type::NominalInstance(instance) => matches!(
                        instance.known_class(db),
                        Some(
                            KnownClass::Tuple
                                | KnownClass::List
                                | KnownClass::Str
                                | KnownClass::Bytes
                                | KnownClass::Bytearray
                                | KnownClass::Range
                                | KnownClass::Memoryview
                        )
                    ),
                    Type::LiteralValue(literal) => {
                        literal.is_string() || literal.is_bytes() || literal.is_literal_string()
                    }
                    _ => false,
                })
            }
            _ => false,
        }
    }

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
                RecursiveOperation::Iterate(mode, projection) => {
                    projection.apply(db, env, ty, mode)
                }
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
        env: &ProgramEnvironment<'db>,
        step: RecursiveOperation<'db>,
    ) -> Self {
        if step.is_identity_for(db, env, Type::Recursive(self)) {
            return self;
        }
        let mut steps = self
            .operations(db)
            .map_or_else(Vec::new, |operations| operations.steps(db).to_vec());
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
