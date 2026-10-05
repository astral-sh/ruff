//! Construct canonical inferable-variable sets in encounter order.

use std::convert::Infallible;
use std::iter::Peekable;
use std::marker::PhantomData;

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::interned::FiniteInternedConfiguration;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarSet, TypeVarSetInner};
use crate::{Db, FxOrderMap};

pub(in crate::types) type TypeVarSetVariables<'db> =
    FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarSetConstructionEffects)]
    /// Supplies identity reads and storage operations for canonical inferable-variable sets.
    pub(in crate::types) trait TypeVarSetConstructionEffects<'db> {
        type Error;
        type Input;

        #[operation(local)]
        async fn is_empty(&self, input: &mut Self::Input) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn empty(&self) -> Result<TypeVarSet<'db>, Self::Error>;
        #[operation(local)]
        async fn new_variables(&self) -> Result<TypeVarSetVariables<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, input: &mut Self::Input) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn insert(&self, variables: &mut TypeVarSetVariables<'db>, identity: BoundTypeVarIdentity<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn shrink(&self, variables: &mut TypeVarSetVariables<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern(&self, variables: TypeVarSetVariables<'db>) -> Result<TypeVarSetInner<'db>, Self::Error>;
        #[operation(local)]
        async fn publish(&self, inner: TypeVarSetInner<'db>) -> Result<TypeVarSet<'db>, Self::Error>;
    }

    #[synchronous(typevar_set_from_typevars_sync)]
    #[capabilities(effects = TypeVarSetConstructionEffects)]
    #[passive_values()]
    /// Retains the first bound instance of each identity, then interns the ordered set.
    pub(in crate::types) async fn typevar_set_from_typevars_with<'db, E: TypeVarSetConstructionEffects<'db>>(
        mut input: E::Input,
        effects: &E,
    ) -> Result<TypeVarSet<'db>, E::Error> {
        if effects.is_empty(&mut input).await? {
            return effects.empty().await;
        }
        let mut variables = effects.new_variables().await?;
        #[cursor_loop]
        while let Some(variable) = effects.next_variable(&mut input).await? {
            let identity = effects.identity(variable).await?;
            effects.insert(&mut variables, identity, variable).await?;
        }
        effects.shrink(&mut variables).await?;
        let inner = effects.intern(variables).await?;
        effects.publish(inner).await
    }
}

/// Runs set construction against ordinary Salsa field access and interning.
struct InlineTypeVarSetConstruction<'db, I> {
    db: &'db dyn Db,
    input: PhantomData<I>,
}

impl<'db, I> SynchronousTypeVarSetConstructionEffects<'db> for InlineTypeVarSetConstruction<'db, I>
where
    I: Iterator<Item = BoundTypeVarInstance<'db>>,
{
    type Error = Infallible;
    type Input = Peekable<I>;

    fn is_empty(&self, input: &mut Self::Input) -> Result<bool, Infallible> {
        Ok(input.peek().is_none())
    }

    fn empty(&self) -> Result<TypeVarSet<'db>, Infallible> {
        Ok(TypeVarSet::None)
    }

    fn new_variables(&self) -> Result<TypeVarSetVariables<'db>, Infallible> {
        Ok(FxOrderMap::default())
    }

    fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(input.next())
    }

    fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(variable.identity(self.db))
    }

    fn insert(
        &self,
        variables: &mut TypeVarSetVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        variables.entry(identity).or_insert(variable);
        Ok(())
    }

    fn shrink(&self, variables: &mut TypeVarSetVariables<'db>) -> Result<(), Infallible> {
        variables.shrink_to_fit();
        Ok(())
    }

    fn intern(
        &self,
        variables: TypeVarSetVariables<'db>,
    ) -> Result<TypeVarSetInner<'db>, Infallible> {
        Ok(TypeVarSetInner::new_internal(self.db, variables))
    }

    fn publish(&self, inner: TypeVarSetInner<'db>) -> Result<TypeVarSet<'db>, Infallible> {
        Ok(TypeVarSet::Some(inner))
    }
}

/// Constructs a canonical inferable-variable set through the shared synchronous decisions.
pub(super) fn from_typevars<'db>(
    db: &'db dyn Db,
    typevars: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
) -> TypeVarSet<'db> {
    match typevar_set_from_typevars_sync(
        typevars.into_iter().peekable(),
        &InlineTypeVarSetConstruction {
            db,
            input: PhantomData,
        },
    ) {
        Ok(set) => set,
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for TypeVarSetInner<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Each entry contains one fixed bound identity and one interned instance handle.
        // Neither hashing nor equality follows the identity's referenced definitions.
        fields.0.len().checked_mul(12)?.checked_add(1)
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
pub(in crate::types) type TypeVarSetValues<'db> = InternedValues<'db, TypeVarSetInner<'static>, ()>;

#[cfg(any(test, feature = "experimental-analysis"))]
/// Registers the existing set ingredient for controlled canonical value construction.
pub(in crate::types) fn register_typevar_set_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<TypeVarSetValues<'db>> {
    registry.finite_interned_values_with_memos(TypeVarSetInner::ingredient(db.zalsa()), ())
}
