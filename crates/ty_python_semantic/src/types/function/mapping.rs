//! Function payload mapping shared by ordinary and controlled execution.

use std::alloc::Layout;
use std::convert::Infallible;
use std::mem;

use salsa::execution_probe::FieldRequest;

use super::{FunctionLiteral, FunctionType, UpdatedFunctionSignatures};
use crate::Db;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::typevar::BoundTypeVarInstance;
use crate::types::{ApplySpecialization, ApplyTypeMappingVisitor, KnownBoundMethodType, Type, TypeContext, TypeMapping, UnionType};

/// Supplies stored fields, effective signatures, nested mapping and canonical function interning.
pub(in crate::types) trait FunctionMappingEffects<'db> {
    type Error;

    /// Admits local work and storage before invoking an action, including its factory and result
    /// carriers. A missing quote denotes overflow. Captured owners must survive refusal drainage.
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error>;

    /// Borrows a callable's signatures after admitting construction of the field request.
    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error>;

    /// Reads one ParamSpec substitution without mapping the replacement or changing its attributes.
    async fn paramspec_substitution(
        &self,
        db: &'db dyn Db,
        specialization: &ApplySpecialization<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    /// Wraps the effective last-definition signature in one regular callable when no stored
    /// implementation callables exist, funding the signature clone and canonical construction.
    async fn implementation_callable(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;

    /// Maps a public signature and prepays construction and retirement of its owned result.
    async fn map_signature(
        &self,
        db: &'db dyn Db,
        signature: &CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableSignature<'db>, Self::Error>;

    async fn map_callable(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableType<'db>, Self::Error>;

    /// Interns the admitted payload, funding the interner's future and owned input transfers.
    async fn intern_function(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> Result<FunctionType<'db>, Self::Error>;
}

/// Visits overloads in stored order to find ParamSpecs whose substitutions are unions.
/// It borrows the selected signature payload; candidate deduplication belongs to the caller.
#[derive(Debug)]
pub(in crate::types) struct UnionParamSpecCandidates<'db> {
    signatures: Option<std::slice::Iter<'db, Signature<'db>>>,
}

impl<'db> UnionParamSpecCandidates<'db> {
    /// Returns the next union-valued ParamSpec, skipping overloads without such a substitution.
    /// The caller can process this candidate before any later overload's substitution is read.
    pub(in crate::types) async fn next_with<E: FunctionMappingEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        specialization: &ApplySpecialization<'_, 'db>,
        effects: &E,
    ) -> Result<Option<(BoundTypeVarInstance<'db>, UnionType<'db>)>, E::Error> {
        loop {
            let variable = effects.local(Some(12), Some(0), || {
                self.signatures.as_mut().and_then(Iterator::next).map(|signature| {
                    signature.parameters().as_paramspec_with_prefix().map(|(_, variable)| variable)
                })
            }).await?;
            let Some(variable) = variable else {
                return Ok(None);
            };
            let Some(variable) = variable else {
                continue;
            };
            let replacement = effects.paramspec_substitution(db, specialization, variable).await?;
            let candidate = effects.local(Some(3), Some(0), || {
                match replacement {
                    Some(Type::Union(union)) => Some((variable, union)),
                    _ => None,
                }
            }).await?;
            if candidate.is_some() {
                return Ok(candidate);
            }
        }
    }
}

/// Selects the callable signatures inspected before union-valued ParamSpec expansion.
/// Lazy specializations inspect stored function updates without inferring an absent signature;
/// other specializations request the effective signature, using canonical inference when needed.
pub(in crate::types) async fn union_paramspec_candidates_with<'db, E: FunctionMappingEffects<'db>>(
    db: &'db dyn Db,
    ty: Type<'db>,
    specialization: &ApplySpecialization<'_, 'db>,
    effects: &E,
) -> Result<UnionParamSpecCandidates<'db>, E::Error> {
    let signatures = match ty {
        Type::FunctionLiteral(function) => {
            paramspec_function_signature_with(db, function, specialization, effects).await?
        }
        Type::BoundMethod(method)
        | Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(method)) => {
            let request = effects.local(Some(2), Some(0), || method.field_requests(db).func()).await?;
            let function = effects.field(request).await?;
            let function = effects.local(Some(1), Some(0), || function.as_function_literal()).await?;
            match function {
                Some(function) => paramspec_function_signature_with(db, function, specialization, effects).await?,
                None => {
                    let request = effects.local(Some(2), Some(0), || method.field_requests(db).func()).await?;
                    let callable = effects.field(request).await?;
                    let callable = effects.local(Some(1), Some(0), || callable.as_callable()).await?;
                    match callable {
                        Some(callable) => {
                            Some(effects.callable_signatures(db, callable).await?)
                        }
                        None => None,
                    }
                }
            }
        }
        Type::Callable(callable) => {
            Some(effects.callable_signatures(db, callable).await?)
        }
        _ => None,
    };
    effects.local(Some(2), Some(0), || UnionParamSpecCandidates {
        signatures: signatures.map(CallableSignature::iter),
    }).await
}

