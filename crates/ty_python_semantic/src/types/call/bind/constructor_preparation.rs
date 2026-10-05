//! Preparation-time constructor transformations and declared-return normalization.

use std::convert::Infallible;

use super::ownership::{CloneBindingsEffects, clone_bindings_with};
use super::{Binding, Bindings, CallableBinding, ConstructorCallableKind};
use crate::types::call::bindings::{BindingsEffects, InlineBindingsEffects};
use crate::types::constructor::effects::checked_source;
use crate::types::generics::GenericContext;
use crate::types::signatures::constructor_preparation::{
    ConstructorSignatureEffects, InlineConstructorSignatureEffects, bind_self_with_receiver_with,
    bind_unused_self_with,
};
use crate::types::signatures::effects::legacy_inline;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::typevar::TypeVarIdentity;
use crate::types::{BoundMethodType, BoundTypeVarInstance, Type};
use crate::{Db, ProgramEnvironment};

/// Identifies the child whose storage or semantic effects are not yet available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorStorageOperation {
    GenericContextMerge,
}

/// Transforms retained bindings before arguments are matched against their signatures.
///
/// Mutating operations borrow the caller's owner so that a refused child cannot destroy it
/// before the invocation has drained. Downstream attachment must cover the complete owned
/// bindings structure, including bindings retained by property-accessor errors.
///
/// An error does not roll back earlier mutations. The caller must propagate it and retire the
/// partially transformed bindings after active children drain. Only a successful outer
/// `constructor_bindings_with` result may be transferred to call invocation.
pub(in crate::types) trait ConstructorBindingStorageEffects<'db>:
    BindingsEffects<'db>
{
    /// Bakes descriptor binding before supplying the implicit constructor class argument.
    async fn bind_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
        self_type: Type<'db>,
        instance_type: Type<'db>,
    ) -> Result<(), Self::Error>;

    /// Wraps regular callables and normalizes each overload's declared constructor return.
    async fn wrap_constructor(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error>;

    /// Records a possibly absent implicit `__new__` or `__init__` call.
    async fn mark_unbound(
        &self,
        bindings: &mut Bindings<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error>;

    /// Applies the initializer receiver to an otherwise unused `Self` variable.
    async fn bind_initializer_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error>;

    /// Copies the complete downstream bindings into every constructor entry.
    async fn attach_downstream(
        &self,
        bindings: &mut Bindings<'db>,
        downstream: &Bindings<'db>,
    ) -> Result<(), Self::Error>;

    /// Adds the class's generic context to entry and downstream overloads.
    async fn apply_class_context(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        context: Option<GenericContext<'db>>,
    ) -> Result<(), Self::Error>;

    /// Constructs a gradual signature for an existing constructor fallback branch.
    async fn fallback(
        &self,
        receiver: Type<'db>,
        context: Option<GenericContext<'db>>,
        return_type: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;

    /// Admits the final logical transfer while the caller retains the complete bindings owner.
    async fn transfer(&self, bindings: &Bindings<'db>) -> Result<(), Self::Error>;
}

impl<'db> ConstructorBindingStorageEffects<'db> for InlineBindingsEffects<'_, 'db> {
    async fn bind_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
        self_type: Type<'db>,
        instance_type: Type<'db>,
    ) -> Result<(), Self::Error> {
        checked_source(db, || {
            Ok(legacy_inline(bind_new_with(
                db,
                env,
                bindings,
                self_type,
                instance_type,
                &InlineConstructorSignatureEffects,
            )))
        })
    }

    async fn wrap_constructor(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error> {
        checked_source(db, || {
            Ok(legacy_inline(wrap_constructor_with(
                db,
                bindings,
                instance_type,
                kind,
                &InlineConstructorSignatureEffects,
            )))
        })
    }

    async fn mark_unbound(
        &self,
        bindings: &mut Bindings<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error> {
        mark_unbound(bindings, kind);
        Ok(())
    }

    async fn bind_initializer_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        checked_source(db, || {
            Ok(legacy_inline(bind_initializer_self_with(
                db,
                env,
                bindings,
                &InlineConstructorSignatureEffects,
            )))
        })
    }

    async fn attach_downstream(
        &self,
        bindings: &mut Bindings<'db>,
        downstream: &Bindings<'db>,
    ) -> Result<(), Self::Error> {
        Ok(legacy_inline(attach_downstream_with(
            bindings,
            downstream,
            &super::ownership::InlineCloneBindingsEffects,
        )))
    }

    async fn apply_class_context(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        context: Option<GenericContext<'db>>,
    ) -> Result<(), Self::Error> {
        checked_source(db, || {
            Ok(legacy_inline(
                super::constructor_context_walk::apply_class_context_with(
                    db,
                    bindings,
                    context,
                    &InlineConstructorSignatureEffects,
                ),
            ))
        })
    }

    async fn fallback(
        &self,
        receiver: Type<'db>,
        context: Option<GenericContext<'db>>,
        return_type: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(fallback(receiver, context, return_type))
    }

    async fn transfer(&self, _bindings: &Bindings<'db>) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Sets the implicit-call flag without changing overloads or their ownership.
pub(in crate::types) fn mark_unbound(bindings: &mut Bindings<'_>, kind: ConstructorCallableKind) {
    match kind {
        ConstructorCallableKind::New => bindings.set_implicit_dunder_new_is_possibly_unbound(),
        ConstructorCallableKind::Init => bindings.set_implicit_dunder_init_is_possibly_unbound(),
        ConstructorCallableKind::MetaclassCall => {}
    }
}

/// Builds the gradual binding used by the constructor's existing fallback decisions.
pub(in crate::types) fn fallback<'db>(
    receiver: Type<'db>,
    context: Option<GenericContext<'db>>,
    return_type: Type<'db>,
) -> Bindings<'db> {
    Binding::single(
        receiver,
        Signature::new_generic(context, Parameters::gradual_form(), return_type),
    )
    .into()
}

/// Quotes the fixed values moved through gradual binding assembly, excluding its parameters.
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) const fn fallback_representation_bytes() -> usize {
    size_of::<Signature<'_>>()
        + size_of::<Binding<'_>>()
        + size_of::<CallableBinding<'_>>()
        + size_of::<super::CallableItem<'_>>()
        + size_of::<super::BindingsElement<'_>>()
        + size_of::<Bindings<'_>>()
}

/// Supplies the alias and type-variable fields used to normalize declared constructor returns.
pub(in crate::types) trait ConstructorReturnEffects<'db> {
    type Error;

    /// Admits the fixed signature and first-parameter inspection used by return normalization.
    async fn checkpoint(&self) -> Result<(), Self::Error>;

    async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;

    async fn is_self(&self, ty: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;

    async fn identity(
        &self,
        ty: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarIdentity<'db>, Self::Error>;
}

/// Reads constructor return dependencies immediately for ordinary binding preparation.
pub(super) struct InlineConstructorReturnEffects<'db>(pub(super) &'db dyn Db);

impl<'db> ConstructorReturnEffects<'db> for InlineConstructorReturnEffects<'db> {
    type Error = Infallible;

    async fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.0))
    }

    async fn is_self(&self, ty: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error> {
        Ok(ty.typevar(self.0).is_self(self.0))
    }

    async fn identity(
        &self,
        ty: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarIdentity<'db>, Self::Error> {
        Ok(ty.typevar(self.0).identity(self.0))
    }
}

