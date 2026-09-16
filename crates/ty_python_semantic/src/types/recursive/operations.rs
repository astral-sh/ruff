//! Deferred operations retain every operand, including query references in different positions.

use ruff_python_ast::ExprContext;
use ty_python_core::EvaluationMode;

use super::InferenceKey;
use crate::types::iteration::IterationProjection;
use crate::types::{KnownClass, PromotionKind, PromotionMode, Type, TypeContext, TypeMapping};
use crate::{Db, Program, ProgramEnvironment};

/// A shared expression node. Its identity includes all inputs, never their provisional solutions.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveOperationNode<'db> {
    #[returns(copy)]
    program: Program<'db>,
    #[returns(copy)]
    pub(super) operation: RecursiveOperation<'db>,
}

impl get_size2::GetSize for RecursiveOperationNode<'_> {}

/// The complete inputs of an operation that can be retained in an inference equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursiveOperation<'db> {
    Promote {
        operand: Type<'db>,
        mode: PromotionMode,
        kind: PromotionKind,
    },
    Subscript {
        value: Type<'db>,
        index: Type<'db>,
    },
    Iterate {
        operand: Type<'db>,
        mode: EvaluationMode,
        projection: IterationProjection,
    },
}

impl<'db> RecursiveOperation<'db> {
    pub(super) fn operands(self) -> impl Iterator<Item = Type<'db>> {
        match self {
            Self::Promote { operand, .. } | Self::Iterate { operand, .. } => [Some(operand), None],
            Self::Subscript { value, index } => [Some(value), Some(index)],
        }
        .into_iter()
        .flatten()
    }

    /// Substitute inputs without evaluating the expression or introducing a recursive binder.
    pub(super) fn map_operands(self, mut map: impl FnMut(Type<'db>) -> Type<'db>) -> Self {
        match self {
            Self::Promote {
                operand,
                mode,
                kind,
            } => Self::Promote {
                operand: map(operand),
                mode,
                kind,
            },
            Self::Subscript { value, index } => Self::Subscript {
                value: map(value),
                index: map(index),
            },
            Self::Iterate {
                operand,
                mode,
                projection,
            } => Self::Iterate {
                operand: map(operand),
                mode,
                projection,
            },
        }
    }

    /// Retain the operation when any input needs a query equation to expose its outer shape.
    pub(in crate::types) fn deferred(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        if !self.operands().any(
            |ty| matches!(ty, Type::Recursive(recursive) if recursive.inference_key(db).is_some()),
        ) {
            return None;
        }
        if self.is_identity(db, env) {
            return self.operands().next();
        }
        Some(
            InferenceKey::Operation(RecursiveOperationNode::new(db, env.program(db), self))
                .reference(db),
        )
    }

    /// Recognize identities from stored expressions or known types without solving inputs.
    pub(in crate::types) fn is_identity(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        match self {
            Self::Promote { mut operand, .. } => {
                while let Type::Recursive(recursive) = operand
                    && let Some(InferenceKey::Operation(node)) = recursive.inference_key(db)
                {
                    let earlier = node.operation(db);
                    if self.same_promotion(earlier) {
                        return true;
                    }
                    if earlier.invalidates(self) {
                        break;
                    }
                    let Self::Promote { operand: input, .. } = earlier else {
                        break;
                    };
                    operand = input;
                }
                false
            }
            Self::Subscript {
                value: input,
                index,
            } => {
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
            Self::Iterate { .. } => false,
        }
    }

    fn same_promotion(self, other: Self) -> bool {
        matches!((self, other),
            (Self::Promote { mode, kind, .. }, Self::Promote { mode: other_mode, kind: other_kind, .. })
                if mode == other_mode && kind == other_kind)
    }

    /// Whether this step can make an earlier operation effective again.
    fn invalidates(self, earlier: Self) -> bool {
        match (self, earlier) {
            (
                Self::Promote { mode, kind, .. },
                Self::Promote {
                    mode: earlier_mode,
                    kind: earlier_kind,
                    ..
                },
            ) if mode == earlier_mode => {
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

    pub(super) fn apply(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Self::Promote {
                operand,
                mode,
                kind,
            } => operand.apply_type_mapping(
                db,
                env,
                &TypeMapping::Promote(mode, kind),
                TypeContext::default(),
            ),
            Self::Iterate {
                operand,
                mode,
                projection,
            } => projection.apply(db, env, operand, mode),
            Self::Subscript { value, index } => value
                .subscript_impl(db, env, index, ExprContext::Load)
                .unwrap_or_else(|error| error.result_type()),
        }
    }
}

impl<'db> RecursiveOperationNode<'db> {
    pub(super) fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_program(self.program(db))
    }
}
