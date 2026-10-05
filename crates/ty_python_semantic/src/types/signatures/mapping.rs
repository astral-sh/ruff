//! Maps callable signatures in source order with explicit semantic children and storage admission.

use std::convert::Infallible;

use smallvec::SmallVec;

use super::source::parameters_storage_quote;
use super::{
    CallableSignature, ConcatenateTail, Parameter, ParameterDefault, ParameterKind, Parameters,
    ParametersKind, Signature, SignatureExtras,
};
use crate::Db;
use crate::types::mapping::return_callables::ReturnCallableReplacements;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::generics::GenericContext;
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, MaterializationKind, Type, TypeContext, TypeMapping,
};

/// Supplies nested mapping and finite storage for rebuilding callable signatures.
///
/// A controlled caller admits the complete shared-mapper future and its result before polling it.
/// Its inline child futures and partial overload/parameter buffers belong to that same owner.
/// Semantic children must retain the supplied visitor, environment, and mapping polarity.
pub(in crate::types) trait SignatureMappingEffects<'db> {
    type Error;

    /// Admits `action` and its output before executing it.
    ///
    /// The provider adds the closure, its retained `Option` slot, the output, and the `Result`
    /// carrier to `requested_bytes`. The supplied quotation covers additional allocations and
    /// intermediate values; either `None` refuses before executing the action. The provider keeps
    /// the action outside the rejecting callback, which borrows its slot and takes it only after
    /// admission. This keeps captured partial results alive until queued children have drained.
    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Retains mapped parameters without recollecting their owned buffer.
    /// The provider admits boxing and owner construction; each entry's nested owners already
    /// carry the retirement cost paid when that entry was created.
    async fn finish_parameters(
        &self,
        parameters: Vec<Parameter<'db>>,
        kind: ParametersKind<'db>,
    ) -> Result<Parameters<'db>, Self::Error>;

    async fn new_parameters(&self, capacity: usize) -> Result<Vec<Parameter<'db>>, Self::Error>;

    async fn push_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> Result<(), Self::Error>;

    async fn new_overloads(&self, capacity: usize) -> Result<SmallVec<[Signature<'db>; 1]>, Self::Error>;

    async fn push_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> Result<(), Self::Error>;

    /// Admits a callback that can construct one fixed Box, whose nested fields are already funded.
    async fn box_local<T, U>(&self, action: impl FnOnce() -> U) -> Result<U, Self::Error>;

    /// Admits one parameter-kind reconstruction and its cloned name's retirement.
    async fn parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> Result<ParameterKind<'db>, Self::Error>;

    /// Checks an overload's ParamSpec substitution before mapping its generic context.
    /// [`CallableSignature::apply_type_mapping_impl`] already expands applicable ParamSpecs
    /// for ordinary mapping. A controlled provider refuses an applicable expansion here
    /// if it cannot rebuild the ParamSpec parameters.
    async fn check_paramspec_specialization(
        &self,
        signature: &Signature<'db>,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Result<(), Self::Error>;

    /// Maps one nested type with the caller's retained visitor and the supplied polarity.
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error>;

    /// Applies the mapping's generic-context update; promotion copies the context handle.
    async fn update_generic_context(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericContext<'db>, Self::Error>;

    /// Maps the signature's receiver constraints before mapping its parameters.
    ///
    /// Providers admit ownership and retirement of returned constraints. A provider without
    /// constraint reconstruction support accepts source-free terminals only; all other present
    /// constraints require the ordinary mapping and satisfaction path.
    async fn map_receiver_constraints(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<OwnedConstraintSet<'db>>, Self::Error>;

    /// Expands mapped starred variadic annotations, preserving parameter source positions.
    ///
    /// The shared caller selects this operation from the syntax flag and variadic kind before
    /// any tuple lookup. Providers without expansion support reject at this boundary.
    async fn expand_starred_parameters(
        &self,
        db: &'db dyn Db,
        parameters: &mut Parameters<'db>,
    ) -> Result<(), Self::Error>;

    /// Replaces a callable handle without reading its signatures or metadata.
    /// A missing entry preserves the original callable.
    async fn return_callable_replacement(
        &self,
        replacements: ReturnCallableReplacements<'_, 'db>,
        callable: CallableType<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;

    /// Borrows the canonical signature payload of an interned callable.
    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error>;

    /// Interns mapped signatures while retaining the callable's kind and deprecation metadata.
    ///
    /// The provider admits the interner input and child future, retaining this already funded
    /// signature owner outside rejecting callbacks until any queued children have drained.
    async fn replace_callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;

    /// Maps a callable payload, allowing ordinary specialization to expand a ParamSpec first.
    async fn map_callable_signature(
        &self,
        db: &'db dyn Db,
        signatures: &CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableSignature<'db>, Self::Error>
    where
        Self: Sized,
    {
        map_callable_signature_with(db, signatures, mapping, tcx, visitor, self).await
    }
}

/// Executes signature mapping synchronously through the ordinary semantic operations.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InlineSignatureMappingEffects;

impl<'db> SignatureMappingEffects<'db> for InlineSignatureMappingEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn finish_parameters(
        &self,
        parameters: Vec<Parameter<'db>>,
        kind: ParametersKind<'db>,
    ) -> Result<Parameters<'db>, Self::Error> {
        Ok(Parameters::from_owned(parameters.into_boxed_slice(), kind))
    }

    async fn new_parameters(&self, capacity: usize) -> Result<Vec<Parameter<'db>>, Self::Error> {
        Ok(Vec::with_capacity(capacity))
    }

    async fn push_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> Result<(), Self::Error> {
        parameters.push(parameter);
        Ok(())
    }

    async fn new_overloads(&self, capacity: usize) -> Result<SmallVec<[Signature<'db>; 1]>, Self::Error> {
        Ok(SmallVec::with_capacity(capacity))
    }

    async fn push_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> Result<(), Self::Error> {
        overloads.push(signature);
        Ok(())
    }

    async fn box_local<T, U>(&self, action: impl FnOnce() -> U) -> Result<U, Self::Error> {
        Ok(action())
    }

    async fn parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> Result<ParameterKind<'db>, Self::Error> {
        Ok(action())
    }

    async fn check_paramspec_specialization(
        &self,
        _signature: &Signature<'db>,
        _mapping: &TypeMapping<'_, 'db>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    async fn update_generic_context(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(mapping.update_signature_generic_context(db, visitor.env, context))
    }

    async fn map_receiver_constraints(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<OwnedConstraintSet<'db>>, Self::Error> {
        Ok(signature.map_receiver_constraints(db, mapping, tcx, visitor))
    }

    async fn expand_starred_parameters(
        &self,
        db: &'db dyn Db,
        parameters: &mut Parameters<'db>,
    ) -> Result<(), Self::Error> {
        *parameters = parameters.expand_starred_variadic_annotations(db);
        Ok(())
    }

    async fn return_callable_replacement(
        &self,
        replacements: ReturnCallableReplacements<'_, 'db>,
        callable: CallableType<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(replacements.get(callable).unwrap_or(callable))
    }

    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(callable.signatures(db))
    }

    async fn replace_callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(callable.with_signatures(db, signatures))
    }

    async fn map_callable_signature(
        &self,
        db: &'db dyn Db,
        signatures: &CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableSignature<'db>, Self::Error> {
        Ok(signatures.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
}

/// Maps an interned callable's signatures and preserves its binding and deprecation metadata.
///
/// `RescopeReturnCallables` replaces the callable handle without inspecting or rebuilding
/// its signatures. Other modes map the signatures and preserve callable metadata.
pub(in crate::types) async fn map_callable_type_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    callable: CallableType<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<CallableType<'db>, E::Error> {
    let replacements = effects.local(Some(2), Some(0), || {
        match mapping {
            TypeMapping::RescopeReturnCallables(replacements) => Some(*replacements),
            _ => None,
        }
    }).await?;
    if let Some(replacements) = replacements {
        return effects.return_callable_replacement(replacements, callable).await;
    }
    let signatures = effects.callable_signatures(db, callable).await?;
    let mapped = effects
        .map_callable_signature(db, signatures, mapping, tcx, visitor)
        .await?;
    effects
        .replace_callable_signatures(db, callable, mapped)
        .await
}

/// Maps overloads in their stored order, retaining all source and recovery metadata.
///
/// ParamSpec specialization can replace one overload with several. The ordinary entry point
/// handles that case separately and sends each non-expanding overload through this traversal.
pub(in crate::types) async fn map_callable_signature_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    signatures: &CallableSignature<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<CallableSignature<'db>, E::Error> {
    let count = effects
        .local(Some(1), Some(0), || signatures.overloads.len())
        .await?;
    // One overload is stored inline. Spilled storage is reserved once at the known final length.
    let mut overloads = effects.new_overloads(count).await?;
    let mut input = effects
        .local(Some(1), Some(0), || signatures.overloads.iter())
        .await?;
    while let Some(signature) = effects.local(Some(2), Some(0), || input.next()).await? {
        effects.check_paramspec_specialization(signature, mapping).await?;
        let mapped = map_signature_with(db, signature, mapping, tcx, visitor, effects).await?;
        effects.push_overload(&mut overloads, mapped).await?;
    }
    effects
        .local(Some(2), Some(0), || CallableSignature { overloads })
        .await
}

