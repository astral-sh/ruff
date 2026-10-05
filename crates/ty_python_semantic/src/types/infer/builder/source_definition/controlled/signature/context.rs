//! Admits signature-context collection and the `merge_signature_contexts_with` rule:
//! a sole `typing.Self` variable can coexist with PEP 695 parameters.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::super::{SourceAccess, SourceEffects};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::generics::signature_context::{SignatureContextEffects, SignatureContextMergeEffects};
use crate::types::legacy_typevars::find_legacy_typevars_with_effects;
use crate::types::signatures::{Parameter, Parameters};
use crate::types::{BoundTypeVarInstance, GenericContext, Type, TypeVarKind};
use crate::{FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SignatureContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn environment(&self, definition: Definition<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.local_with_fixed_transfers(2, 0, || ProgramEnvironment::from_file(file)).await
    }

    async fn new_variables(&self) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        // The empty ordered set has no backing allocation. This admission also pays for the cursor.
        self.local_with_fixed_transfers(12, size_of::<usize>(), FxOrderSet::default).await
    }

    async fn next_annotation(&self, parameters: &Parameters<'db>, cursor: &mut usize) -> RunResult<Option<(usize, Type<'db>)>> {
        self.local_with_fixed_transfers(
            8,
            size_of::<Option<&Parameter<'db>>>() + size_of::<usize>() + size_of::<Type<'db>>(),
            || {
                let parameter = parameters.get(*cursor)?;
                let index = *cursor;
                *cursor += 1;
                Some((index, parameter.annotated_type()))
            },
        ).await
    }

    async fn eager_default(&self, parameters: &Parameters<'db>, index: usize) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(4, size_of::<Option<&Parameter<'db>>>(), || {
            parameters.get(index).and_then(Parameter::eager_default_type)
        }).await
    }

    async fn collect(&self, env: &ProgramEnvironment<'db>, definition: Definition<'db>, ty: Type<'db>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>) -> RunResult<()> {
        self.type_parameter_future(|| {
            find_legacy_typevars_with_effects(self.db(), env, ty, Some(definition), variables, self)
        }).await?.await
    }

    async fn finish(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> RunResult<Option<GenericContext<'db>>> {
        // Prepay either final Option construction here; the set remains owned across the child.
        let empty = self.local_with_fixed_transfers(4, 2 * size_of::<Option<GenericContext<'db>>>(), || variables.is_empty()).await?;
        if empty {
            Ok(None)
        } else {
            let context = self.type_parameter_future(|| self.context_from_legacy_variables(env, variables)).await?.await?;
            Ok(Some(context))
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SignatureContextMergeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn prepare(&self) -> RunResult<()> {
        self.local_with_fixed_transfers(6, 4 * size_of::<Option<GenericContext<'db>>>(), || ()).await
    }

    async fn is_only_self(&self, context: GenericContext<'db>) -> RunResult<bool> {
        let variables = TypeVarBindingEffects::variables(self, context).await?;
        let only = self.local_with_fixed_transfers(6, size_of::<usize>() + size_of::<bool>(), || {
            if variables.len() == 1 {
                GenericContext::variable_at_in(variables, 0)
            } else {
                None
            }
        }).await?;
        let Some(variable) = only else {
            return Ok(false);
        };
        let typevar = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        let kind = TypeVarBindingEffects::kind(self, typevar).await?;
        self.local_with_fixed_transfers(1, 0, || matches!(kind, TypeVarKind::TypingSelf)).await
    }

    async fn merge(&self, legacy: GenericContext<'db>, pep695: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        self.type_parameter_future(|| self.merge_return_context(legacy, pep695)).await?.await
    }

    async fn publish(&self, context: Option<GenericContext<'db>>) -> RunResult<Option<GenericContext<'db>>> {
        self.local_with_fixed_transfers(1, 0, || context).await
    }
}
