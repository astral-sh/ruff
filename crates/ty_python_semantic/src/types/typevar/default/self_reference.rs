use std::cell::RefCell;
use std::convert::Infallible;

use smallvec::SmallVec;

use crate::types::cyclic::TypeIdentity;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::typevar::{TypeVarDefaultVisitor, TypeVarIdentity, TypeVarInstance};
use crate::types::visitor::SmallSet;
use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, RecursiveType, Specialization, Type,
    TypeAliasType, any_over_type,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

pub(in crate::types) struct SelfReferenceState<'db> {
    pub(in crate::types) seen_typevars: RefCell<SmallSet<TypeVarInstance<'db>, 8>>,
    pub(in crate::types) seen_types: RefCell<SmallVec<[TypeIdentity<'db>; 1]>>,
}

impl<'db> SelfReferenceState<'db> {
    pub(in crate::types) fn new() -> Self {
        Self {
            seen_typevars: RefCell::new(SmallSet::default()),
            seen_types: RefCell::new(SmallVec::new()),
        }
    }
}

pub(in crate::types) struct SelfReferenceFacts;

pub(in crate::types::typevar) struct OrdinarySelfReferenceEffects<'env, 'visitor, 'db> {
    pub(in crate::types::typevar) db: &'db dyn Db,
    pub(in crate::types::typevar) env: &'env ProgramEnvironment<'db>,
    pub(in crate::types::typevar) visitor: &'visitor TypeVarDefaultVisitor<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSelfReferenceEffects)]
    pub(in crate::types) trait SelfReferenceEffects<'db> {
        type Error;
        type State;

        #[operation(local)]
        async fn new_state(&self) -> Result<Self::State, Self::Error>;
        #[operation(source)]
        async fn identity(&self, variable: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn bound_typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn remember_variable(&self, state: &Self::State, variable: TypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn checked_default(&self, variable: TypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn search(&self, state: &Self::State, ty: Type<'db>, target: TypeVarIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn variable_reference(&self, state: &Self::State, variable: TypeVarInstance<'db>, target: TypeVarIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias_reference(&self, state: &Self::State, alias: TypeAliasType<'db>, target: TypeVarIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn recursive_reference(&self, state: &Self::State, recursive: RecursiveType<'db>, target: TypeVarIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn alias_specialization(&self, alias: TypeAliasType<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
        #[operation(source)]
        async fn specialization_types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, types: &[Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn alias_generic_context(&self, alias: TypeAliasType<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(source)]
        async fn generic_variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn alias_identity(&self, alias: TypeAliasType<'db>) -> Result<TypeIdentity<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_identity(&self, recursive: RecursiveType<'db>) -> Result<TypeIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn remember_type(&self, state: &Self::State, identity: TypeIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn alias_raw_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn recursive_arguments(&self, recursive: RecursiveType<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
        #[operation(child)]
        async fn recursive_unfold(&self, recursive: RecursiveType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl SelfReferenceFacts {
        fn same_identity(&self, left: TypeVarIdentity<'_>, right: TypeVarIdentity<'_>) -> bool {
            left == right
        }
    }

    #[synchronous(type_is_self_referential_sync)]
    #[capabilities(effects = SelfReferenceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn type_is_self_referential_with<'db, E: SelfReferenceEffects<'db>>(
        variable: TypeVarInstance<'db>, ty: Type<'db>, effects: &E,
    ) -> Result<bool, E::Error> {
        let state = effects.new_state().await?;
        let target = effects.identity(variable).await?;
        effects.search(&state, ty, target).await
    }

    #[synchronous(variable_is_self_referential_sync)]
    #[capabilities(effects = SelfReferenceEffects, facts = SelfReferenceFacts)]
    #[passive_values()]
    pub(in crate::types) async fn variable_is_self_referential_with<'db, E: SelfReferenceEffects<'db>>(
        variable: TypeVarInstance<'db>, target: TypeVarIdentity<'db>, state: &E::State, facts: SelfReferenceFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        if facts.same_identity(effects.identity(variable).await?, target) {
            return Ok(true);
        }
        if !effects.remember_variable(state, variable).await? {
            return Ok(false);
        }
        match effects.checked_default(variable).await? {
            Some(default_ty) => effects.search(state, default_ty, target).await,
            None => Ok(false),
        }
    }

    #[synchronous(alias_is_self_referential_sync)]
    #[capabilities(effects = SelfReferenceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn alias_is_self_referential_with<'db, E: SelfReferenceEffects<'db>>(
        alias: TypeAliasType<'db>, target: TypeVarIdentity<'db>, state: &E::State, effects: &E,
    ) -> Result<bool, E::Error> {
        let specialization = effects.alias_specialization(alias).await?;
        // A nested specialization can contain self even when its alias body was already visited.
        if let Some(specialization) = specialization {
            let types = effects.specialization_types(specialization).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(ty) = effects.next_type(types, &mut cursor).await? {
                if effects.search(state, ty, target).await? {
                    return Ok(true);
                }
            }
        } else if let Some(context) = effects.alias_generic_context(alias).await? {
            let variables = effects.generic_variables(context).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(variable) = effects.next_variable(variables, &mut cursor).await? {
                let variable = effects.bound_typevar(variable).await?;
                if effects.variable_reference(state, variable, target).await? {
                    return Ok(true);
                }
            }
        }
        // The shared recursive identity also stops specializations that keep growing.
        let identity = effects.alias_identity(alias).await?;
        if !effects.remember_type(state, identity).await? {
            return Ok(false);
        }
        let value_type = match specialization {
            Some(_) => effects.alias_value(alias).await?,
            None => effects.alias_raw_value(alias).await?,
        };
        effects.search(state, value_type, target).await
    }

    #[synchronous(recursive_is_self_referential_sync)]
    #[capabilities(effects = SelfReferenceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn recursive_is_self_referential_with<'db, E: SelfReferenceEffects<'db>>(
        recursive: RecursiveType<'db>, target: TypeVarIdentity<'db>, state: &E::State, effects: &E,
    ) -> Result<bool, E::Error> {
        if let Some(arguments) = effects.recursive_arguments(recursive).await? {
            let types = effects.specialization_types(arguments).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(ty) = effects.next_type(types, &mut cursor).await? {
                if effects.search(state, ty, target).await? {
                    return Ok(true);
                }
            }
        }
        let identity = effects.recursive_identity(recursive).await?;
        if !effects.remember_type(state, identity).await? {
            return Ok(false);
        }
        let unfolded = effects.recursive_unfold(recursive).await?;
        effects.search(state, unfolded, target).await
    }

    #[synchronous(self_reference_predicate_sync)]
    #[capabilities(effects = SelfReferenceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn self_reference_predicate_with<'db, E: SelfReferenceEffects<'db>>(
        ty: Type<'db>, target: TypeVarIdentity<'db>, state: &E::State, effects: &E,
    ) -> Result<bool, E::Error> {
        match ty {
            Type::TypeVar(variable) => {
                let variable = effects.bound_typevar(variable).await?;
                effects.variable_reference(state, variable, target).await
            }
            Type::KnownInstance(KnownInstanceType::TypeVar(variable)) => effects.variable_reference(state, variable, target).await,
            Type::TypeAlias(alias) => effects.alias_reference(state, alias, target).await,
            Type::Recursive(recursive) => effects.recursive_reference(state, recursive, target).await,
            Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => effects.alias_reference(state, alias, target).await,
            _ => Ok(false),
        }
    }
}

impl<'db> SynchronousSelfReferenceEffects<'db> for OrdinarySelfReferenceEffects<'_, '_, 'db> {
    type Error = Infallible;
    type State = SelfReferenceState<'db>;

    fn new_state(&self) -> Result<Self::State, Infallible> {
        Ok(SelfReferenceState::new())
    }

    fn identity(&self, variable: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(variable.identity(self.db))
    }

    fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(variable.typevar(self.db))
    }

    fn remember_variable(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(state.seen_typevars.borrow_mut().insert(variable))
    }

    fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(variable.default_type_impl(self.db, self.env, Some(self.visitor)))
    }

    fn search(
        &self,
        state: &Self::State,
        ty: Type<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Infallible> {
        Ok(any_over_type(self.db, self.env, ty, false, |inner_ty| {
            let Ok(result) = self_reference_predicate_sync(inner_ty, target, state, self);
            result
        }))
    }

    fn variable_reference(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Infallible> {
        variable_is_self_referential_sync(variable, target, state, SelfReferenceFacts, self)
    }

    fn alias_reference(
        &self,
        state: &Self::State,
        alias: TypeAliasType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Infallible> {
        alias_is_self_referential_sync(alias, target, state, self)
    }

    fn recursive_reference(
        &self,
        state: &Self::State,
        recursive: RecursiveType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Infallible> {
        recursive_is_self_referential_sync(recursive, target, state, self)
    }

    fn alias_specialization(
        &self,
        alias: TypeAliasType<'db>,
    ) -> Result<Option<Specialization<'db>>, Infallible> {
        Ok(alias.specialization(self.db))
    }

    fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(specialization.types(self.db))
    }

    fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Infallible> {
        let ty = types.get(*cursor).copied();
        if ty.is_some() {
            *cursor += 1;
        }
        Ok(ty)
    }

    fn alias_generic_context(
        &self,
        alias: TypeAliasType<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(alias.generic_context(self.db))
    }

    fn generic_variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(self.db)))
    }

    fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let variable = GenericContext::variable_at_in(variables, *cursor);
        if variable.is_some() {
            *cursor += 1;
        }
        Ok(variable)
    }

    fn alias_identity(&self, alias: TypeAliasType<'db>) -> Result<TypeIdentity<'db>, Infallible> {
        Ok(Type::TypeAlias(alias).to_type_identity(self.db))
    }

    fn recursive_identity(
        &self,
        recursive: RecursiveType<'db>,
    ) -> Result<TypeIdentity<'db>, Infallible> {
        Ok(Type::Recursive(recursive).to_type_identity(self.db))
    }

    fn remember_type(
        &self,
        state: &Self::State,
        identity: TypeIdentity<'db>,
    ) -> Result<bool, Infallible> {
        let mut seen = state.seen_types.borrow_mut();
        if seen.contains(&identity) {
            return Ok(false);
        }
        seen.push(identity);
        Ok(true)
    }

    fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(alias.value_type(self.db))
    }

    fn alias_raw_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(alias.raw_value_type(self.db))
    }

    fn recursive_arguments(
        &self,
        recursive: RecursiveType<'db>,
    ) -> Result<Option<Specialization<'db>>, Infallible> {
        Ok(recursive.arguments(self.db))
    }

    fn recursive_unfold(&self, recursive: RecursiveType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(recursive.unfold(self.db, self.env).into_type())
    }
}