/// Maps a signature's generic context, constraints, parameters, and return type in that order.
///
/// A ParamSpec value describes only parameters, so its stored return type is preserved.
pub(in crate::types) async fn map_signature_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<Signature<'db>, E::Error> {
    let generic_context = match effects
        .local(Some(1), Some(0), || signature.generic_context)
        .await?
    {
        Some(context) => Some(
            effects
                .update_generic_context(db, context, mapping, visitor)
                .await?,
        ),
        None => None,
    };
    let receiver_constraints = effects
        .map_receiver_constraints(db, signature, mapping, tcx, visitor)
        .await?;
    let source_overload_index = effects
        .local(Some(1), Some(0), || signature.source_overload_index_raw())
        .await?;
    let extras = effects
        .box_local::<SignatureExtras<'db>, _>(|| {
            SignatureExtras::new(source_overload_index, receiver_constraints)
        })
        .await?;
    let parameters =
        map_parameters_with(db, &signature.parameters, mapping, tcx, visitor, effects).await?;
    let return_ty = if effects
        .local(Some(1), Some(0), || signature.is_paramspec_value)
        .await?
    {
        effects
            .local(Some(1), Some(0), || signature.return_ty)
            .await?
    } else {
        let return_ty = effects
            .local(Some(1), Some(0), || signature.return_ty)
            .await?;
        effects
            .map_type(db, return_ty, mapping, tcx, visitor)
            .await?
    };
    effects
        .local(Some(16), Some(0), || Signature {
            generic_context,
            definition: signature.definition,
            extras,
            parameters,
            return_ty,
            is_paramspec_value: signature.is_paramspec_value,
            is_recursion_recovery: signature.is_recursion_recovery,
        })
        .await
}