/// Returns the declared inference return before any call-dependent specialization is applied.
///
/// Initializers infer the constructed instance. Unannotated non-recovery returns do the same;
/// `__new__` also recognizes `Self` and a returned type variable shared with its `type[T]` receiver.
pub(in crate::types) async fn constructor_return_with<'db, E: ConstructorReturnEffects<'db>>(
    signature: &Signature<'db>,
    kind: ConstructorCallableKind,
    instance_type: Type<'db>,
    effects: &E,
) -> Result<Type<'db>, E::Error> {
    effects.checkpoint().await?;
    let declared = effects.resolve_alias(signature.return_ty).await?;
    match (kind, declared) {
        (ConstructorCallableKind::Init, _) => return Ok(instance_type),
        (_, ty) if ty.is_unknown() && !signature.is_recursion_recovery() => {
            return Ok(instance_type);
        }
        (ConstructorCallableKind::New, Type::TypeVar(typevar)) => {
            if self_like_constructor_return_with(signature, typevar, effects).await? {
                return Ok(instance_type);
            }
        }
        (ConstructorCallableKind::New | ConstructorCallableKind::MetaclassCall, _) => {}
    }
    Ok(signature.return_ty)
}

/// Is a type variable returned from a constructor method a representation of the self type?
///
/// Handles `typing.Self` annotations and `__new__` methods returning `T` where `self:
/// type[T]`.
async fn self_like_constructor_return_with<'db, E: ConstructorReturnEffects<'db>>(
    signature: &Signature<'db>,
    return_typevar: BoundTypeVarInstance<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    if effects.is_self(return_typevar).await? {
        return Ok(true);
    }
    if let Some(parameter) = signature.parameters().get(0)
        && let Type::SubclassOf(subclass) =
            effects.resolve_alias(parameter.annotated_type()).await?
        && let Some(receiver_typevar) = subclass.into_type_var()
    {
        return Ok(
            effects.identity(receiver_typevar).await? == effects.identity(return_typevar).await?
        );
    }
    Ok(false)
}

