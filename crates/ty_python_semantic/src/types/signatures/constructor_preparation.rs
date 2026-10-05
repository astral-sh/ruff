//! Receiver-signature preparation shared by ordinary and controlled constructor bindings.

use std::borrow::Cow;
use std::convert::Infallible;

use super::{Parameter, Parameters, Signature, SignatureExtras, merge_receiver_constraints};
use crate::types::constraints::OwnedConstraintSet;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    SelfBinding, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

/// Semantic children that can stop controlled receiver-signature preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorSignatureOperation {
    SyntheticReceiver,
    SelfMapping,
    ReceiverDomain,
    ReceiverConstraint,
    ReceiverConstraintMerge,
    GenericContextSelfRemoval,
    UnusedSelfRelation,
    UnusedSelfSpecialization,
}

/// Returns `(work, bytes)` for gradual-parameter construction, transfer, and owner retirement.
/// Returns `None` if either quotation overflows.
pub(in crate::types) fn gradual_parameters_quote() -> Option<(usize, usize)> {
    let quote = parameters_storage_quote(2)?;
    Some((
        quote.work.checked_add(12)?,
        quote
            .bytes
            .checked_add(size_of::<Parameters<'_>>())?
            .checked_add(size_of::<[Parameter<'_>; 2]>())?,
    ))
}

/// Supplies semantic children while the caller retains the original signature.
///
/// Every produced owner must have its eventual cleanup admitted before construction. Local
/// actions may borrow the retained signature; they must not hide database queries or mappings.
pub(in crate::types) trait ConstructorSignatureEffects<'db> {
    type Error;

    /// Admits local work and storage before running `action`.
    ///
    /// The provider adds the action and result representations to `requested_bytes`. Callers
    /// quote additional input or intermediate storage; either `None` quotation refuses before
    /// running the action.
    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Returns the work needed to destroy a signature, admitting only the quotation itself.
    /// The caller admits the returned work before replacing or destroying that signature.
    async fn signature_retirement(&self, signature: &Signature<'db>) -> Result<usize, Self::Error>;

    async fn contains_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;
    async fn synthetic_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn bind_self_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn bind_self_signature_types(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        parameters: &mut Parameters<'db>,
        return_type: &mut Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> Result<(), Self::Error>;
    async fn resolve_alias(&self, db: &'db dyn Db, ty: Type<'db>)
    -> Result<Type<'db>, Self::Error>;
    async fn receiver_violates_domain(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;
    async fn receiver_constraint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        annotation: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error>;
    async fn clone_constraints(
        &self,
        constraints: &OwnedConstraintSet<'db>,
    ) -> Result<OwnedConstraintSet<'db>, Self::Error>;
    async fn intersect_constraints(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: &OwnedConstraintSet<'db>,
        second: &OwnedConstraintSet<'db>,
    ) -> Result<Option<OwnedConstraintSet<'db>>, Self::Error>;
    async fn remove_self(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        binding_context: Option<BindingContext<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error>;
    async fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;
    async fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Self::Error>;
    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error>;
    async fn identity(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
    async fn bound_or_constraints(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
    async fn constraint_elements(
        &self,
        db: &'db dyn Db,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;
    async fn default_type(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn specialize_unused_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        variable: BoundTypeVarInstance<'db>,
        self_type: Type<'db>,
    ) -> Result<Signature<'db>, Self::Error>;
}

/// Runs receiver-signature children synchronously for ordinary execution.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InlineConstructorSignatureEffects;

impl<'db> ConstructorSignatureEffects<'db> for InlineConstructorSignatureEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn signature_retirement(
        &self,
        _signature: &Signature<'db>,
    ) -> Result<usize, Self::Error> {
        Ok(0)
    }

    async fn contains_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(ty.contains_self(db, env))
    }

    async fn synthetic_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            db,
            Type::object(),
            BindingContext::Synthetic(env.program(db)),
        )))
    }

    async fn bind_self_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> Result<Type<'db>, Self::Error> {
        let mapping = TypeMapping::BindSelf(SelfBinding::new(db, env, self_type, context));
        Ok(ty.apply_type_mapping(db, env, &mapping, TypeContext::default()))
    }

    async fn bind_self_signature_types(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        parameters: &mut Parameters<'db>,
        return_type: &mut Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> Result<(), Self::Error> {
        let mapping = TypeMapping::BindSelf(SelfBinding::new(db, env, self_type, context));
        *parameters = parameters.apply_type_mapping_impl(
            db,
            &mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        *return_type = return_type.apply_type_mapping(db, env, &mapping, TypeContext::default());
        Ok(())
    }

    async fn resolve_alias(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(db))
    }

    async fn receiver_violates_domain(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(Signature::receiver_violates_typevar_domain(
            db, env, receiver, typevar,
        ))
    }

    async fn receiver_constraint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        annotation: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error> {
        Ok(receiver.when_constraint_set_assignable_to_owned(db, env, annotation))
    }

    async fn clone_constraints(
        &self,
        constraints: &OwnedConstraintSet<'db>,
    ) -> Result<OwnedConstraintSet<'db>, Self::Error> {
        Ok(constraints.clone())
    }

    async fn intersect_constraints(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: &OwnedConstraintSet<'db>,
        second: &OwnedConstraintSet<'db>,
    ) -> Result<Option<OwnedConstraintSet<'db>>, Self::Error> {
        Ok(merge_receiver_constraints(
            db,
            env,
            Some(first),
            Some(second),
        ))
    }

    async fn remove_self(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        binding_context: Option<BindingContext<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(context.remove_self(db, binding_context))
    }

    async fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.typevar(db).is_self(db))
    }

    async fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(variable.typevar(db).upper_bound(db, env))
    }

    async fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(source.is_assignable_to(db, env, target))
    }

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(db)))
    }

    async fn identity(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error> {
        Ok(variable.identity(db))
    }

    async fn bound_or_constraints(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(variable.typevar(db).bound_or_constraints(db, env))
    }

    async fn constraint_elements(
        &self,
        db: &'db dyn Db,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(constraints.elements(db))
    }

    async fn default_type(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(variable.default_type(db))
    }

    async fn specialize_unused_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        variable: BoundTypeVarInstance<'db>,
        self_type: Type<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        let mapping =
            TypeMapping::ApplySpecialization(ApplySpecialization::Single(variable, self_type));
        Ok(signature.apply_type_mapping_impl(
            db,
            &mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        ))
    }
}

