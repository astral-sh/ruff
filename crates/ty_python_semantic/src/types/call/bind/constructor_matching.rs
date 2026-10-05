//! Freshens constructor signatures and matches every downstream callable in source order.

use std::alloc::Layout;
use std::convert::Infallible;
use std::slice;

use super::constructor::{ConstructorBinding, ConstructorContext};
use super::constructor_preparation::{InlineConstructorReturnEffects, constructor_return_with};
use super::parameter_matching::{InlineParameterMatching, ParameterMatchingEffects};
use super::{BindingsElement, CallableBinding, CallableItem, ConstructorCallableKind};
use crate::types::call::CallArguments;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::Signature;
use crate::types::typevar::TypeVarNonceGenerator;
use crate::types::typevar::constructor_nonce::{
    ConstructorNonceEffects, InlineConstructorNonceEffects,
};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, Type, TypeContext, TypeMapping,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
pub(in crate::types) mod observations;

#[cfg(test)]
pub(in crate::types) mod fixtures;

/// Identifies a storage mutation observed before admission and after successful completion.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ConstructorMatchingOperation {
    FrameGrowth,
    ContextBufferGrowth,
    BoundArguments,
    MatcherAllocation,
    SignatureInstall,
    ResultTransfer,
    GenericContextIntern,
    ConstraintIntern,
}

/// Identifies the constructor pass that visits a retained callable entry.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ConstructorMatchingPhase {
    Freshening,
    Matching,
}

/// Supplies canonical fields and semantic children while matching retains the binding tree.
/// Mapping methods each create an independent root visitor; their descendants share that visitor.
/// A failed child leaves prior mutations in the binding tree, which remains alive until children drain.
pub(in crate::types) trait ConstructorMatchingEffects<'db>:
    ConstructorNonceEffects<'db>
{
    async fn class_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error>;

    async fn specialization_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error>;

    async fn is_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn context_from_typevars(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> Result<GenericContext<'db>, Self::Error>;

    async fn freshen_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Type<'db>, Self::Error>;

    async fn freshen_signature(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Signature<'db>, Self::Error>;

    async fn normalized_return(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        kind: ConstructorCallableKind,
        instance: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    /// Quotes logical work for retiring the old signature; the caller admits it before replacement.
    async fn signature_retirement(&self, signature: &Signature<'db>) -> Result<usize, Self::Error>;

    #[cfg(test)]
    fn before_matching(&self, _operation: ConstructorMatchingOperation) {}

    #[cfg(test)]
    fn after_matching(&self, _operation: ConstructorMatchingOperation) {}

    #[cfg(test)]
    fn matching_entry(&self, _phase: ConstructorMatchingPhase, _identity: usize) {}
}

impl<'db> ConstructorNonceEffects<'db> for InlineParameterMatching {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error> {
        InlineConstructorNonceEffects.variables(db, context).await
    }

    async fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BindingContext<'db>, Self::Error> {
        InlineConstructorNonceEffects
            .binding_context(db, variable)
            .await
    }
}

impl<'db> ConstructorMatchingEffects<'db> for InlineParameterMatching {
    async fn class_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        Ok(ty
            .class_specialization(db, env)
            .map(|(_, specialization)| specialization))
    }

    async fn specialization_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }

    async fn is_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.is_paramspec(db))
    }

    async fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.typevar(db).is_self(db))
    }

    async fn context_from_typevars(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(GenericContext::from_typevar_instances(
            db,
            env,
            variables.iter().copied(),
        ))
    }

    async fn freshen_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_type_mapping(
            db,
            env,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: context,
                delta,
            },
            TypeContext::default(),
        ))
    }

    async fn freshen_signature(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(signature.apply_type_mapping_impl(
            db,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: context,
                delta,
            },
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        ))
    }

    async fn normalized_return(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        kind: ConstructorCallableKind,
        instance: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        constructor_return_with(
            signature,
            kind,
            instance,
            &InlineConstructorReturnEffects(db),
        )
        .await
    }

    async fn signature_retirement(
        &self,
        _signature: &Signature<'db>,
    ) -> Result<usize, Self::Error> {
        Ok(0)
    }
}

/// Carries the single class freshening decision through all constructor descendants.
#[derive(Clone, Copy, Debug)]
struct Freshening<'db> {
    context: GenericContext<'db>,
    delta: u32,
    instance: Type<'db>,
}

/// Retains the remaining siblings while constructor entries are freshened depth first.
#[derive(Debug)]
enum FreshenFrame<'bindings, 'db> {
    Elements(slice::IterMut<'bindings, BindingsElement<'db>>),
    Items(slice::IterMut<'bindings, CallableItem<'db>>),
    Constructor(&'bindings mut ConstructorBinding<'db>),
}

