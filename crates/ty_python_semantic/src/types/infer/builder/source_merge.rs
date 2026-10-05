//! Canonical-result merging shared by ordinary and controlled source inference.

use std::convert::Infallible;

use super::{
    Definition, DefinitionInference, DefinitionInferenceExtra, ExpressionInference,
    InferenceRegion, TypeInferenceBuilder,
};
use crate::types::{Type, UnionType};
use crate::{Db, ProgramEnvironment};

#[cfg(feature = "experimental-analysis")]
use super::source_definition::controlled::{SourceAccess, SourceEffects, SourceOperation};
#[cfg(feature = "experimental-analysis")]
use salsa::execution_probe::{RunError, RunResult};

pub(super) mod constraints;
pub(super) use constraints::merge_collection_constraints;

#[cfg(test)]
mod tests;

pub(in crate::types::infer) trait ExpressionMergeEffects<'db> {
    type Error;

    async fn prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
    ) -> Result<(), Self::Error>;

    async fn union(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        existing: Type<'db>,
        incoming: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) -> Result<(), Self::Error>;
}

pub(super) struct LegacyExpressionMergeEffects;

impl<'db> ExpressionMergeEffects<'db> for LegacyExpressionMergeEffects {
    type Error = Infallible;

    async fn prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
    ) -> Result<(), Self::Error> {
        builder.extend_expression_prefix(inference);
        Ok(())
    }

    async fn union(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        existing: Type<'db>,
        incoming: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(db, env, existing, incoming))
    }

    async fn suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) -> Result<(), Self::Error> {
        builder.extend_expression_suffix(inference, resolved_cycle, include_bindings);
        Ok(())
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) async fn extend_expression_with<E: ExpressionMergeEffects<'db>>(
        &mut self,
        inference: &ExpressionInference<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);
        self.extend_expression_unchecked_with(inference, true, effects)
            .await
    }

    pub(super) async fn extend_expression_unchecked_with<E: ExpressionMergeEffects<'db>>(
        &mut self,
        inference: &ExpressionInference<'db>,
        include_bindings: bool,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.prefix(self, inference).await?;
        let resolved_cycle = if let Some(incoming) = inference.fallback_type() {
            Some(match self.cycle_recovery {
                Some(existing) => {
                    effects
                        .union(self.db(), self.program_environment(), existing, incoming)
                        .await?
                }
                None => incoming,
            })
        } else {
            None
        };
        effects
            .suffix(self, inference, resolved_cycle, include_bindings)
            .await
    }

    pub(super) fn extend_expression_prefix(&mut self, inference: &ExpressionInference<'db>) {
        self.extend_expression_types(inference.expressions.iter().copied());
        if let Some(extra) = &inference.extra {
            self.implicit_aliases
                .extend(extra.implicit_aliases.iter().copied());
            self.comparison_truthiness
                .extend(extra.comparison_truthiness.iter().copied());
            self.context.extend(&extra.diagnostics);
        }
    }

    pub(super) fn extend_expression_suffix(
        &mut self,
        inference: &ExpressionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) {
        if let Some(cycle) = resolved_cycle {
            self.cycle_recovery = Some(cycle);
        }
        if let Some(extra) = &inference.extra {
            self.called_functions
                .extend(extra.called_functions.iter().copied());
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
            self.expected_types
                .extend(extra.expected_types.iter().copied());
            self.type_expression_flags
                .extend(extra.type_expression_flags.iter().copied());
            merge_collection_constraints(
                &mut self.collection_use_constraints,
                &extra.collection_use_constraints,
            );
            if include_bindings && !matches!(self.region, InferenceRegion::Scope(..)) {
                self.bindings.extend(extra.bindings.iter().copied());
            }
        }
    }
}

#[cfg(feature = "experimental-analysis")]
impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExpressionMergeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
    ) -> RunResult<()> {
        self.merge_expression_prefix(builder, inference).await
    }

    async fn union(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _existing: Type<'db>,
        _incoming: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) -> RunResult<()> {
        self.merge_expression_suffix(builder, inference, resolved_cycle, include_bindings)
            .await
    }
}

pub(in crate::types::infer) trait DefinitionMergeEffects<'db>:
    ExpressionMergeEffects<'db>
{
    async fn definition_prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> Result<(), Self::Error>;

    async fn definition_suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &DefinitionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
    ) -> Result<(), Self::Error>;
}