/// Returns whether the return type or a retained parameter annotation contains `Self`.
/// Checks the return type first, then parameters in source order, stopping at the first `Self`.
pub(super) async fn needs_self_mapping_with<'db, E: ConstructorSignatureEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signature: &Signature<'db>,
    receiver_is_removed: bool,
    effects: &E,
) -> Result<bool, E::Error> {
    let return_type = effects
        .local(Some(1), Some(0), || signature.return_ty)
        .await?;
    if effects.contains_self(db, env, return_type).await? {
        return effects.local(Some(1), Some(0), || true).await;
    }
    let mut parameters = effects
        .local(Some(2), Some(0), || {
            signature
                .parameters
                .iter()
                .skip(usize::from(receiver_is_removed))
        })
        .await?;
    while let Some(parameter) = effects
        .local(Some(2), Some(0), || parameters.next())
        .await?
    {
        let annotation = effects
            .local(Some(1), Some(0), || parameter.annotated_type())
            .await?;
        if effects.contains_self(db, env, annotation).await? {
            return effects.local(Some(1), Some(0), || true).await;
        }
    }
    effects.local(Some(1), Some(0), || false).await
}

/// Discards only terminal-true receiver constraints before cloning or intersecting them.
async fn merge_receiver_constraints_with<'db, E: ConstructorSignatureEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    first: Option<&OwnedConstraintSet<'db>>,
    second: Option<&OwnedConstraintSet<'db>>,
    effects: &E,
) -> Result<Option<OwnedConstraintSet<'db>>, E::Error> {
    let retained = effects
        .local(Some(4), Some(0), || {
            (
                first.filter(|constraints| !constraints.is_trivially_always_satisfied()),
                second.filter(|constraints| !constraints.is_trivially_always_satisfied()),
            )
        })
        .await?;
    match retained {
        (None, None) => effects.local(Some(1), Some(0), || None).await,
        (Some(constraints), None) | (None, Some(constraints)) => {
            let cloned = effects.clone_constraints(constraints).await?;
            effects.local(Some(1), Some(0), || Some(cloned)).await
        }
        (Some(first), Some(second)) => effects.intersect_constraints(db, env, first, second).await,
    }
}

