//! Move function type variables used only by returned callables into those callables' contexts.

use std::collections::hash_map;
use std::convert::Infallible;

use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::definition::Definition;

use super::return_locations::{OrdinaryLocationEffects, TypeVarLocations, collect_locations_sync};
use crate::types::mapping::return_callables::{
    ReturnCallableReplacements, ReturnTypevarReplacements,
};
use crate::types::signatures::{CallableSignature, Parameters};
use crate::types::{
    ApplySpecialization, ApplyTypeMappingVisitor, BoundTypeVarInstance, CallableType,
    GenericContext, Type, TypeContext, TypeMapping,
};
use crate::{Db, FxIndexMap, FxOrderSet, ProgramEnvironment};

/// Retains the occurrence sets and completed replacements until return-type mapping finishes.
#[derive(Debug)]
pub(in crate::types) struct ReturnScopeState<'db> {
    pub(in crate::types) outside: FxHashSet<BoundTypeVarInstance<'db>>,
    pub(in crate::types) callables:
        hash_map::IntoIter<CallableType<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
    pub(in crate::types) moved: FxHashSet<BoundTypeVarInstance<'db>>,
    pub(in crate::types) replacements: FxHashMap<CallableType<'db>, CallableType<'db>>,
}

/// Iterates the variables of one outermost returned callable in encounter order.
#[derive(Debug)]
pub(in crate::types) struct CallableVariables<'db> {
    pub(in crate::types) callable: CallableType<'db>,
    pub(in crate::types) variables: ordermap::set::IntoIter<BoundTypeVarInstance<'db>>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousReturnScopeEffects)]
    /// Supplies occurrence collection, variable renaming, and the two independent return mappings.
    pub(in crate::types) trait ReturnScopeEffects<'db> {
        type Error;
        type Renamings;
        type Replacements;

        #[operation(child)]
        /// Collects all parameter and return occurrences before any variable is renamed.
        async fn locations(&self, parameters: &Parameters<'db>, return_type: Type<'db>) -> Result<TypeVarLocations<'db>, Self::Error>;
        #[operation(local)]
        /// Takes ownership of occurrence sets and initializes the unpublished replacement maps.
        async fn state(&self, locations: TypeVarLocations<'db>) -> Result<ReturnScopeState<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_callable(&self, state: &mut ReturnScopeState<'db>) -> Result<Option<CallableVariables<'db>>, Self::Error>;
        #[operation(local)]
        /// Reserves enough candidate slots for every variable in the current callable.
        async fn candidates(&self, callable: &CallableVariables<'db>) -> Result<Vec<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, callable: &mut CallableVariables<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        /// Checks whether an occurrence prevents this variable from moving into a returned callable.
        async fn outside(&self, state: &ReturnScopeState<'db>, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn is_bound_by(&self, variable: BoundTypeVarInstance<'db>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        /// Appends one eligible original variable in the callable's encounter order.
        async fn keep(&self, candidates: &mut Vec<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn is_empty(&self, candidates: &[BoundTypeVarInstance<'db>]) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_candidate(&self, candidates: &[BoundTypeVarInstance<'db>], cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        /// Records an original identity for removal from the function's outer context.
        async fn moved(&self, state: &mut ReturnScopeState<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        /// Reserves the insertion-ordered original-to-renamed map for one callable.
        async fn renamings(&self, candidates: &[BoundTypeVarInstance<'db>]) -> Result<FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        /// Appends the return suffix without changing binding or raw bound/default metadata.
        async fn rename(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn insert_renaming(&self, renamings: &mut FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>, original: BoundTypeVarInstance<'db>, renamed: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        /// Retains the completed variable map until all mapping children have drained.
        async fn retain_renamings(&self, renamings: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>) -> Result<Self::Renamings, Self::Error>;
        #[operation(local)]
        async fn callable(&self, variables: &CallableVariables<'db>) -> Result<CallableType<'db>, Self::Error>;
        #[operation(child)]
        /// Maps one callable's signatures with a fresh visitor before adding renamed declarations.
        async fn map_signatures(&self, callable: CallableType<'db>, renamings: &Self::Renamings) -> Result<CallableSignature<'db>, Self::Error>;
        #[operation(child)]
        /// Constructs a context from renamed values in their map's insertion order.
        async fn renamed_context(&self, renamings: &Self::Renamings) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(child)]
        /// Adds the renamed declarations after each mapped overload's existing declarations.
        async fn inherit_context(&self, signatures: &CallableSignature<'db>, context: GenericContext<'db>) -> Result<CallableSignature<'db>, Self::Error>;
        #[operation(child)]
        /// Interns completed signatures while preserving the callable's kind and deprecation metadata.
        async fn with_signatures(&self, callable: CallableType<'db>, signatures: CallableSignature<'db>) -> Result<CallableType<'db>, Self::Error>;
        #[operation(local)]
        async fn insert_replacement(&self, state: &mut ReturnScopeState<'db>, callable: CallableType<'db>, replacement: CallableType<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        /// Retains all completed callable replacements before mapping the original return type.
        async fn retain_replacements(&self, state: &mut ReturnScopeState<'db>) -> Result<Self::Replacements, Self::Error>;
        #[operation(child)]
        /// Applies callable-handle replacements with a fresh visitor, including for an empty map.
        async fn map_return(&self, return_type: Type<'db>, replacements: &Self::Replacements) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        /// Removes moved originals, returning no outer context when no variables remain.
        async fn trim_context(&self, context: GenericContext<'db>, state: &ReturnScopeState<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
    }

    /// Moves only function-owned callable-only variables, then removes their originals from the function context.
    #[synchronous(rescope_return_callables_sync)]
    #[capabilities(effects = ReturnScopeEffects)]
    #[passive_values()]
    pub(in crate::types) async fn rescope_return_callables_with<'db, E: ReturnScopeEffects<'db>>(
        context: GenericContext<'db>, parameters: &Parameters<'db>, return_type: Type<'db>, definition: Definition<'db>, effects: &E,
    ) -> Result<(Option<GenericContext<'db>>, Type<'db>), E::Error> {
        let locations = effects.locations(parameters, return_type).await?;
        let mut state = effects.state(locations).await?;
        #[cursor_loop]
        while let Some(callable_variables) = effects.next_callable(&mut state).await? {
            let mut callable_variables = callable_variables;
            let mut candidates = effects.candidates(&callable_variables).await?;
            #[cursor_loop]
            while let Some(variable) = effects.next_variable(&mut callable_variables).await? {
                if !effects.outside(&state, variable).await?
                    && effects.is_bound_by(variable, definition).await? {
                    effects.keep(&mut candidates, variable).await?;
                }
            }
            if effects.is_empty(&candidates).await? { continue; }
            // Save original identities before constructing any renamed variable. These originals
            // determine which variables are removed from the function's generic context.
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(variable) = effects.next_candidate(&candidates, &mut cursor).await? {
                effects.moved(&mut state, variable).await?;
            }
            let mut renamings = effects.renamings(&candidates).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(original) = effects.next_candidate(&candidates, &mut cursor).await? {
                let renamed = effects.rename(original).await?;
                effects.insert_renaming(&mut renamings, original, renamed).await?;
            }
            let renamings = effects.retain_renamings(renamings).await?;
            let callable = effects.callable(&callable_variables).await?;
            let signatures = effects.map_signatures(callable, &renamings).await?;
            let inherited = effects.renamed_context(&renamings).await?;
            let signatures = effects.inherit_context(&signatures, inherited).await?;
            let replacement = effects.with_signatures(callable, signatures).await?;
            effects.insert_replacement(&mut state, callable, replacement).await?;
        }
        let replacements = effects.retain_replacements(&mut state).await?;
        let return_type = effects.map_return(return_type, &replacements).await?;
        let context = effects.trim_context(context, &state).await?;
        Ok((context, return_type))
    }
}

/// Runs return-callable scoping through ordinary inference and mapping operations.
pub(in crate::types) struct OrdinaryReturnScope<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousReturnScopeEffects<'db> for OrdinaryReturnScope<'_, 'db> {
    type Error = Infallible;
    type Renamings = FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>;
    type Replacements = FxHashMap<CallableType<'db>, CallableType<'db>>;

    fn locations(
        &self,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
    ) -> Result<TypeVarLocations<'db>, Infallible> {
        collect_locations_sync(
            parameters,
            return_type,
            &OrdinaryLocationEffects {
                db: self.db,
                env: self.env,
            },
        )
    }
    fn state(&self, locations: TypeVarLocations<'db>) -> Result<ReturnScopeState<'db>, Infallible> {
        Ok(ReturnScopeState {
            outside: locations.found_outside_callable_return,
            callables: locations.found_inside_callable_return.into_iter(),
            moved: FxHashSet::default(),
            replacements: FxHashMap::default(),
        })
    }
    fn next_callable(
        &self,
        state: &mut ReturnScopeState<'db>,
    ) -> Result<Option<CallableVariables<'db>>, Infallible> {
        Ok(state
            .callables
            .next()
            .map(|(callable, variables)| CallableVariables {
                callable,
                variables: variables.into_iter(),
            }))
    }
    fn candidates(
        &self,
        callable: &CallableVariables<'db>,
    ) -> Result<Vec<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(Vec::with_capacity(callable.variables.len()))
    }
    fn next_variable(
        &self,
        callable: &mut CallableVariables<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(callable.variables.next())
    }
    fn outside(
        &self,
        state: &ReturnScopeState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(state.outside.contains(&variable))
    }
    fn is_bound_by(
        &self,
        variable: BoundTypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(variable.binding_context(self.db).definition() == Some(definition))
    }
    fn keep(
        &self,
        candidates: &mut Vec<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        candidates.push(variable);
        Ok(())
    }
    fn is_empty(&self, candidates: &[BoundTypeVarInstance<'db>]) -> Result<bool, Infallible> {
        Ok(candidates.is_empty())
    }
    fn next_candidate(
        &self,
        candidates: &[BoundTypeVarInstance<'db>],
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let value = candidates.get(*cursor).copied();
        if value.is_some() {
            *cursor += 1;
        }
        Ok(value)
    }
    fn moved(
        &self,
        state: &mut ReturnScopeState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        state.moved.insert(variable);
        Ok(())
    }
    fn renamings(
        &self,
        candidates: &[BoundTypeVarInstance<'db>],
    ) -> Result<Self::Renamings, Infallible> {
        Ok(FxIndexMap::with_capacity_and_hasher(
            candidates.len(),
            Default::default(),
        ))
    }
    fn rename(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(variable.with_name_suffix(self.db, "return"))
    }
    fn insert_renaming(
        &self,
        renamings: &mut Self::Renamings,
        original: BoundTypeVarInstance<'db>,
        renamed: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        renamings.insert(original, renamed);
        Ok(())
    }
    fn retain_renamings(&self, renamings: Self::Renamings) -> Result<Self::Renamings, Infallible> {
        Ok(renamings)
    }
    fn callable(
        &self,
        variables: &CallableVariables<'db>,
    ) -> Result<CallableType<'db>, Infallible> {
        Ok(variables.callable)
    }
    fn map_signatures(
        &self,
        callable: CallableType<'db>,
        renamings: &Self::Renamings,
    ) -> Result<CallableSignature<'db>, Infallible> {
        Ok(callable.signatures(self.db).apply_type_mapping_impl(
            self.db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(
                ReturnTypevarReplacements::Borrowed(renamings),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(self.env),
        ))
    }
    fn renamed_context(
        &self,
        renamings: &Self::Renamings,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(GenericContext::from_typevar_instances(
            self.db,
            self.env,
            renamings.values().copied(),
        ))
    }
    fn inherit_context(
        &self,
        signatures: &CallableSignature<'db>,
        context: GenericContext<'db>,
    ) -> Result<CallableSignature<'db>, Infallible> {
        Ok(signatures.with_inherited_generic_context(self.db, context))
    }
    fn with_signatures(
        &self,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> Result<CallableType<'db>, Infallible> {
        Ok(callable.with_signatures(self.db, signatures))
    }
    fn insert_replacement(
        &self,
        state: &mut ReturnScopeState<'db>,
        callable: CallableType<'db>,
        replacement: CallableType<'db>,
    ) -> Result<(), Infallible> {
        state.replacements.insert(callable, replacement);
        Ok(())
    }
    fn retain_replacements(
        &self,
        state: &mut ReturnScopeState<'db>,
    ) -> Result<Self::Replacements, Infallible> {
        Ok(std::mem::take(&mut state.replacements))
    }
    fn map_return(
        &self,
        return_type: Type<'db>,
        replacements: &Self::Replacements,
    ) -> Result<Type<'db>, Infallible> {
        Ok(return_type.apply_type_mapping(
            self.db,
            self.env,
            &TypeMapping::RescopeReturnCallables(ReturnCallableReplacements::Borrowed(
                replacements,
            )),
            TypeContext::default(),
        ))
    }
    fn trim_context(
        &self,
        context: GenericContext<'db>,
        state: &ReturnScopeState<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        let mut kept = context
            .variables(self.db)
            .filter(|variable| !state.moved.contains(variable))
            .peekable();
        Ok(if kept.peek().is_none() {
            None
        } else {
            Some(GenericContext::from_typevar_instances(
                self.db, self.env, kept,
            ))
        })
    }
}