/// Provides signature transformations while the callable retains its old overload storage.
pub(in crate::types) trait ReceiverBindingEffects<'db>:
    ConstructorSignatureEffects<'db>
{
    /// Observes the boundary immediately before a constructor-item mutation admission.
    #[cfg(test)]
    fn constructor_wrap_before(&self) {}

    /// Observes the same boundary inside its successful mutation callback.
    #[cfg(test)]
    fn constructor_wrap_after(&self) {}

    async fn typing_self_type(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    /// Normalizes a constructor return, or copies the declared return when context is `None`.
    async fn normalized_return(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        constructor: Option<(ConstructorCallableKind, Type<'db>)>,
    ) -> Result<Type<'db>, Self::Error>;

    /// Admits detaching an old downstream whose producer prepaid its complete retirement.
    async fn retire_downstream(
        &self,
        downstream: &mut Option<Box<Bindings<'db>>>,
    ) -> Result<(), Self::Error>;

    /// Updates instance contexts and inference returns in the supplied root and its downstreams.
    /// Property-error binding owners do not participate in constructor context propagation.
    async fn constructor_descendants(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
    ) -> Result<(), Self::Error>;

    async fn merge_generic_context(
        &self,
        db: &'db dyn Db,
        existing: Option<GenericContext<'db>>,
        incoming: GenericContext<'db>,
    ) -> Result<GenericContext<'db>, Self::Error>;
}

impl<'db> ReceiverBindingEffects<'db> for InlineConstructorSignatureEffects {
    async fn typing_self_type(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(method.typing_self_type(db))
    }

    async fn normalized_return(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        constructor: Option<(ConstructorCallableKind, Type<'db>)>,
    ) -> Result<Type<'db>, Self::Error> {
        match constructor {
            Some((kind, instance)) => {
                constructor_return_with(
                    signature,
                    kind,
                    instance,
                    &InlineConstructorReturnEffects(db),
                )
                .await
            }
            None => Ok(signature.return_ty),
        }
    }

    async fn retire_downstream(
        &self,
        downstream: &mut Option<Box<Bindings<'db>>>,
    ) -> Result<(), Self::Error> {
        *downstream = None;
        Ok(())
    }

    async fn constructor_descendants(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
    ) -> Result<(), Self::Error> {
        super::constructor_context_walk::set_constructor_instance_with(
            db,
            bindings,
            instance_type,
            self,
        )
        .await
    }

    async fn merge_generic_context(
        &self,
        db: &'db dyn Db,
        existing: Option<GenericContext<'db>>,
        incoming: GenericContext<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(match existing {
            None => incoming,
            Some(existing) => existing.merge(db, incoming),
        })
    }
}

/// Consumes an implicit receiver in each overload and preserves diagnostic parameter offsets.
pub(in crate::types) async fn bake_callable_receiver_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    binding: &mut CallableBinding<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let Some(bound_self) = effects
        .local(Some(2), Some(0), || binding.bound_type.take())
        .await?
    else {
        return Ok(());
    };
    let typing_self = match effects
        .local(Some(1), Some(0), || binding.callable_type)
        .await?
    {
        Type::BoundMethod(method) => effects.typing_self_type(db, method).await?,
        _ => bound_self,
    };
    let count = effects
        .local(Some(1), Some(0), || binding.overloads.len())
        .await?;
    for index in 0..count {
        let (removed_receiver, context) = effects
            .local(Some(5), Some(0), || {
                let overload = &binding.overloads[index];
                (
                    overload
                        .signature
                        .parameters()
                        .get(0)
                        .is_some_and(Parameter::is_positional),
                    overload
                        .constructor_context
                        .map(|context| (context.kind(), context.instance_type())),
                )
            })
            .await?;
        let signature = bind_self_with_receiver_with(
            db,
            env,
            &binding.overloads[index].signature,
            Some(bound_self),
            Some(typing_self),
            effects,
        )
        .await?;
        let return_ty = effects.normalized_return(db, &signature, context).await?;
        effects
            .local(
                Some(3),
                Some(size_of::<Signature<'db>>() + size_of::<Type<'db>>() + size_of::<usize>()),
                || {
                    let overload = &mut binding.overloads[index];
                    overload.signature = signature;
                    overload.return_ty = return_ty;
                    overload.source_parameter_index_offset += usize::from(removed_receiver);
                },
            )
            .await?;
    }
    Ok(())
}

/// Binds an unused `Self` without consuming the callable's implicit receiver parameter.
pub(in crate::types) async fn bind_callable_unused_self_with<
    'db,
    E: ReceiverBindingEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    binding: &mut CallableBinding<'db>,
    self_type: Type<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let count = effects
        .local(Some(1), Some(0), || binding.overloads.len())
        .await?;
    for index in 0..count {
        let replacement = bind_unused_self_with(
            db,
            env,
            &binding.overloads[index].signature,
            self_type,
            effects,
        )
        .await?;
        let Some(signature) = replacement else {
            continue;
        };
        let context = effects
            .local(Some(3), Some(0), || {
                let overload = &binding.overloads[index];
                overload
                    .constructor_context
                    .map(|context| (context.kind(), context.instance_type()))
            })
            .await?;
        let return_ty = effects.normalized_return(db, &signature, context).await?;
        effects
            .local(
                Some(2),
                Some(size_of::<Signature<'db>>() + size_of::<Type<'db>>()),
                || {
                    let overload = &mut binding.overloads[index];
                    overload.signature = signature;
                    overload.return_ty = return_ty;
                },
            )
            .await?;
    }
    Ok(())
}