/// Retains disjoint entry and downstream borrows while every callable is matched depth first.
#[derive(Debug)]
enum MatchFrame<'bindings, 'db> {
    Elements(slice::IterMut<'bindings, BindingsElement<'db>>),
    Items(slice::IterMut<'bindings, CallableItem<'db>>),
    Callable(&'bindings mut CallableBinding<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferKind {
    Frames,
    Variables,
}

impl BufferKind {
    #[cfg(test)]
    const fn operation(self) -> ConstructorMatchingOperation {
        match self {
            Self::Frames => ConstructorMatchingOperation::FrameGrowth,
            Self::Variables => ConstructorMatchingOperation::ContextBufferGrowth,
        }
    }
}

/// Reserves a geometrically growing buffer before constructing its next value.
/// New backing prepays its retirement; moves and initialized values are counted as logical work.
async fn push_with<'db, T, E: ConstructorMatchingEffects<'db>>(
    values: &mut Vec<T>,
    make: impl FnOnce() -> T,
    _kind: BufferKind,
    effects: &E,
) -> Result<(), E::Error> {
    let len = values.len();
    let needs_growth = len == values.capacity();
    let target = if needs_growth {
        values
            .capacity()
            .checked_mul(2)
            .map(|capacity| capacity.max(4))
    } else {
        Some(values.capacity())
    };
    let work = if needs_growth {
        target.and_then(|target| len.checked_mul(2)?.checked_add(target)?.checked_add(80))
    } else {
        Some(68)
    };
    let bytes = if needs_growth {
        target
            .and_then(|target| Layout::array::<T>(target).ok())
            .map(|layout| layout.size())
            .and_then(|bytes| bytes.checked_add(len.checked_mul(size_of::<T>())?))
            .and_then(|bytes| bytes.checked_add(size_of::<T>()))
    } else {
        Some(size_of::<T>())
    };
    #[cfg(test)]
    if needs_growth {
        effects.before_matching(_kind.operation());
    }
    effects
        .local(work, bytes, || {
            if let Some(target) = target
                && needs_growth
            {
                values.reserve_exact(target - len);
            }
            values.push(make());
            #[cfg(test)]
            if needs_growth {
                effects.after_matching(_kind.operation());
            }
        })
        .await
}

/// Reads one variable from a borrowed canonical context without retaining a database cursor.
async fn next_variable_with<'db, E: ConstructorMatchingEffects<'db>>(
    variables: &ContextVariables<'db>,
    cursor: &mut usize,
    effects: &E,
) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
    effects
        .local(Some(5), Some(0), || {
            let variable = GenericContext::variable_at_in(variables, *cursor);
            if variable.is_some() {
                *cursor += 1;
            }
            variable
        })
        .await
}

/// Reports whether the context contains a ParamSpec, which skips freshening before any nonce decision.
async fn has_paramspec_with<'db, E: ConstructorMatchingEffects<'db>>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let variables = effects.variables(db, context).await?;
    let mut cursor = effects.local(Some(1), Some(0), || 0).await?;
    while let Some(variable) = next_variable_with(variables, &mut cursor, effects).await? {
        if effects.is_paramspec(db, variable).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Combines class variables with only the `Self` variables declared by the selected signature.
async fn signature_context_with<'db, E: ConstructorMatchingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class_context: GenericContext<'db>,
    signature_context: Option<GenericContext<'db>>,
    effects: &E,
) -> Result<GenericContext<'db>, E::Error> {
    let mut selected = effects.local(Some(1), Some(0), Vec::new).await?;
    let variables = effects.variables(db, class_context).await?;
    let mut cursor = effects.local(Some(1), Some(0), || 0).await?;
    while let Some(variable) = next_variable_with(variables, &mut cursor, effects).await? {
        push_with(&mut selected, || variable, BufferKind::Variables, effects).await?;
    }
    if let Some(signature_context) = signature_context {
        let variables = effects.variables(db, signature_context).await?;
        let mut cursor = effects.local(Some(1), Some(0), || 0).await?;
        while let Some(variable) = next_variable_with(variables, &mut cursor, effects).await? {
            if effects.is_self(db, variable).await? {
                push_with(&mut selected, || variable, BufferKind::Variables, effects).await?;
            }
        }
    }
    effects.context_from_typevars(db, env, &selected).await
}