/// Borrows a stored or effective function signature according to specialization laziness.
async fn paramspec_function_signature_with<'db, E: FunctionMappingEffects<'db>>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    specialization: &ApplySpecialization<'_, 'db>,
    effects: &E,
) -> Result<Option<&'db CallableSignature<'db>>, E::Error> {
    let stored_only = effects.local(Some(1), Some(0), || specialization.preserves_lazy_signatures()).await?;
    if stored_only {
        let request = effects.local(Some(2), Some(0), || function.field_requests(db).updated_signatures()).await?;
        let stored = effects.field(request).await?;
        effects.local(Some(2), Some(0), || match stored {
            Some(updated) => match &updated.signature {
                Some(signature) => Some(signature),
                None => None,
            },
            None => None,
        }).await
    } else {
        Ok(Some(effects.signature(db, function).await?))
    }
}

/// Maps function signatures while preserving the literal and descriptor override. Structural
/// mappings and lazy specialization visit only stored updates; other mappings first obtain the
/// effective public signature, then map any separate implementation callables in order.
pub(in crate::types) async fn map_function_with<'db, E: FunctionMappingEffects<'db>>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<FunctionType<'db>, E::Error> {
    effects
        .local(
            Some(8),
            size_of::<Option<CallableSignature<'db>>>()
                .checked_add(size_of::<Option<Box<[CallableType<'db>]>>>())
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<Result<FunctionType<'db>, E::Error>>())
                }),
            || (),
        )
        .await?;
    let literal = effects.field(function.field_requests(db).literal()).await?;
    // Returned-callable rescoping and type-alias specialization should not rebuild signatures from the
    // function literal; doing so can re-enter recursive `TypeOf` evaluation.
    let stored_only = effects
        .local(Some(4), Some(0), || {
            mapping.is_structural()
                || matches!(
                    mapping,
                    TypeMapping::ApplySpecialization(specialization)
                        | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. }
                        if specialization.preserves_lazy_signatures()
                )
        })
        .await?;
    let signature = if stored_only {
        let stored = effects
            .field(function.field_requests(db).updated_signatures())
            .await?;
        effects
            .local(Some(2), Some(0), || match stored {
                Some(updated) => match &updated.signature {
                    Some(signature) => Some(signature),
                    None => None,
                },
                None => None,
            })
            .await?
    } else {
        Some(effects.signature(db, function).await?)
    };
    let mut updated_signature = if let Some(signature) = signature {
        Some(
            effects
                .map_signature(db, signature, mapping, tcx, visitor)
                .await?,
        )
    } else {
        None
    };

    let mut implementation_callables = if stored_only
        || effects.has_separate_implementation(db, literal).await?
    {
        let stored = effects
            .field(function.field_requests(db).updated_signatures())
            .await?;
        let callables = effects
            .local(Some(2), Some(0), || match stored {
                Some(updated) => updated.implementation_callables.as_deref(),
                None => None,
            })
            .await?;
        if let Some(callables) = callables {
            Some(
                map_implementation_callables_with(db, callables, mapping, tcx, visitor, effects)
                    .await?,
            )
        } else if stored_only {
            None
        } else {
            let callable = effects.implementation_callable(db, function).await?;
            let callables = effects.local(Some(1), Some(0), || [callable]).await?;
            Some(
                map_implementation_callables_with(db, &callables, mapping, tcx, visitor, effects)
                    .await?,
            )
        }
    } else {
        None
    };

    if effects
        .local(Some(2), Some(0), || {
            updated_signature.is_none() && implementation_callables.is_none()
        })
        .await?
    {
        return Ok(function);
    }

    // The mapped signature and callable slice already include their construction and retirement
    // costs. Only the outer payload box is new, and both owners stay outside the admission action.
    let updated = effects
        .local(
            Some(8),
            Some(size_of::<UpdatedFunctionSignatures<'db>>()),
            || {
                UpdatedFunctionSignatures::new(
                    updated_signature.take(),
                    implementation_callables.take(),
                )
            },
        )
        .await?;
    let descriptor_kind = effects
        .field(function.field_requests(db).descriptor_kind())
        .await?;
    effects
        .intern_function(db, literal, updated, descriptor_kind)
        .await
}

