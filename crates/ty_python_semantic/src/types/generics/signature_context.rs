//! Collects function-owned legacy variables and combines them with a method's PEP 695 context.

use std::convert::Infallible;

use ty_python_core::definition::Definition;

use crate::types::signatures::{Parameter, Parameters};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::{ApplySpecialization, BoundTypeVarInstance, GenericContext, Type, TypeContext, TypeMapping};
use crate::{Db, FxOrderSet, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies ordered signature annotations and the definition-filtered `find_legacy_typevars` traversal.
    /// Defaults are inspected only after their parameter annotation has been collected.
    #[synchronous(SynchronousSignatureContextEffects)]
    pub(in crate::types) trait SignatureContextEffects<'db> {
        type Error;

        #[operation(source)]
        async fn environment(&self, definition: Definition<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(local)]
        async fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_annotation(&self, parameters: &Parameters<'db>, cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn eager_default(&self, parameters: &Parameters<'db>, index: usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn collect(&self, env: &ProgramEnvironment<'db>, definition: Definition<'db>, ty: Type<'db>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<Option<GenericContext<'db>>, Self::Error>;
    }

    /// Combines a sole `typing.Self` variable with PEP 695 parameters; other mixtures keep PEP 695.
    #[synchronous(SynchronousSignatureContextMergeEffects)]
    pub(in crate::types) trait SignatureContextMergeEffects<'db> {
        type Error;

        #[operation(local)]
        async fn prepare(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn is_only_self(&self, context: GenericContext<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn merge(&self, legacy: GenericContext<'db>, pep695: GenericContext<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(local)]
        async fn publish(&self, context: Option<GenericContext<'db>>) -> Result<Option<GenericContext<'db>>, Self::Error>;
    }

    /// Builds a generic context from legacy type variables in a function signature.
    /// Visits each parameter annotation and eager default, then the return annotation. Only variables
    /// owned by `definition` participate; an empty collection has no generic context.
    #[synchronous(signature_context_sync)]
    #[capabilities(effects = SignatureContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn signature_context_with<'db, E: SignatureContextEffects<'db>>(
        definition: Definition<'db>,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        let env = effects.environment(definition).await?;
        let mut variables = effects.new_variables().await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(parameter) = effects.next_annotation(parameters, &mut cursor).await? {
            let (index, annotation) = parameter;
            effects.collect(&env, definition, annotation, &mut variables).await?;
            if let Some(default) = effects.eager_default(parameters, index).await? {
                effects.collect(&env, definition, default, &mut variables).await?;
            }
        }
        effects.collect(&env, definition, return_type, &mut variables).await?;
        effects.finish(&env, variables).await
    }

    /// Prepends a sole `typing.Self` variable to a method's PEP 695 context; other mixes keep PEP 695.
    /// `typing.Self` describes the receiver type rather than a separately declared legacy parameter,
    /// so it may occur alongside either parameter syntax.
    #[synchronous(merge_signature_contexts_sync)]
    #[capabilities(effects = SignatureContextMergeEffects)]
    #[passive_values()]
    pub(in crate::types) async fn merge_signature_contexts_with<'db, E: SignatureContextMergeEffects<'db>>(
        pep695: Option<GenericContext<'db>>,
        legacy: Option<GenericContext<'db>>,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        effects.prepare().await?;
        let context = match (legacy, pep695) {
            (Some(legacy), Some(pep695)) => {
                if effects.is_only_self(legacy).await? {
                    Some(effects.merge(legacy, pep695).await?)
                } else {
                    // The parameter and return annotations still contain the legacy variables.
                    // Post-inference `check_pep695_function_legacy_typevars` recollects them
                    // from the raw signature, including eager defaults, to report invalid mixes.
                    Some(pep695)
                }
            }
            (Some(context), None) | (None, Some(context)) => Some(context),
            (None, None) => None,
        };
        effects.publish(context).await
    }
}

/// Uses ordinary source access and canonical context construction for shared signature decisions.
pub(super) struct OrdinarySignatureContextEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousSignatureContextEffects<'db> for OrdinarySignatureContextEffects<'db> {
    type Error = Infallible;

    fn environment(&self, definition: Definition<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_definition(definition))
    }

    fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(FxOrderSet::default())
    }

    fn next_annotation(&self, parameters: &Parameters<'db>, cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Infallible> {
        let Some(parameter) = parameters.get(*cursor) else {
            return Ok(None);
        };
        let index = *cursor;
        *cursor += 1;
        Ok(Some((index, parameter.annotated_type())))
    }

    fn eager_default(&self, parameters: &Parameters<'db>, index: usize) -> Result<Option<Type<'db>>, Infallible> {
        Ok(parameters.get(index).and_then(Parameter::eager_default_type))
    }

    fn collect(&self, env: &ProgramEnvironment<'db>, definition: Definition<'db>, ty: Type<'db>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<(), Infallible> {
        ty.find_legacy_typevars(self.db, env, Some(definition), variables);
        Ok(())
    }

    fn finish(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(if variables.is_empty() {
            None
        } else {
            Some(GenericContext::from_typevar_instances(self.db, env, variables))
        })
    }
}

impl<'db> SynchronousSignatureContextMergeEffects<'db> for OrdinarySignatureContextEffects<'db> {
    type Error = Infallible;

    fn prepare(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn is_only_self(&self, context: GenericContext<'db>) -> Result<bool, Infallible> {
        let mut variables = context.variables(self.db);
        let first = variables.next();
        Ok(first.is_some_and(|variable| variables.next().is_none() && variable.typevar(self.db).is_self(self.db)))
    }

    fn merge(&self, legacy: GenericContext<'db>, pep695: GenericContext<'db>) -> Result<GenericContext<'db>, Infallible> {
        Ok(legacy.merge(self.db, pep695))
    }

    fn publish(&self, context: Option<GenericContext<'db>>) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(context)
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Filters declarations through raw substitutions, then optionally maps each retained occurrence.
    #[synchronous(SynchronousContextSpecializationEffects)]
    pub(in crate::types) trait ContextSpecializationEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn next(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn lookup(&self, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn same_identity(&self, mapped: BoundTypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn map_retained(&self, env: &ProgramEnvironment<'db>, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    }

    /// Returns the next unspecialized declaration, retaining only unchanged identities after raw lookup.
    /// When mapping retained Self bounds or constraints, each retained declaration gets its own new
    /// mapping visitor. This cursor is consumed by the existing context constructor so program
    /// resolution still precedes its mapping children.
    #[synchronous(next_specialized_context_variable_sync)]
    #[capabilities(effects = ContextSpecializationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn next_specialized_context_variable_with<'db, E: ContextSpecializationEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
        specialization: &ApplySpecialization<'_, 'db>,
        specialize_self_domain: bool,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        #[cursor_loop]
        while let Some(variable) = effects.next(variables, cursor).await? {
            let keep = match effects.lookup(specialization, variable).await? {
                None => true,
                Some(Type::TypeVar(mapped)) => effects.same_identity(mapped, variable).await?,
                Some(_) => false,
            };
            if keep {
                if specialize_self_domain {
                    if let Some(mapped) = effects.map_retained(env, specialization, variable).await? {
                        return Ok(Some(mapped));
                    }
                } else {
                    return Ok(Some(variable));
                }
            }
        }
        Ok(None)
    }
}

/// Returns a generic context retaining declarations with no raw replacement or the same bound
/// identity, in their original order. Optionally specializes retained Self upper bounds or ordered
/// constraints, as requested by `specialization`. Returns an interned empty context when none survive.
///
/// Filters and maps declarations lazily while the ordinary context constructor consumes them,
/// so the constructor resolves its program before mapping children.
pub(in crate::types) fn specialize_signature_context<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    context: GenericContext<'db>,
    specialization: ApplySpecialization<'_, 'db>,
) -> GenericContext<'db> {
    let variables = context.variables_inner(db);
    let mut cursor = 0;
    let specialize_self_domain = specialization.specialize_self_domain();
    let effects = OrdinarySignatureContextEffects { db };
    let variables = std::iter::from_fn(|| {
        match next_specialized_context_variable_sync(env, variables, &mut cursor, &specialization, specialize_self_domain, &effects) {
            Ok(variable) => variable,
            Err(never) => match never {},
        }
    });
    GenericContext::from_typevar_instances(db, env, variables)
}

impl<'db> SynchronousContextSpecializationEffects<'db> for OrdinarySignatureContextEffects<'db> {
    type Error = Infallible;

    fn next(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let variable = GenericContext::variable_at_in(variables, *cursor);
        if variable.is_some() {
            *cursor += 1;
        }
        Ok(variable)
    }

    fn lookup(&self, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(specialization.get(self.db, variable))
    }

    fn same_identity(&self, mapped: BoundTypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(mapped.identity(self.db) == original.identity(self.db))
    }

    fn map_retained(&self, env: &ProgramEnvironment<'db>, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(Type::TypeVar(variable).apply_type_mapping(self.db, env, &TypeMapping::ApplySpecialization(*specialization), TypeContext::default()).as_typevar())
    }
}
