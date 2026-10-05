//! Specialize a context with its unchanged bound variables in encounter order.

use std::convert::Infallible;

use super::context_construction::ContextVariables;
use crate::Db;
use crate::types::{GenericContext, Specialization, Type};

#[cfg(test)]
pub(in crate::types) mod observations;

/// Source arity and output storage retained by an identity collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct IdentityStorage {
    pub(in crate::types) expected: usize,
    pub(in crate::types) len: usize,
    pub(in crate::types) capacity: usize,
}

/// Retains borrowed context variables and the flat argument buffer while they are collected.
#[derive(Debug)]
pub(in crate::types) struct IdentityArguments<'db> {
    variables: &'db ContextVariables<'db>,
    next: usize,
    types: Vec<Type<'db>>,
    #[cfg(test)]
    observation: Option<observations::ArgumentsLifetime>,
}

impl<'db> IdentityArguments<'db> {
    /// Reserves the complete argument count; each step can then append without growing the buffer.
    pub(in crate::types) fn new(variables: &'db ContextVariables<'db>) -> Self {
        Self {
            variables,
            next: 0,
            types: Vec::with_capacity(variables.len()),
            #[cfg(test)]
            observation: None,
        }
    }

    /// Appends the next unchanged bound occurrence, returning None only after the complete context.
    pub(in crate::types) fn append_next(&mut self) -> Option<()> {
        let variable = GenericContext::variable_at_in(self.variables, self.next)?;
        self.next += 1;
        self.types.push(Type::TypeVar(variable));
        Some(())
    }

    /// Reports the source arity, initialized output length and retained capacity for finishing.
    pub(in crate::types) fn storage(&self) -> IdentityStorage {
        IdentityStorage {
            expected: self.variables.len(),
            len: self.types.len(),
            capacity: self.types.capacity(),
        }
    }

    /// Transfers the completed argument buffer without changing any stored type occurrence.
    pub(in crate::types) fn into_types(self) -> Vec<Type<'db>> {
        #[cfg(test)]
        if let Some(mut observation) = self.observation {
            observation.transferred();
        }
        self.types
    }

    #[cfg(test)]
    /// Records collection retirement after its Vec drops, or marks that the Vec was transferred.
    pub(in crate::types) fn observe(&mut self, db: &dyn Db) {
        self.observation = Some(observations::ArgumentsLifetime::new(db));
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies the context's complete ordered variables and preserves each bound occurrence.
    /// `start` uses that context's variables; `append_next` finishes only after all are appended.
    /// `finish` checks arity and interns the same context with no materialization or tuple fields.
    #[synchronous(SynchronousIdentitySpecializationEffects)]
    pub(in crate::types) trait IdentitySpecializationEffects<'db> {
        type Error;

        #[operation(source)]
        async fn start(&self, context: GenericContext<'db>) -> Result<IdentityArguments<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn append_next(&self, arguments: &mut IdentityArguments<'db>) -> Result<Option<()>, Self::Error>;
        #[operation(child)]
        async fn finish(&self, context: GenericContext<'db>, arguments: IdentityArguments<'db>) -> Result<Specialization<'db>, Self::Error>;
    }

    /// Constructs the identity specialization, preserving existing bound occurrences without freshening.
    /// It does not resolve their bounds or defaults.
    #[synchronous(identity_specialization_sync)]
    #[capabilities(effects = IdentitySpecializationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn identity_specialization_with<'db, E: IdentitySpecializationEffects<'db>>(
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<Specialization<'db>, E::Error> {
        let mut arguments = effects.start(context).await?;
        #[cursor_loop]
        while let Some(_appended) = effects.append_next(&mut arguments).await? {}
        effects.finish(context, arguments).await
    }
}

pub(super) struct OrdinaryIdentityEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousIdentitySpecializationEffects<'db> for OrdinaryIdentityEffects<'db> {
    type Error = Infallible;

    fn start(&self, context: GenericContext<'db>) -> Result<IdentityArguments<'db>, Self::Error> {
        Ok(IdentityArguments::new(context.variables_inner(self.db)))
    }

    fn append_next(
        &self,
        arguments: &mut IdentityArguments<'db>,
    ) -> Result<Option<()>, Self::Error> {
        Ok(arguments.append_next())
    }

    fn finish(
        &self,
        context: GenericContext<'db>,
        arguments: IdentityArguments<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(context.specialize(self.db, arguments.into_types()))
    }
}