/// Maps parameter annotations contravariantly, then expands starred variadic annotations.
///
/// The stored parameter-list kind is preserved. Materializing gradual parameters takes the
/// existing top/bottom shortcut before flipping the mapping or visiting any annotation.
pub(in crate::types) async fn map_parameters_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    parameters: &Parameters<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<Parameters<'db>, E::Error> {
    let materialization = effects
        .local(Some(3), Some(0), || {
            if let TypeMapping::Materialize(kind) = mapping
                && matches!(
                    parameters.data.kind,
                    ParametersKind::Gradual | ParametersKind::Concatenate(ConcatenateTail::Gradual)
                )
            {
                Some(*kind)
            } else {
                None
            }
        })
        .await?;
    if let Some(kind) = materialization {
        let quote = parameters_storage_quote(2);
        return effects
            .local(
                quote.map(|quote| quote.work),
                quote.and_then(|quote| quote.bytes.checked_add(size_of::<[Parameter<'db>; 2]>())),
                || match kind {
                    // The bottom materialization of the `...` parameters is `(*object, **object)`,
                    // which accepts any call and is thus a subtype of all other parameters.
                    MaterializationKind::Bottom => Parameters::bottom(),
                    MaterializationKind::Top => Parameters::top(),
                },
            )
            .await;
    }

    // Parameters are in contravariant position, so we need to flip the type mapping.
    let mapping = effects.local(Some(1), Some(0), || mapping.flip()).await?;
    let count = effects.local(Some(1), Some(0), || parameters.len()).await?;
    let mut value = effects.new_parameters(count).await?;
    let mut input = effects
        .local(Some(1), Some(0), || parameters.iter())
        .await?;
    let mut has_starred_variadic = false;
    while let Some(parameter) = effects.local(Some(2), Some(0), || input.next()).await? {
        let mapped = map_parameter_with(db, parameter, &mapping, tcx, visitor, effects).await?;
        effects
            .local(Some(4), Some(0), || {
                has_starred_variadic |= mapped.is_variadic() && mapped.has_starred_annotation();
            })
            .await?;
        effects.push_parameter(&mut value, mapped).await?;
    }
    let kind = effects.local(Some(1), Some(0), || parameters.data.kind).await?;
    let mut result = effects
        .finish_parameters(value, kind)
        .await?;
    if has_starred_variadic {
        effects.expand_starred_parameters(db, &mut result).await?;
    }
    Ok(result)
}

/// Maps one parameter's annotation and default while preserving its definition and flags.
/// The supplied mapping is applied as-is; [`map_parameters_with`] flips the enclosing callable's
/// polarity before calling this helper.
pub(in crate::types) async fn map_parameter_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    parameter: &Parameter<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<Parameter<'db>, E::Error> {
    let annotation = effects
        .local(Some(1), Some(0), || parameter.annotated_type)
        .await?;
    let annotated_type = effects
        .map_type(db, annotation, mapping, tcx, visitor)
        .await?;
    let kind = map_parameter_kind_with(db, &parameter.kind, mapping, tcx, visitor, effects).await?;
    effects
        .local(Some(8), Some(0), || Parameter {
            annotated_type,
            definition: parameter.definition,
            kind,
            inferred_annotation: parameter.inferred_annotation,
            annotation_kind: parameter.annotation_kind,
            source_parameter_index: parameter.source_parameter_index,
        })
        .await
}

/// Maps an eager default when required and clones the parameter's shared name handle.
///
/// Promotion preserves default values in either polarity. Deferred defaults are never inferred.
async fn map_parameter_kind_with<'db, E: SignatureMappingEffects<'db>>(
    db: &'db dyn Db,
    kind: &ParameterKind<'db>,
    mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &E,
) -> Result<ParameterKind<'db>, E::Error> {
    let default = effects
        .local(Some(1), Some(0), || match kind {
            ParameterKind::PositionalOnly { default_type, .. }
            | ParameterKind::PositionalOrKeyword { default_type, .. }
            | ParameterKind::KeywordOnly { default_type, .. } => *default_type,
            ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => None,
        })
        .await?;
    let default_type = match default {
        Some(default) => match mapping {
            TypeMapping::ReplaceParameterDefaults => {
                Some(ParameterDefault::Inferred(Type::unknown()))
            }
            // Defaults describe values, not the set of accepted arguments. Promoting the
            // enclosing callable must not widen those values.
            TypeMapping::Promote(..) => Some(default),
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
            | TypeMapping::ApplyRecursiveSubstitution(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(_)
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::Materialize(_)
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => Some(match default {
                ParameterDefault::Inferred(ty) => ParameterDefault::Inferred(
                    effects.map_type(db, ty, mapping, tcx, visitor).await?,
                ),
                ParameterDefault::Deferred(_) => default,
            }),
        },
        None => None,
    };
    // Name cloning shares its CharStr allocation. Charge the fixed clone and the eventual
    // parameter retirement before acquiring the name owner. The provider's output-carrier
    // quotation includes the inline name representation.
    effects
        .parameter_kind_local(|| match kind {
            ParameterKind::PositionalOnly { name, .. } => ParameterKind::PositionalOnly {
                name: name.clone(),
                default_type,
            },
            ParameterKind::PositionalOrKeyword { name, .. } => ParameterKind::PositionalOrKeyword {
                name: name.clone(),
                default_type,
            },
            ParameterKind::KeywordOnly { name, .. } => ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type,
            },
            ParameterKind::Variadic { name } => ParameterKind::Variadic { name: name.clone() },
            ParameterKind::KeywordVariadic { name } => {
                ParameterKind::KeywordVariadic { name: name.clone() }
            }
        })
        .await
}
