//! Construct a generic context from variables in encounter order.

use std::convert::Infallible;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::interned::FiniteInternedConfiguration;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::GenericContext;
use crate::types::{BoundTypeVarIdentity, BoundTypeVarInstance};
use crate::{Db, FxOrderMap, Program, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ContextConstructionWork {
    Program,
    InitialCapacity { lower_bound: usize },
    Advance,
    Identity,
    Insert { len: usize, capacity: usize },
    Shrink { len: usize, capacity: usize },
    Intern { len: usize },
    Publish,
}

pub(in crate::types) trait ContextConstructionControl {
    type Error;

    fn checkpoint(&self, work: ContextConstructionWork) -> Result<(), Self::Error>;
}

pub(super) struct Unrestricted;

impl ContextConstructionControl for Unrestricted {
    type Error = Infallible;

    fn checkpoint(&self, _: ContextConstructionWork) -> Result<(), Infallible> {
        Ok(())
    }
}

pub(in crate::types) type ContextVariables<'db> =
    FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousContextConstructionEffects)]
    pub(in crate::types) trait ContextConstructionEffects<'db> {
        type Error;
        type Input;

        #[operation(source)]
        async fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn input_lower_bound(&self, input: &Self::Input) -> Result<usize, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, input: &mut Self::Input) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_variables(&self, lower_bound: usize) -> Result<ContextVariables<'db>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn insert(&self, variables: &mut ContextVariables<'db>, identity: BoundTypeVarIdentity<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn shrink(&self, variables: &mut ContextVariables<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern(&self, program: Program<'db>, variables: ContextVariables<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(local)]
        async fn publish(&self, context: GenericContext<'db>) -> Result<GenericContext<'db>, Self::Error>;
    }

    #[synchronous(context_from_typevars_sync)]
    #[capabilities(effects = ContextConstructionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn context_from_typevars_with<'db, E: ContextConstructionEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut input: E::Input,
        effects: &E,
    ) -> Result<GenericContext<'db>, E::Error> {
        let _ = db;
        let program = effects.program(env).await?;
        let lower_bound = effects.input_lower_bound(&input).await?;
        let mut variables = effects.new_variables(lower_bound).await?;
        #[cursor_loop]
        while let Some(variable) = effects.next_variable(&mut input).await? {
            let identity = effects.identity(variable).await?;
            effects.insert(&mut variables, identity, variable).await?;
        }
        effects.shrink(&mut variables).await?;
        let context = effects.intern(program, variables).await?;
        effects.publish(context).await
    }
}

pub(in crate::types) struct InlineContextConstruction<'control, 'db, C, I> {
    db: &'db dyn Db,
    control: &'control C,
    program: Program<'db>,
    input: PhantomData<I>,
}

impl<'control, 'db, C, I> InlineContextConstruction<'control, 'db, C, I> {
    /// Supplies the ordinary ordered-map operations to a shared context input adapter.
    pub(in crate::types) const fn new(
        db: &'db dyn Db,
        control: &'control C,
        program: Program<'db>,
    ) -> Self {
        Self {
            db,
            control,
            program,
            input: PhantomData,
        }
    }
}

impl<'db, C, I> ContextConstructionEffects<'db> for InlineContextConstruction<'_, 'db, C, I>
where
    C: ContextConstructionControl,
    I: Iterator<Item = BoundTypeVarInstance<'db>>,
{
    type Error = C::Error;
    type Input = I;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error> {
        SynchronousContextConstructionEffects::program(self, env)
    }

    async fn input_lower_bound(&self, input: &I) -> Result<usize, Self::Error> {
        SynchronousContextConstructionEffects::input_lower_bound(self, input)
    }

    async fn next_variable(
        &self,
        input: &mut I,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        SynchronousContextConstructionEffects::next_variable(self, input)
    }

    async fn new_variables(
        &self,
        lower_bound: usize,
    ) -> Result<ContextVariables<'db>, Self::Error> {
        SynchronousContextConstructionEffects::new_variables(self, lower_bound)
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error> {
        SynchronousContextConstructionEffects::identity(self, variable)
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        SynchronousContextConstructionEffects::insert(self, variables, identity, variable)
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> Result<(), Self::Error> {
        SynchronousContextConstructionEffects::shrink(self, variables)
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        SynchronousContextConstructionEffects::intern(self, program, variables)
    }

    async fn publish(
        &self,
        context: GenericContext<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        SynchronousContextConstructionEffects::publish(self, context)
    }
}

impl<'db, C, I> SynchronousContextConstructionEffects<'db>
    for InlineContextConstruction<'_, 'db, C, I>
where
    C: ContextConstructionControl,
    I: Iterator<Item = BoundTypeVarInstance<'db>>,
{
    type Error = C::Error;
    type Input = I;

    fn program(&self, _env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(self.program)
    }

    fn input_lower_bound(&self, input: &I) -> Result<usize, Self::Error> {
        Ok(input.size_hint().0)
    }

    fn next_variable(
        &self,
        input: &mut I,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Advance)?;
        Ok(input.next())
    }

    fn new_variables(&self, lower_bound: usize) -> Result<ContextVariables<'db>, Self::Error> {
        self.control
            .checkpoint(ContextConstructionWork::InitialCapacity { lower_bound })?;
        Ok(FxOrderMap::with_capacity_and_hasher(
            lower_bound,
            BuildHasherDefault::default(),
        ))
    }

    fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Identity)?;
        Ok(variable.identity(self.db))
    }

    fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Insert {
            len: variables.len(),
            capacity: variables.capacity(),
        })?;
        variables.insert(identity, variable);
        Ok(())
    }

    fn shrink(&self, variables: &mut ContextVariables<'db>) -> Result<(), Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Shrink {
            len: variables.len(),
            capacity: variables.capacity(),
        })?;
        variables.shrink_to_fit();
        Ok(())
    }

    fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Intern {
            len: variables.len(),
        })?;
        Ok(GenericContext::new_internal(self.db, program, variables))
    }

    fn publish(&self, context: GenericContext<'db>) -> Result<GenericContext<'db>, Self::Error> {
        self.control.checkpoint(ContextConstructionWork::Publish)?;
        Ok(context)
    }
}

impl<'db> GenericContext<'db> {
    pub(in crate::types) fn from_typevar_instances_with<C: ContextConstructionControl>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        type_params: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
        control: &C,
    ) -> Result<Self, C::Error> {
        control.checkpoint(ContextConstructionWork::Program)?;
        Self::from_typevar_instances_in_program_with(db, env.program(db), type_params, control)
    }

    pub(super) fn from_typevar_instances_in_program_with<C: ContextConstructionControl>(
        db: &'db dyn Db,
        program: Program<'db>,
        type_params: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
        control: &C,
    ) -> Result<Self, C::Error> {
        context_from_typevars_sync(
            db,
            &ProgramEnvironment::from_program(program),
            type_params.into_iter(),
            &InlineContextConstruction {
                db,
                control,
                program,
                input: PhantomData,
            },
        )
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for GenericContext<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        fields
            .1
            .len()
            .checked_mul(5)?
            .checked_add(fields.1.capacity())?
            .checked_add(2)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) type GenericContextValues<'db> =
    InternedValues<'db, GenericContext<'static>, ()>;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn register_generic_context_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<GenericContextValues<'db>> {
    registry.finite_interned_values_with_memos(GenericContext::ingredient(db.zalsa()), ())
}
