//! Installs an implicit receiver annotation and retains its type variable in the signature context.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::Arc;

use super::Signature;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::{BoundTypeVarInstance, GenericContext, Type, TypeVarKind};
use crate::{Db, ProgramEnvironment};

/// The context work remaining after the first parameter's annotation has been replaced.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InstalledReceiver<'db> {
    pub(in crate::types) variable: BoundTypeVarInstance<'db>,
    pub(in crate::types) context: Option<GenericContext<'db>>,
}

/// Returns whether the first parameter can receive an inferred receiver annotation.
pub(in crate::types) fn receiver_is_eligible(signature: &Signature<'_>) -> bool {
    signature.parameters.data.value.first().is_some_and(|parameter| {
        parameter.is_positional()
            && parameter.annotated_type.is_unknown()
            && parameter.inferred_annotation
    })
}

/// Replaces the first annotation, returning its type variable and existing context when needed.
/// Controlled callers admit any copy of the shared parameter buffer before calling this function.
pub(in crate::types) fn install_annotation<'db>(
    signature: &mut Signature<'db>,
    receiver: Type<'db>,
) -> Option<InstalledReceiver<'db>> {
    let parameter = Arc::make_mut(&mut signature.parameters.data).value.first_mut()?;
    parameter.annotated_type = receiver;
    let variable = match receiver {
        Type::TypeVar(variable) => Some(variable),
        Type::SubclassOf(subclass_of) => subclass_of.into_type_var(),
        _ => None,
    }?;
    Some(InstalledReceiver {
        variable,
        context: signature.generic_context,
    })
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousExplicitSelfEffects)]
    pub(in crate::types) trait ExplicitSelfEffects<'db> {
        type Error;

        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
    }

    #[synchronous(SynchronousImplicitReceiverEffects)]
    pub(in crate::types) trait ImplicitReceiverEffects<'db> {
        type Error;
        type Receiver;

        #[operation(local)]
        async fn eligible(&self, signature: &Signature<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn receiver(&self, input: Self::Receiver) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn install_annotation(&self, signature: &mut Signature<'db>, receiver: Type<'db>) -> Result<Option<InstalledReceiver<'db>>, Self::Error>;
        #[operation(child)]
        async fn binds_receiver(&self, context: GenericContext<'db>, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(child)]
        async fn prepend_context(&self, env: &ProgramEnvironment<'db>, variable: BoundTypeVarInstance<'db>, variables: Option<&'db ContextVariables<'db>>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(local)]
        async fn set_context(&self, signature: &mut Signature<'db>, context: GenericContext<'db>) -> Result<(), Self::Error>;
    }

    /// Returns whether the stored context contains a `typing.Self` variable.
    #[synchronous(context_has_explicit_self_sync)]
    #[capabilities(effects = ExplicitSelfEffects)]
    #[passive_values()]
    pub(in crate::types) async fn context_has_explicit_self_with<'db, E: ExplicitSelfEffects<'db>>(
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(variable) = effects.next_variable(variables, &mut cursor).await? {
            if matches!(effects.variable_kind(variable).await?, TypeVarKind::TypingSelf) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Installs an eligible receiver, then prepends its type variable if the context lacks its identity.
    /// Receiver selection runs only after eligibility; an absent receiver leaves the signature unchanged.
    #[synchronous(install_implicit_receiver_sync)]
    #[capabilities(effects = ImplicitReceiverEffects)]
    #[passive_values()]
    pub(in crate::types) async fn install_implicit_receiver_with<'db, E: ImplicitReceiverEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        signature: &mut Signature<'db>,
        input: E::Receiver,
        effects: &E,
    ) -> Result<(), E::Error> {
        if !effects.eligible(signature).await? {
            return Ok(());
        }
        let Some(receiver) = effects.receiver(input).await? else {
            return Ok(());
        };
        let Some(InstalledReceiver { variable, context }) = effects.install_annotation(signature, receiver).await? else {
            return Ok(());
        };

        // If we've added an implicit `self` annotation, we might need to update the
        // signature's generic context, too. (The generic context should include any synthetic
        // typevars created for `typing.Self`, even if the `typing.Self` annotation was added
        // implicitly.)
        let variables = match context {
            Some(context) => {
                if effects.binds_receiver(context, variable).await? {
                    return Ok(());
                }
                Some(effects.variables(context).await?)
            }
            None => None,
        };
        let context = effects.prepend_context(env, variable, variables).await?;
        effects.set_context(signature, context).await
    }
}

#[derive(Clone, Copy)]
struct OrdinaryExplicitSelfEffects<'db>(&'db dyn Db);

impl<'db> SynchronousExplicitSelfEffects<'db> for OrdinaryExplicitSelfEffects<'db> {
    type Error = Infallible;

    fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(self.0)))
    }

    fn next_variable(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let variable = GenericContext::variable_at_in(variables, *cursor);
        if variable.is_some() {
            *cursor += 1;
        }
        Ok(variable)
    }

    fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(variable.typevar(self.0).kind(self.0))
    }
}

/// Checks an ordinary signature context for `typing.Self` in stored variable order.
pub(in crate::types) fn context_has_explicit_self<'db>(db: &'db dyn Db, context: GenericContext<'db>) -> bool {
    match context_has_explicit_self_sync(context, &OrdinaryExplicitSelfEffects(db)) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

#[derive(Clone, Copy)]
struct OrdinaryImplicitReceiverEffects<'db, F> {
    db: &'db dyn Db,
    receiver: PhantomData<F>,
}

impl<'db, F: FnOnce() -> Option<Type<'db>>> SynchronousImplicitReceiverEffects<'db>
    for OrdinaryImplicitReceiverEffects<'db, F>
{
    type Error = Infallible;
    type Receiver = F;

    fn eligible(&self, signature: &Signature<'db>) -> Result<bool, Infallible> {
        Ok(receiver_is_eligible(signature))
    }

    fn receiver(&self, input: F) -> Result<Option<Type<'db>>, Infallible> {
        Ok(input())
    }

    fn install_annotation(&self, signature: &mut Signature<'db>, receiver: Type<'db>) -> Result<Option<InstalledReceiver<'db>>, Infallible> {
        Ok(install_annotation(signature, receiver))
    }

    fn binds_receiver(&self, context: GenericContext<'db>, variable: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(context.binds_typevar(self.db, variable.typevar(self.db)).is_some())
    }

    fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(self.db)))
    }

    fn prepend_context(&self, env: &ProgramEnvironment<'db>, variable: BoundTypeVarInstance<'db>, variables: Option<&'db ContextVariables<'db>>) -> Result<GenericContext<'db>, Infallible> {
        let first = std::iter::once(variable);
        Ok(match variables {
            Some(variables) => GenericContext::from_typevar_instances(self.db, env, first.chain(variables.values().copied())),
            None => GenericContext::from_typevar_instances(self.db, env, first),
        })
    }

    fn set_context(&self, signature: &mut Signature<'db>, context: GenericContext<'db>) -> Result<(), Infallible> {
        signature.generic_context = Some(context);
        Ok(())
    }
}

/// Runs receiver installation synchronously while preserving the lazy, once-only callback.
pub(super) fn add_implicit_receiver<'db, F: FnOnce() -> Option<Type<'db>>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signature: &mut Signature<'db>,
    receiver: F,
) {
    let effects = OrdinaryImplicitReceiverEffects { db, receiver: PhantomData };
    match install_implicit_receiver_sync(env, signature, receiver, &effects) {
        Ok(()) => (),
        Err(never) => match never {},
    }
}
