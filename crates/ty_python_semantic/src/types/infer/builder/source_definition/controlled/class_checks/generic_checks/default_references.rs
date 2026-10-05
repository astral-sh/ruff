//! Admitted class default scans borrow canonical variables and retain the first invalid reference.

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::ClassCheckEffects;
use super::scan_cost::{cursor_quote, cursor_step_quote, instance_equality_quote};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::infer::builder::post_inference::static_class::generic_checks::default_references::{
    ClassDefaultReferenceEffects, TypeVariableReference, VariableCursor, VariableRange,
    check_class_default_references_with, contains_default_reference_with,
    invalid_default_reference_with, type_variable_reference,
};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_generic_validation::{self as observations, ValidationStage};
use crate::types::local_transfer::boxed_future_with_fixed_transfers_at;
use crate::types::local_transfer::collections::{CALL_1, event_quote};
use crate::types::typevar::TypeVarInstance;
use crate::types::visitor::runtime::{RuntimeTypeSearchWith, RuntimeTypeWalk};
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral, Type};

/// A search predicate that identifies references outside the earlier variables of a class.
/// Keeps those borrowed variables available while the type walker suspends.
#[derive(Debug)]
struct InvalidDefaultReference<'effects, 'variables, 'db, E> {
    effects: &'effects E,
    variables: &'variables ContextVariables<'db>,
    position: usize,
}

impl<'run, 'db: 'run, E: ClassDefaultReferenceEffects<'db, Error = RunError>>
    RuntimeTypeSearchWith<'run, 'db, Option<TypeVarInstance<'db>>>
    for InvalidDefaultReference<'_, '_, 'db, E>
{
    async fn predicate(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<TypeVarInstance<'db>>> {
        let bytes = size_of::<[(Type<'db>, &ContextVariables<'db>, usize, &E); 2]>();
        boxed_future_with_fixed_transfers_at(endpoint, Ok((22, bytes)), || {
            invalid_default_reference_with(ty, self.variables, self.position, self.effects)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Reports the first invalid reference in each class type-parameter default.
    /// References must name earlier variables in the same class; the shared scan uses checked
    /// defaults and the existing ordered type walker.
    pub(super) async fn check_class_default_references(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        let bytes = size_of::<[(StaticClassLiteral<'db>, GenericContext<'db>, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((16, bytes)), || {
                check_class_default_references_with(class, context, self)
            })
            .await?
            .await?;
        #[cfg(test)]
        observations::validation_completed(
            self.builder.context.file(),
            class,
            ValidationStage::DefaultReferences,
            &self.builder.context.retained_diagnostics(),
        );
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassDefaultReferenceEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        let bytes = size_of::<[(GenericContext<'db>, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((10, bytes)), || {
                TypeVarBindingEffects::variables(self.source, context)
            })
            .await?
            .await
    }

    async fn cursor<'a>(
        &self,
        variables: &'a ContextVariables<'db>,
        range: VariableRange,
    ) -> RunResult<VariableCursor<'a, 'db>> {
        let (work, bytes) = const { cursor_quote() }?;
        self.source
            .local_with_fixed_transfers(work, bytes, || VariableCursor::new(variables, range))
            .await
    }

    async fn next_variable(
        &self,
        cursor: &mut VariableCursor<'_, 'db>,
    ) -> RunResult<Option<(usize, BoundTypeVarInstance<'db>)>> {
        let (work, bytes) = const { cursor_step_quote() }?;
        self.source
            .local_with_fixed_transfers(work, bytes, || cursor.next())
            .await
    }

    async fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        let bytes = size_of::<[(BoundTypeVarInstance<'db>, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((10, bytes)), || {
                TypeVarBindingEffects::bound_typevar(self.source, variable)
            })
            .await?
            .await
    }

    async fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let bytes =
            size_of::<[(TypeVarInstance<'db>, &crate::ProgramEnvironment<'db>, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((15, bytes)), || {
                self.source
                    .typevar_default(variable, self.builder.program_environment())
            })
            .await?
            .await
    }

    async fn first_invalid(
        &self,
        default: Type<'db>,
        variables: &ContextVariables<'db>,
        position: usize,
    ) -> RunResult<Option<TypeVarInstance<'db>>> {
        // The nested predicate and walk are initialized here; the local helper separately
        // admits the factory captures, the returned walk and their suspension transfers.
        let (work, bytes) = const {
            match event_quote(
                3 * CALL_1 + 20,
                &[
                    size_of::<InvalidDefaultReference<'_, '_, 'db, Self>>(),
                    size_of::<
                        RuntimeTypeWalk<
                            '_,
                            'run,
                            'db,
                            InvalidDefaultReference<'_, '_, 'db, Self>,
                            &SourceEffects<'_, 'run, 'db, A>,
                        >,
                    >(),
                    size_of::<&dyn crate::Db>(),
                    size_of::<&TaskEndpoint<'run, 'db>>(),
                ],
            ) {
                Some(quote) => Ok(quote),
                None => Err(RunError::Contract(
                    "default-reference walk construction quotation overflow",
                )),
            }
        }?;
        let mut walk = self
            .source
            .local_with_fixed_transfers(work, bytes, || RuntimeTypeWalk {
                db: self.builder.db(),
                endpoint: self.source.access.endpoint(),
                query: InvalidDefaultReference {
                    effects: self,
                    variables,
                    position,
                },
                unavailable: self.source,
            })
            .await?;
        let bytes = size_of::<[(Type<'db>, TypeSearchMode, TypeWalkFacts, &mut ()); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((17, bytes)), || {
                search_type_with(
                    default,
                    TypeSearchMode::SkipLazyAttributes,
                    TypeWalkFacts,
                    &mut walk,
                )
            })
            .await?
            .await
    }

    async fn reference(&self, ty: Type<'db>) -> RunResult<TypeVariableReference<'db>> {
        let bytes = size_of::<[(Type<'db>, TypeVariableReference<'db>); 2]>();
        self.source
            .local_with_fixed_transfers(13, bytes, || type_variable_reference(ty))
            .await
    }

    async fn same_instance(
        &self,
        left: TypeVarInstance<'db>,
        right: TypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let (work, bytes) = const { instance_equality_quote() }?;
        self.source
            .local_with_fixed_transfers(work, bytes, || left == right)
            .await
    }

    async fn contains_instance(
        &self,
        variables: &ContextVariables<'db>,
        range: VariableRange,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let bytes = size_of::<
            [(
                &ContextVariables<'db>,
                VariableRange,
                TypeVarInstance<'db>,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((17, bytes)), || {
                contains_default_reference_with(variables, range, variable, self)
            })
            .await?
            .await
    }

    async fn report(
        &self,
        class: StaticClassLiteral<'db>,
        variable: TypeVarInstance<'db>,
        referenced: TypeVarInstance<'db>,
        is_later_in_list: bool,
    ) -> RunResult<()> {
        let bytes = size_of::<
            [(
                StaticClassLiteral<'db>,
                TypeVarInstance<'db>,
                TypeVarInstance<'db>,
                bool,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((19, bytes)), || {
                self.report_invalid_default_reference(class, variable, referenced, is_later_in_list)
            })
            .await?
            .await
    }
}