/// Bakes the receiver of every callable in the union/intersection structure.
pub(in crate::types) async fn bake_bindings_receivers_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings: &mut Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let elements = effects
        .local(Some(1), Some(0), || bindings.elements.len())
        .await?;
    for element in 0..elements {
        let items = effects
            .local(Some(1), Some(0), || bindings.elements[element].items.len())
            .await?;
        for item in 0..items {
            bake_callable_receiver_with(
                db,
                env,
                bindings.elements[element].items[item].callable_mut(),
                effects,
            )
            .await?;
        }
    }
    Ok(())
}

/// Bakes each `__new__` receiver, supplies implicit `cls`, and binds otherwise unused `Self`.
/// Any constructor chain the callable previously represented is retired after those transforms.
pub(in crate::types) async fn bind_new_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings: &mut Bindings<'db>,
    self_type: Type<'db>,
    instance_type: Type<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let elements = effects
        .local(Some(1), Some(0), || bindings.elements.len())
        .await?;
    for element in 0..elements {
        let items = effects
            .local(Some(1), Some(0), || bindings.elements[element].items.len())
            .await?;
        for item in 0..items {
            let binding = bindings.elements[element].items[item].callable_mut();
            // If descriptor binding produced a bound callable, bake that into the signature
            // first, then bind `cls` for constructor-call semantics (the call site omits `cls`).
            // Note: This intentionally preserves `type.__call__` behavior for `@classmethod __new__`,
            // which receives an extra implicit `cls` and errors at call sites.
            bake_callable_receiver_with(db, env, binding, effects).await?;
            effects
                .local(Some(1), Some(size_of::<Option<Type<'db>>>()), || {
                    binding.bound_type = Some(self_type)
                })
                .await?;
            bind_callable_unused_self_with(db, env, binding, instance_type, effects).await?;
            if let Some(constructor) = bindings.elements[element].items[item].as_constructor_mut() {
                effects
                    .retire_downstream(&mut constructor.downstream_constructor)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Binds otherwise unused `Self` to each initializer receiver without consuming that receiver.
/// Applies to top-level callables without changing descendants.
pub(in crate::types) async fn bind_initializer_self_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings: &mut Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let elements = effects
        .local(Some(1), Some(0), || bindings.elements.len())
        .await?;
    for element in 0..elements {
        let items = effects
            .local(Some(1), Some(0), || bindings.elements[element].items.len())
            .await?;
        for item in 0..items {
            let binding = bindings.elements[element].items[item].callable_mut();
            let (bound_type, signature_type) = effects
                .local(Some(2), Some(0), || {
                    (binding.bound_type, binding.signature_type)
                })
                .await?;
            if let Some(bound_type) = bound_type {
                let self_type = match signature_type {
                    Type::BoundMethod(method) => effects.typing_self_type(db, method).await?,
                    _ => bound_type,
                };
                bind_callable_unused_self_with(db, env, binding, self_type, effects).await?;
            }
        }
    }
    Ok(())
}

/// Wraps all callable entries and updates each constructor overload's declared inference return.
pub(in crate::types) async fn wrap_constructor_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    bindings: &mut Bindings<'db>,
    instance_type: Type<'db>,
    kind: ConstructorCallableKind,
    effects: &E,
) -> Result<(), E::Error> {
    let elements = effects
        .local(Some(1), Some(0), || bindings.elements.len())
        .await?;
    for element in 0..elements {
        let items = effects
            .local(Some(1), Some(0), || bindings.elements[element].items.len())
            .await?;
        for item in 0..items {
            #[cfg(test)]
            effects.constructor_wrap_before();
            effects
                .local(
                    Some(8),
                    Some(3 * size_of::<super::CallableItem<'db>>()),
                    || {
                        let item = &mut bindings.elements[element].items[item];
                        if let super::CallableItem::Regular(binding) = item {
                            let placeholder = super::CallableItem::Regular(
                                CallableBinding::not_callable(binding.callable_type),
                            );
                            *item = std::mem::replace(item, placeholder)
                                .wrap_as_constructor(instance_type, kind);
                        }
                        #[cfg(test)]
                        effects.constructor_wrap_after();
                    },
                )
                .await?;
        }
    }
    effects
        .constructor_descendants(db, bindings, instance_type)
        .await
}

/// Clones complete downstream bindings into every constructor entry after admitting each owner.
pub(in crate::types) async fn attach_downstream_with<'db, E: CloneBindingsEffects<'db>>(
    bindings: &mut Bindings<'db>,
    downstream: &Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let elements = effects
        .local(Some(1), Some(0), || bindings.elements.len())
        .await?;
    for element in 0..elements {
        let items = effects
            .local(Some(1), Some(0), || bindings.elements[element].items.len())
            .await?;
        for item in 0..items {
            let Some(constructor) = effects
                .local(Some(1), Some(0), || {
                    bindings.elements[element].items[item].as_constructor_mut()
                })
                .await?
            else {
                continue;
            };
            let cloned = clone_bindings_with(downstream, effects).await?;
            let mut cloned = effects.local(Some(1), Some(0), || Some(cloned)).await?;
            #[cfg(test)]
            effects.downstream_install_before();
            effects
                .local(
                    Some(5),
                    Some(
                        size_of::<Bindings<'db>>()
                            + size_of::<Box<Bindings<'db>>>()
                            + size_of::<Option<Box<Bindings<'db>>>>(),
                    ),
                    || {
                        if let Some(cloned) = cloned.take() {
                            constructor.downstream_constructor = Some(Box::new(cloned));
                        }
                        #[cfg(test)]
                        effects.downstream_install_after();
                    },
                )
                .await?;
        }
    }
    Ok(())
}
