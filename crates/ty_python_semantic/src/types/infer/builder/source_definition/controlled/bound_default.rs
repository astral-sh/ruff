use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::typevar::default::{
    BoundDefaultEffects, BoundDefaultFacts, bound_typevar_default_recover_with,
    bound_typevar_default_with,
};
use crate::types::typevar::{TypeVarDefaultEvaluation, TypeVarInstance};
use crate::types::{BindingContext, BoundTypeVarInstance, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_bound_typevar_default(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::bound_defaults::observe_default_query(
            self.db(),
            variable,
        );
        bound_typevar_default_with(variable, BoundDefaultFacts, self).await
    }

    pub(in crate::types::infer) async fn recover_bound_typevar_default(
        &self,
        cycle: &salsa::Cycle<'_>,
        previous: &Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        bound_typevar_default_recover_with(
            cycle,
            *previous,
            value,
            variable,
            BoundDefaultFacts,
            self,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BoundDefaultEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        TypeVarBindingEffects::bound_typevar(self, variable).await
    }

    async fn stored_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarDefaultEvaluation<'db>>> {
        self.field(variable.default_request(self.access.endpoint().field_request_context()))
            .await
    }

    async fn definition(&self, variable: TypeVarInstance<'db>) -> RunResult<Definition<'db>> {
        TypeVarBindingEffects::typevar_definition(self, variable)
            .await?
            .ok_or(RunError::Contract(
                "a bound TypeVar with a default must have a source definition",
            ))
    }

    async fn default_type(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        stored: TypeVarDefaultEvaluation<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.environment_program(env).await?;
        self.type_parameter_future(|| self.typevar_default_from_stored(variable, env, stored))
            .await?.await
    }

    async fn binding_context(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BindingContext<'db>> {
        let identity = TypeVarBindingEffects::bound_identity(self, variable).await?;
        self.local(1, 0, || identity.binding_context).await
    }

    async fn bind_default(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        binding: BindingContext<'db>,
    ) -> RunResult<Type<'db>> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::default_binding::observe_binding(
            self.db(),
            default,
            binding,
        );
        self.bind_legacy_typevars(default, env, binding).await
    }

    async fn definition_file(&self, definition: Definition<'db>) -> RunResult<ProgramFile<'db>> {
        SourceEffects::definition_file(self, definition).await
    }

    async fn cycle_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        SourceEffects::cycle_normalize(self, env, default, previous, cycle).await
    }

    async fn recursive_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.normalize_cycle_heads(env, default, cycle).await
    }
}