/// Freshens one constructor entry while preserving its source-level constructed instance.
async fn freshen_entry_with<'db, E: ConstructorMatchingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constructor: &mut ConstructorBinding<'db>,
    freshening: Freshening<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    #[cfg(test)]
    effects.matching_entry(
        ConstructorMatchingPhase::Freshening,
        std::ptr::from_ref(&constructor.entry).addr(),
    );
    // Keep the source-level instance on `ConstructorBinding`; the final return type applies
    // the inferred specialization to that instance. Only the per-overload context is
    // call-local, so its instance must use the same fresh type variables as the signature.
    let context = effects
        .local(Some(3), Some(0), || {
            constructor
                .context()
                .with_instance_type(freshening.instance)
        })
        .await?;
    let receiver = effects
        .local(Some(1), Some(0), || constructor.entry.bound_type)
        .await?;
    if let Some(receiver) = receiver {
        let receiver = effects
            .freshen_type(db, env, receiver, freshening.context, freshening.delta)
            .await?;
        effects
            .local(Some(1), Some(size_of::<Option<Type<'db>>>()), || {
                constructor.entry.bound_type = Some(receiver);
            })
            .await?;
    }
    let mut overloads = effects
        .local(Some(1), Some(0), || constructor.entry.overloads.iter_mut())
        .await?;
    while let Some(overload) = effects.local(Some(2), Some(0), || overloads.next()).await? {
        // The constructor's `Self` bound must use the same fresh class type variables as
        // its receiver. Include only `Self` variables owned by this signature, so a caller's
        // `Self` used as an explicit class type argument retains its original bound.
        let signature_context = effects
            .local(Some(1), Some(0), || overload.signature.generic_context)
            .await?;
        let signature_context =
            signature_context_with(db, env, freshening.context, signature_context, effects).await?;
        let signature = effects
            .freshen_signature(
                db,
                env,
                &overload.signature,
                signature_context,
                freshening.delta,
            )
            .await?;
        let retirement = effects.signature_retirement(&overload.signature).await?;
        #[cfg(test)]
        effects.before_matching(ConstructorMatchingOperation::SignatureInstall);
        effects
            .local(
                retirement.checked_add(2),
                Some(size_of::<Signature<'db>>()),
                || {
                    overload.signature = signature;
                    #[cfg(test)]
                    effects.after_matching(ConstructorMatchingOperation::SignatureInstall);
                },
            )
            .await?;
        effects
            .local(
                Some(1),
                Some(size_of::<Option<ConstructorContext<'db>>>()),
                || {
                    overload.constructor_context = Some(context);
                },
            )
            .await?;
        let return_type = effects
            .normalized_return(
                db,
                &overload.signature,
                context.kind(),
                context.instance_type(),
            )
            .await?;
        effects
            .local(Some(1), Some(size_of::<Type<'db>>()), || {
                overload.return_ty = return_type
            })
            .await?;
    }
    Ok(())
}

/// Freshens eligible constructor entries and every constructor descendant with one nonce delta.
pub(super) async fn freshen_constructor_with<'db, E: ConstructorMatchingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constructor: &mut ConstructorBinding<'db>,
    nonce_generator: &TypeVarNonceGenerator<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let instance = effects
        .local(Some(1), Some(0), || constructor.constructed_instance_type())
        .await?;
    let Some(specialization) = effects.class_specialization(db, env, instance).await? else {
        return Ok(());
    };
    let context = effects.specialization_context(db, specialization).await?;
    if has_paramspec_with(db, context, effects).await?
        || !nonce_generator
            .should_freshen_with(db, context, effects)
            .await?
    {
        return Ok(());
    }
    let nonce = nonce_generator.next_with(effects).await?;
    let delta = effects.local(Some(1), Some(0), || nonce.value()).await?;
    let fresh_instance = effects
        .freshen_type(db, env, instance, context, delta)
        .await?;
    // Only freshen a generic context that belongs to the constructed instance itself.
    // `class_specialization` can also find a context through a class-object type variable's
    // bound, but freshening that context would detach the constructor parameters from the
    // receiver.
    if effects
        .local(Some(1), Some(0), || fresh_instance == instance)
        .await?
    {
        return Ok(());
    }
    let freshening = effects
        .local(Some(3), Some(0), || Freshening {
            context,
            delta,
            instance: fresh_instance,
        })
        .await?;
    let mut frames = effects.local(Some(1), Some(0), Vec::new).await?;
    push_with(
        &mut frames,
        || FreshenFrame::Constructor(constructor),
        BufferKind::Frames,
        effects,
    )
    .await?;
    while let Some(frame) = effects.local(Some(2), Some(0), || frames.pop()).await? {
        match frame {
            FreshenFrame::Elements(mut elements) => {
                let element = effects.local(Some(2), Some(0), || elements.next()).await?;
                if let Some(element) = element {
                    push_with(
                        &mut frames,
                        || FreshenFrame::Elements(elements),
                        BufferKind::Frames,
                        effects,
                    )
                    .await?;
                    let items = effects
                        .local(Some(1), Some(0), || {
                            FreshenFrame::Items(element.items.iter_mut())
                        })
                        .await?;
                    push_with(&mut frames, || items, BufferKind::Frames, effects).await?;
                }
            }
            FreshenFrame::Items(mut items) => {
                let item = effects.local(Some(2), Some(0), || items.next()).await?;
                if let Some(item) = item {
                    push_with(
                        &mut frames,
                        || FreshenFrame::Items(items),
                        BufferKind::Frames,
                        effects,
                    )
                    .await?;
                    match item {
                        CallableItem::Regular(_) => {}
                        CallableItem::Constructor(constructor) => {
                            push_with(
                                &mut frames,
                                || FreshenFrame::Constructor(constructor),
                                BufferKind::Frames,
                                effects,
                            )
                            .await?;
                        }
                    }
                }
            }
            FreshenFrame::Constructor(constructor) => {
                freshen_entry_with(db, env, constructor, freshening, effects).await?;
                let downstream = effects
                    .local(Some(2), Some(0), || {
                        constructor
                            .downstream_constructor
                            .as_deref_mut()
                            .map(|downstream| {
                                FreshenFrame::Elements(downstream.elements.iter_mut())
                            })
                    })
                    .await?;
                if let Some(downstream) = downstream {
                    push_with(&mut frames, || downstream, BufferKind::Frames, effects).await?;
                }
            }
        }
    }
    Ok(())
}