/// Maps implementation callables in their stored order, retaining the partial buffer across
/// each child mapping. Buffer allocation and cleanup are admitted before the first entry.
async fn map_implementation_callables_with<'db, E: FunctionMappingEffects<'db>>(
    db: &'db dyn Db,
    callables: &[CallableType<'db>],
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<Box<[CallableType<'db>]>, E::Error> {
    let length = effects.local(Some(1), Some(0), || callables.len()).await?;
    let bytes = Layout::array::<CallableType<'db>>(length)
        .ok()
        .map(|layout| layout.size());
    let mut mapped = effects
        .local(length.checked_add(4), bytes, || Vec::with_capacity(length))
        .await?;
    let mut cursor = effects.local(Some(1), Some(0), || callables.iter()).await?;
    while let Some(callable) = effects
        .local(Some(2), Some(0), || cursor.next().copied())
        .await?
    {
        let callable = effects
            .map_callable(db, callable, mapping, tcx, visitor)
            .await?;
        effects
            .local(Some(3), Some(0), || mapped.push(callable))
            .await?;
    }
    // Converting a vector to a boxed slice may shrink its allocation. Fund a possible relocation
    // before moving the buffer; the closure only borrows the owner until admission succeeds.
    effects
        .local(
            length.checked_mul(2).and_then(|work| work.checked_add(4)),
            bytes,
            || mem::take(&mut mapped).into_boxed_slice(),
        )
        .await
}

/// Executes function mapping inline through the existing ordinary signature and callable paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct InlineFunctionMappingEffects;

impl<'db> FunctionMappingEffects<'db> for InlineFunctionMappingEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(function.signature(db))
    }

    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(callable.signatures(db))
    }

    async fn paramspec_substitution(
        &self,
        db: &'db dyn Db,
        specialization: &ApplySpecialization<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(specialization.get(db, variable))
    }

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(literal.has_separate_implementation(db))
    }

    async fn implementation_callable(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(CallableType::single(
            db,
            function.last_definition_signature(db).clone(),
        ))
    }

    async fn map_signature(
        &self,
        db: &'db dyn Db,
        signature: &CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableSignature<'db>, Self::Error> {
        Ok(signature.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    async fn map_callable(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(callable.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    async fn intern_function(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Ok(FunctionType::new_internal(
            db,
            literal,
            updated,
            descriptor_kind,
        ))
    }
}
