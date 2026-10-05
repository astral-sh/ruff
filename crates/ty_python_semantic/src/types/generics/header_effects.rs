//! Declaration dependencies of PEP 695 generic-context construction.
//!
//! The inline provider keeps the ordinary declaration inference and recovery behavior. Queued
//! providers consume completed canonical header facts and propagate unfinished work as an error
//! or suspension, so an unavailable header cannot silently remove a type parameter.

use std::convert::Infallible;
use std::marker::PhantomData;

use ty_python_core::definition::Definition;

use super::context_construction::{
    ContextConstructionControl, ContextConstructionEffects, ContextConstructionWork,
    ContextVariables, context_from_typevars_with,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext, KnownInstanceType, Type,
    inferred_declaration,
};
use crate::{Db, Program, ProgramEnvironment};

pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

/// A completed declaration lookup, including the ordinary checker's malformed-source recovery.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TypeParameterDeclaration<'db> {
    Variable(TypeVarInstance<'db>),
    /// The completed declaration is rejected or does not declare a type variable.
    Rejected,
}

/// Supplies canonical declarations and binds them to their owning generic definition.
pub(crate) trait TypeParameterEffects<'db>: sealed::Sealed {
    type Error;

    /// Prepares the complete header batch before its first declaration is awaited.
    ///
    /// Queued providers declare their independent requests here. Canonical source providers
    /// receive definitions already captured from the prepared syntax; borrowing the iterator
    /// avoids copying that batch.
    async fn prepare_headers<I>(&self, definitions: &I) -> Result<usize, Self::Error>
    where
        I: ExactSizeIterator<Item = Definition<'db>> + Clone;

    /// Advances through the prepared definition batch in source order.
    async fn next_definition<I>(
        &self,
        definitions: &mut I,
    ) -> Result<Option<Definition<'db>>, Self::Error>
    where
        I: Iterator<Item = Definition<'db>>;

    /// Returns a completed declaration, or preserves the unresolved header dependency.
    async fn type_parameter(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<TypeParameterDeclaration<'db>, Self::Error>;

    /// Attaches the generic definition's binding context to a completed parameter.
    async fn bind(
        &self,
        db: &'db dyn Db,
        variable: TypeVarInstance<'db>,
        binding: Definition<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
}

/// Uses ordinary inference for each declaration consumed by generic-context construction.
#[derive(Debug)]
pub(crate) struct LegacyInlineEffects;

impl sealed::Sealed for LegacyInlineEffects {}

impl ContextConstructionControl for LegacyInlineEffects {
    type Error = Infallible;

    fn checkpoint(&self, _: ContextConstructionWork) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<'db> TypeParameterEffects<'db> for LegacyInlineEffects {
    type Error = Infallible;

    async fn prepare_headers<I>(&self, definitions: &I) -> Result<usize, Self::Error>
    where
        I: ExactSizeIterator<Item = Definition<'db>> + Clone,
    {
        Ok(definitions.len())
    }

    async fn next_definition<I>(
        &self,
        definitions: &mut I,
    ) -> Result<Option<Definition<'db>>, Self::Error>
    where
        I: Iterator<Item = Definition<'db>>,
    {
        Ok(definitions.next())
    }

    async fn type_parameter(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<TypeParameterDeclaration<'db>, Self::Error> {
        Ok(
            if let Some(declared) = inferred_declaration(db, definition).declared()
                && let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declared.inner_type()
            {
                TypeParameterDeclaration::Variable(typevar)
            } else {
                TypeParameterDeclaration::Rejected
            },
        )
    }

    async fn bind(
        &self,
        db: &'db dyn Db,
        variable: TypeVarInstance<'db>,
        binding: Definition<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        Ok(variable.with_binding_context(db, binding))
    }
}

/// Adapts declaration inference to the existing ordered-context construction algorithm.
///
/// Preparing the whole batch precedes map allocation and the first declaration dependency.
/// A completed rejected declaration is skipped; an unfinished declaration propagates its error.
struct HeaderContext<'effects, 'db, E, C, I> {
    db: &'db dyn Db,
    binding: Definition<'db>,
    declarations: &'effects E,
    context: &'effects C,
    input: PhantomData<I>,
}

impl<'db, E, C, I> ContextConstructionEffects<'db> for HeaderContext<'_, 'db, E, C, I>
where
    E: TypeParameterEffects<'db>,
    C: ContextConstructionEffects<'db, Error = E::Error>,
    I: ExactSizeIterator<Item = Definition<'db>> + Clone,
{
    type Error = E::Error;
    type Input = I;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error> {
        self.context.program(env).await
    }

    async fn input_lower_bound(&self, input: &I) -> Result<usize, Self::Error> {
        self.declarations.prepare_headers(input).await
    }

    async fn next_variable(
        &self,
        input: &mut I,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        while let Some(definition) = self.declarations.next_definition(input).await? {
            match self
                .declarations
                .type_parameter(self.db, definition)
                .await?
            {
                TypeParameterDeclaration::Variable(variable) => {
                    return self
                        .declarations
                        .bind(self.db, variable, self.binding)
                        .await
                        .map(Some);
                }
                TypeParameterDeclaration::Rejected => {}
            }
        }
        Ok(None)
    }

    async fn new_variables(
        &self,
        lower_bound: usize,
    ) -> Result<ContextVariables<'db>, Self::Error> {
        self.context.new_variables(lower_bound).await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error> {
        self.context.identity(variable).await
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        self.context.insert(variables, identity, variable).await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> Result<(), Self::Error> {
        self.context.shrink(variables).await
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        self.context.intern(program, variables).await
    }

    async fn publish(
        &self,
        context: GenericContext<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        self.context.publish(context).await
    }
}

/// Creates a context from prepared definitions with the ordinary declaration recovery rules.
pub(super) async fn context_from_headers_with<'db, E, C, I>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    binding: Definition<'db>,
    definitions: I,
    declarations: &E,
    context: &C,
) -> Result<GenericContext<'db>, E::Error>
where
    E: TypeParameterEffects<'db>,
    C: ContextConstructionEffects<'db, Error = E::Error>,
    I: ExactSizeIterator<Item = Definition<'db>> + Clone,
{
    context_from_typevars_with(
        db,
        env,
        definitions,
        &HeaderContext {
            db,
            binding,
            declarations,
            context,
            input: PhantomData,
        },
    )
    .await
}