/// Pushes the downstream first so that the entry is matched before its complete subtree.
async fn push_constructor_with<'bindings, 'db, E: ConstructorMatchingEffects<'db>>(
    frames: &mut Vec<MatchFrame<'bindings, 'db>>,
    constructor: &'bindings mut ConstructorBinding<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let ConstructorBinding {
        entry,
        downstream_constructor,
        ..
    } = constructor;
    let downstream = effects
        .local(Some(2), Some(0), || {
            downstream_constructor
                .as_deref_mut()
                .map(|downstream| MatchFrame::Elements(downstream.elements.iter_mut()))
        })
        .await?;
    if let Some(downstream) = downstream {
        push_with(frames, || downstream, BufferKind::Frames, effects).await?;
    }
    push_with(
        frames,
        || MatchFrame::Callable(entry),
        BufferKind::Frames,
        effects,
    )
    .await
}

/// Matches the entry and all downstream callables without repeating the root freshening pass.
/// Property-accessor error trees remain owned cleanup edges and are not visited by this walk.
pub(super) async fn match_constructor_with<'db, E: ParameterMatchingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constructor: &mut ConstructorBinding<'db>,
    arguments: &CallArguments<'_, 'db>,
    effects: &E,
) -> Result<(), E::Error> {
    // We don't know at this point whether we'll need to check downstream constructors or not
    // (since we can't resolve return types yet), so we match parameters for all downstream
    // constructors; this may be needed for argument type contexts.
    let mut frames = effects.local(Some(1), Some(0), Vec::new).await?;
    push_constructor_with(&mut frames, constructor, effects).await?;
    while let Some(frame) = effects.local(Some(2), Some(0), || frames.pop()).await? {
        match frame {
            MatchFrame::Elements(mut elements) => {
                let element = effects.local(Some(2), Some(0), || elements.next()).await?;
                if let Some(element) = element {
                    push_with(
                        &mut frames,
                        || MatchFrame::Elements(elements),
                        BufferKind::Frames,
                        effects,
                    )
                    .await?;
                    let items = effects
                        .local(Some(1), Some(0), || {
                            MatchFrame::Items(element.items.iter_mut())
                        })
                        .await?;
                    push_with(&mut frames, || items, BufferKind::Frames, effects).await?;
                }
            }
            MatchFrame::Items(mut items) => {
                let item = effects.local(Some(2), Some(0), || items.next()).await?;
                if let Some(item) = item {
                    push_with(
                        &mut frames,
                        || MatchFrame::Items(items),
                        BufferKind::Frames,
                        effects,
                    )
                    .await?;
                    match item {
                        CallableItem::Regular(callable) => {
                            push_with(
                                &mut frames,
                                || MatchFrame::Callable(callable),
                                BufferKind::Frames,
                                effects,
                            )
                            .await?;
                        }
                        CallableItem::Constructor(constructor) => {
                            push_constructor_with(&mut frames, constructor, effects).await?;
                        }
                    }
                }
            }
            MatchFrame::Callable(callable) => {
                callable
                    .match_parameters_with(db, env, arguments, effects)
                    .await?;
            }
        }
    }
    Ok(())
}