/// Removes a positional receiver and preserves its annotation constraint and signature metadata.
///
/// `typing_self_type` substitutes `typing.Self` separately from the runtime `receiver_type`.
/// The original signature stays borrowed while children inspect or transform its owned data.
pub(in crate::types) async fn bind_self_with_receiver_with<
    'db,
    E: ConstructorSignatureEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signature: &Signature<'db>,
    receiver_type: Option<Type<'db>>,
    typing_self_type: Option<Type<'db>>,
    effects: &E,
) -> Result<Signature<'db>, E::Error> {
    let removed_receiver = effects
        .local(Some(3), Some(0), || {
            signature
                .parameters
                .get(0)
                .is_some_and(Parameter::is_positional)
        })
        .await?;
    let explicit_receiver = effects
        .local(Some(4), Some(0), || {
            signature
                .parameters
                .get(0)
                .filter(|parameter| parameter.is_positional() && !parameter.inferred_annotation)
        })
        .await?;

    // TODO: Theoretically, for a signature like `f(*args: *tuple[MyClass, int, *tuple[str, ...]])` with
    // a variadic first parameter, we should also "skip the first parameter" by modifying the tuple type.
    let parameter_count = effects
        .local(Some(2), Some(0), || {
            signature
                .parameters
                .len()
                .saturating_sub(usize::from(removed_receiver))
        })
        .await?;
    let quote = parameters_storage_quote(parameter_count);
    let work = quote
        .map(|quote| quote.work)
        .and_then(|work| work.checked_add(parameter_count.checked_mul(8)?))
        .and_then(|work| work.checked_add(12));
    let bytes = if removed_receiver {
        quote.map(|quote| quote.bytes)
    } else {
        Some(0)
    };
    let mut parameters = effects
        .local(work, bytes, || {
            if removed_receiver {
                signature.parameters.without_first()
            } else {
                signature.parameters.clone()
            }
        })
        .await?;
    let mut return_ty = effects
        .local(Some(1), Some(0), || signature.return_ty)
        .await?;
    let binding_context = effects
        .local(Some(2), Some(0), || {
            signature.definition.map(BindingContext::Definition)
        })
        .await?;
    let receiver_constraint = if let Some(parameter) = explicit_receiver {
        let receiver = match receiver_type {
            Some(receiver) => receiver,
            None => effects.synthetic_receiver(db, env).await?,
        };
        let annotation = effects
            .local(Some(1), Some(0), || parameter.annotated_type())
            .await?;
        let annotation = if let Some(typing_self_type) = typing_self_type {
            effects
                .bind_self_type(db, env, annotation, typing_self_type, binding_context)
                .await?
        } else {
            annotation
        };
        // TODO: Also intersect nested receiver type variables, such as the `T` in
        // `self: list[T]`, with their valid specializations when constructing or solving the
        // receiver constraint set.
        let receiver_typevar = match annotation {
            Type::TypeVar(typevar) => Some(typevar),
            Type::TypeAlias(_) => {
                let resolved = effects.resolve_alias(db, annotation).await?;
                effects
                    .local(Some(1), Some(0), || resolved.as_typevar())
                    .await?
            }
            _ => None,
        };
        let violates_domain = if let Some(typevar) = receiver_typevar {
            effects
                .receiver_violates_domain(db, env, receiver, typevar)
                .await?
        } else {
            false
        };
        let constraint = if violates_domain {
            effects
                .local(Some(2), Some(0), || {
                    Cow::Owned(OwnedConstraintSet::default())
                })
                .await?
        } else {
            effects
                .receiver_constraint(db, env, receiver, annotation)
                .await?
        };
        effects.local(Some(1), Some(0), || Some(constraint)).await?
    } else {
        effects.local(Some(1), Some(0), || None).await?
    };
    let (existing_constraints, added_constraint) = effects
        .local(Some(4), Some(0), || {
            (
                signature.receiver_constraints(),
                receiver_constraint.as_deref(),
            )
        })
        .await?;
    let receiver_constraints =
        merge_receiver_constraints_with(db, env, existing_constraints, added_constraint, effects)
            .await?;
    if let Some(self_type) = typing_self_type
        && needs_self_mapping_with(db, env, signature, removed_receiver, effects).await?
    {
        effects
            .bind_self_signature_types(
                db,
                env,
                &mut parameters,
                &mut return_ty,
                self_type,
                binding_context,
            )
            .await?;
    }
    let context = effects
        .local(Some(1), Some(0), || signature.generic_context)
        .await?;
    let generic_context = match context {
        Some(context) => Some(effects.remove_self(db, context, binding_context).await?),
        None => None,
    };
    let extras_bytes = effects
        .local(Some(3), Some(0), || {
            usize::from(
                signature.source_overload_index_raw().is_some() || receiver_constraints.is_some(),
            ) * size_of::<SignatureExtras<'db>>()
        })
        .await?;
    effects
        .local(Some(12), Some(extras_bytes), || Signature {
            generic_context,
            definition: signature.definition,
            extras: SignatureExtras::new(
                signature.source_overload_index_raw(),
                receiver_constraints,
            ),
            parameters,
            return_ty,
            is_paramspec_value: signature.is_paramspec_value,
            is_recursion_recovery: signature.is_recursion_recovery,
        })
        .await
}