impl<'db> DefinitionMergeEffects<'db> for LegacyExpressionMergeEffects {
    async fn definition_prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> Result<(), Self::Error> {
        builder.extend_definition_prefix(definition, inference);
        Ok(())
    }

    async fn definition_suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &DefinitionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.extend_definition_suffix(inference, resolved_cycle);
        Ok(())
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) async fn extend_definition_with<E: DefinitionMergeEffects<'db>>(
        &mut self,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects
            .definition_prefix(self, definition, inference)
            .await?;
        let resolved_cycle = if let Some(incoming) = inference.fallback_type() {
            Some(match self.cycle_recovery {
                Some(existing) => {
                    effects
                        .union(self.db(), self.program_environment(), existing, incoming)
                        .await?
                }
                None => incoming,
            })
        } else {
            None
        };
        effects
            .definition_suffix(self, inference, resolved_cycle)
            .await
    }

    pub(super) fn extend_definition_prefix(
        &mut self,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) {
        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);

        self.extend_expression_types(inference.expressions.iter().copied());
        self.declarations.extend(inference.declarations(definition));

        if !matches!(self.region, InferenceRegion::Scope(..)) {
            self.bindings.extend(inference.bindings(definition));
        }

        if let Some(extra) = &inference.extra {
            match extra.as_ref() {
                DefinitionInferenceExtra::Qualifiers(qualifiers) => {
                    self.qualifiers.extend(qualifiers.iter().copied());
                }
                DefinitionInferenceExtra::Deferred(deferred) => {
                    self.deferred.extend(deferred.iter().copied());
                }
                DefinitionInferenceExtra::Diagnostics(diagnostics) => {
                    self.context.extend(diagnostics);
                }
                DefinitionInferenceExtra::DeferredAndUndecorated(extra) => {
                    self.deferred.extend(extra.deferred.iter().copied());
                }
                DefinitionInferenceExtra::CalledFunctions(called_functions) => {
                    self.called_functions
                        .extend(called_functions.iter().copied());
                }
                DefinitionInferenceExtra::ExpectedTypes(expected_types) => {
                    self.expected_types.extend(expected_types.iter().copied());
                }
                DefinitionInferenceExtra::StringAnnotations(string_annotations) => {
                    self.string_annotations
                        .extend(string_annotations.iter().copied());
                }
                DefinitionInferenceExtra::Undecorated(_)
                | DefinitionInferenceExtra::DiscardsDictKeyAssignments => {}
                DefinitionInferenceExtra::Other(extra) => {
                    self.implicit_aliases
                        .extend(extra.implicit_aliases.iter().copied());
                    self.comparison_truthiness
                        .extend(extra.comparison_truthiness.iter().copied());
                    self.called_functions
                        .extend(extra.called_functions.iter().copied());
                }
            }
        }
    }

    pub(super) fn extend_definition_suffix(
        &mut self,
        inference: &DefinitionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
    ) {
        if let Some(cycle) = resolved_cycle {
            self.cycle_recovery = Some(cycle);
        }
        if let Some(DefinitionInferenceExtra::Other(extra)) = inference.extra.as_deref() {
            self.context.extend(&extra.diagnostics);
            self.deferred.extend(extra.deferred.iter().copied());
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
            self.expected_types
                .extend(extra.expected_types.iter().copied());
            self.qualifiers.extend(extra.qualifiers.iter().copied());
            self.type_expression_flags
                .extend(extra.type_expression_flags.iter().copied());

            #[expect(
                clippy::iter_over_hash_type,
                reason = "constraints for distinct collection definitions are merged \
                    independently"
            )]
            for (collection_def, constraints) in &extra.collection_use_constraints {
                self.collection_use_constraints
                    .entry(*collection_def)
                    .and_modify(|this| this.extend(constraints))
                    .or_insert(constraints.clone());
            }
        }
    }
}

#[cfg(feature = "experimental-analysis")]
impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DefinitionMergeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn definition_prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        self.merge_definition_prefix(builder, definition, inference)
            .await
    }

    async fn definition_suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &DefinitionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
    ) -> RunResult<()> {
        self.merge_definition_suffix(builder, inference, resolved_cycle)
            .await
    }
}