/// Binds the `Self` receiver if it is unused in the rest of the signature.
///
/// This is purely a performance optimization. Eagerly binding the type of `Self` prevents
/// unnecessary work from being performed by the constraint solver.
pub(in crate::types) async fn bind_unused_self_with<'db, E: ConstructorSignatureEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signature: &Signature<'db>,
    self_type: Type<'db>,
    effects: &E,
) -> Result<Option<Signature<'db>>, E::Error> {
    let Some(context) = effects
        .local(Some(1), Some(0), || signature.generic_context)
        .await?
    else {
        return effects.local(Some(1), Some(0), || None).await;
    };
    let Some(receiver) = effects
        .local(Some(1), Some(0), || signature.parameters.get(0))
        .await?
    else {
        return effects.local(Some(1), Some(0), || None).await;
    };

    // Ensure `Self` is not used elsewhere in the signature, in which case eagerly binding it
    // would be unsound.
    if !effects
        .local(Some(1), Some(0), || receiver.is_positional())
        .await?
        || needs_self_mapping_with(db, env, signature, true, effects).await?
    {
        return effects.local(Some(1), Some(0), || None).await;
    }

    // Extract the `Self` type variable.
    let variable = effects
        .local(Some(3), Some(0), || match receiver.annotated_type() {
            Type::TypeVar(typevar) => Some(typevar),
            Type::SubclassOf(subclass) => subclass.into_type_var(),
            _ => None,
        })
        .await?;
    let Some(self_typevar) = variable else {
        return effects.local(Some(1), Some(0), || None).await;
    };
    if !effects.is_self(db, self_typevar).await? {
        return effects.local(Some(1), Some(0), || None).await;
    }

    // Also ensure that the receiver satisfies the upper bound of `Self`.
    let Some(bound) = effects.upper_bound(db, env, self_typevar).await? else {
        return effects.local(Some(1), Some(0), || None).await;
    };
    if !effects.is_assignable(db, env, self_type, bound).await? {
        return effects.local(Some(1), Some(0), || None).await;
    }

    // And that `Self` is not referenced by any other type variable, in which case removing it
    // from the generic context may leave it unspecialized.
    //
    // TODO: References to `Self` inside of bounds or defaults should not generally be permitted
    // in the first place, but we still avoid leaving dangling references to `Self` out of principle.
    let variables = effects.variables(db, context).await?;
    let mut variables = effects
        .local(Some(1), Some(0), || variables.values().copied())
        .await?;
    while let Some(typevar) = effects.local(Some(2), Some(0), || variables.next()).await? {
        let identity = effects.identity(db, typevar).await?;
        let self_identity = effects.identity(db, self_typevar).await?;
        if effects
            .local(Some(1), Some(0), || identity == self_identity)
            .await?
        {
            continue;
        }
        if let Some(bound) = effects.bound_or_constraints(db, env, typevar).await? {
            match bound {
                TypeVarBoundOrConstraints::UpperBound(bound) => {
                    if effects.contains_self(db, env, bound).await? {
                        return effects.local(Some(1), Some(0), || None).await;
                    }
                }
                TypeVarBoundOrConstraints::Constraints(constraints) => {
                    let elements = effects.constraint_elements(db, constraints).await?;
                    let mut elements = effects
                        .local(Some(1), Some(0), || elements.iter().copied())
                        .await?;
                    while let Some(element) =
                        effects.local(Some(2), Some(0), || elements.next()).await?
                    {
                        if effects.contains_self(db, env, element).await? {
                            return effects.local(Some(1), Some(0), || None).await;
                        }
                    }
                }
            }
        }
        if let Some(default) = effects.default_type(db, typevar).await?
            && effects.contains_self(db, env, default).await?
        {
            return effects.local(Some(1), Some(0), || None).await;
        }
    }
    let specialized = effects
        .specialize_unused_self(db, env, signature, self_typevar, self_type)
        .await?;
    effects.local(Some(1), Some(0), || Some(specialized)).await
}
