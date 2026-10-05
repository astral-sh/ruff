//! Admission for copying complete canonical payloads into an unpublished builder.

mod table_cost;

pub(in crate::types::infer::builder) use table_cost::table_merge_preparation_quote;

use std::alloc::Layout;

use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::suppression::FileSuppressionId;
use crate::types::function::FunctionType;
use crate::types::infer::builder::source_merge::constraints::{
    constraints_finalization_quote, constraints_merge_quote, constraints_metadata_quote,
};
use crate::types::infer::builder::{DeclaredAndInferredType, TypeInferenceBuilder};
use crate::types::infer::{
    DefinitionInference, DefinitionInferenceExtra, ExpressionInference, ExpressionInferenceExtra,
    FunctionDecoratorInference, InferenceRegion, ScopeInferenceExtra, TypeAndRange,
    TypeExpressionFlags,
};
pub(in crate::types::infer::builder) use crate::types::storage_quote::{
    StorageQuote, sequence_merge,
};
use crate::types::{
    Truthiness, Type, TypeAndQualifiers, TypeCheckDiagnostics, TypeOrigin, TypeQualifiers,
};

fn checked(quote: Option<StorageQuote>) -> RunResult<StorageQuote> {
    quote.ok_or(RunError::Contract("source storage quotation overflow"))
}

/// Quotes a definition entry with a Copy value, including debug uniqueness and retained storage.
pub(super) fn definition_map_insert_quote<V: Copy>(len: usize, capacity: usize) -> Option<StorageQuote> {
    type Entry<'db, V> = (Definition<'db>, V);
    let mut quote = sequence_merge::<Entry<'_, V>>(len, capacity, 1)?;
    let entry_bytes = size_of::<Entry<'_, V>>();
    // VecMap checks key uniqueness in debug builds. Growth also prepays both the old
    // backing's retirement and disposal of the replacement if a later child refuses.
    quote.work = quote.work.checked_add(len)?.checked_add(4)?;
    if quote.bytes != 0 {
        Layout::array::<Entry<'_, V>>(quote.bytes.checked_div(entry_bytes)?).ok()?;
        let replacement_capacity = quote.bytes.checked_div(entry_bytes)?;
        quote.bytes = quote.bytes.checked_add(len.checked_mul(entry_bytes)?)?;
        quote.work = quote
            .work
            .checked_add(capacity)?
            .checked_add(replacement_capacity.checked_mul(2)?)?;
    }
    // These are logical entry transfers, including an append into reused capacity.
    quote.bytes = quote.bytes.checked_add(entry_bytes.checked_mul(2)?)?;
    Some(quote)
}

pub(in crate::types::infer::builder) fn slots(capacity: usize) -> Option<usize> {
    if capacity == 0 {
        Some(0)
    } else {
        capacity.checked_add(1)?.checked_mul(4)?.checked_add(32)
    }
}

/// A table with removals supplies its retained backing separately. The other callers own
/// insert-only destination tables. Quotations cover structural work, not allocator latency.
pub(in crate::types::infer::builder) fn table_merge<T>(
    len: usize,
    capacity: usize,
    incoming: usize,
    retained_slots: usize,
) -> Option<(StorageQuote, usize)> {
    let old_slots = slots(capacity)?.max(retained_slots);
    let required = len.checked_add(incoming)?;
    let mut quote = StorageQuote {
        work: old_slots
            .checked_add(incoming.checked_mul(4)?)?
            .checked_add(4)?,
        bytes: 0,
    };
    if required <= capacity {
        return Some((quote, old_slots));
    }
    let capacity_bound = capacity.checked_mul(2)?.max(required).max(4);
    let new_slots = slots(capacity_bound.checked_mul(2)?)?.max(old_slots.checked_mul(2)?);
    quote.work = quote.work.checked_add(new_slots)?.checked_add(len)?;
    quote.bytes = new_slots.checked_mul(size_of::<T>().checked_add(1)?)?;
    Some((quote, new_slots))
}

pub(in crate::types::infer::builder) fn ordered_merge<T>(
    len: usize,
    capacity: usize,
    incoming: usize,
) -> Option<StorageQuote> {
    // IndexSet stores cached hashes beside its ordered values and owns a separate index table.
    table_merge::<usize>(len, capacity, incoming, 0)?
        .0
        .checked_add(sequence_merge::<(usize, T)>(len, capacity, incoming)?)
}

pub(in crate::types::infer::builder) fn dense_finish<T>(
    len: usize,
    capacity: usize,
) -> Option<StorageQuote> {
    Some(StorageQuote {
        work: capacity.checked_add(len.checked_mul(8)?)?.checked_add(4)?,
        bytes: if len == 0 {
            0
        } else {
            len.checked_mul(5)?
                .checked_add(16)?
                .checked_mul(size_of::<T>())?
        },
    })
}

fn frozen_finish<T>(len: usize, backing: usize) -> Option<StorageQuote> {
    let logarithm = if len < 2 { 0 } else { len.ilog2() as usize };
    // FrozenMap/Set collect, sort fixed-size node keys, check uniqueness in debug and box.
    let sorting = len.checked_mul(logarithm.checked_mul(23)?.checked_add(43)?)?;
    let mut quote = dense_finish::<T>(len, backing)?;
    quote.work = quote.work.checked_add(sorting)?.checked_add(backing)?;
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn merge_decorator_inference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &FunctionDecoratorInference<'db>,
    ) -> RunResult<()> {
        self.work(16).await?;
        let expressions = inference.expression_types().len();
        let bindings = inference.bindings().len();
        let called = inference.called_functions().len();
        let aliases = inference.implicit_aliases().len();
        let diagnostics = inference.diagnostics();
        let (diagnostic_len, diagnostic_capacity, used_len, used_capacity) =
            builder.context.retained_diagnostics().storage();
        let (incoming_diagnostics, _, incoming_used, incoming_used_capacity) =
            diagnostics.storage();
        let mut quote = StorageQuote::default();
        for addition in [
            table_merge::<(ExpressionNodeKey, Type<'db>)>(
                builder.expressions.len(),
                builder.expressions.capacity(),
                expressions,
                0,
            )
            .map(|(quote, _)| quote),
            sequence_merge::<(Definition<'db>, Type<'db>)>(
                builder.bindings.0.len(),
                builder.bindings.0.capacity(),
                bindings,
            ),
            ordered_merge::<FunctionType<'db>>(
                builder.called_functions.len(),
                builder.called_functions.capacity(),
                called,
            ),
            ordered_merge::<Definition<'db>>(
                builder.implicit_aliases.len(),
                builder.implicit_aliases.capacity(),
                aliases,
            ),
            sequence_merge::<Diagnostic>(diagnostic_len, diagnostic_capacity, incoming_diagnostics),
            table_merge::<FileSuppressionId>(used_len, used_capacity, incoming_used, 0)
                .map(|(quote, _)| quote),
        ] {
            quote = checked(quote.checked_add(checked(addition)?))?;
        }
        // VecMap checks each incoming binding against earlier entries in debug builds.
        let binding_checks = builder
            .bindings
            .0
            .len()
            .checked_add(bindings)
            .and_then(|length| length.checked_mul(bindings));
        let retirement = self.diagnostic_retirement(diagnostics).await?;
        quote.work = Self::checked(
            binding_checks
                .and_then(|work| quote.work.checked_add(work))
                .and_then(|work| work.checked_add(slots(incoming_used_capacity)?))
                .and_then(|work| work.checked_add(retirement)),
        )?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::function_decorator_ingestion::merge_start(
            self.db(),
            builder,
            inference,
        );
        self.local(quote.work, quote.bytes, || {
            builder.expressions.reserve(expressions);
            builder.bindings.0.reserve_exact(bindings);
            builder.called_functions.reserve_exact(called);
            builder.implicit_aliases.reserve_exact(aliases);
            builder.extend_function_decorator_inference(inference);
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::function_decorator_ingestion::merged(
                self.db(),
                builder,
            );
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn diagnostic_retirement(
        &self,
        diagnostics: &TypeCheckDiagnostics,
    ) -> RunResult<usize> {
        self.work(Self::checked(diagnostics.storage().0.checked_add(1))?)
            .await?;
        let mut work = 1usize;
        for diagnostic in diagnostics {
            let subs = diagnostic.sub_diagnostics();
            self.work(Self::checked(subs.len().checked_add(1))?).await?;
            // Messages and source-file handles have passive destruction. Subdiagnostics have
            // one annotation vector and cannot recursively contain other subdiagnostics.
            work = Self::checked(
                work.checked_add(16)
                    .and_then(|n| n.checked_add(diagnostic.annotations().len().checked_mul(8)?))
                    .and_then(|n| {
                        n.checked_add(
                            diagnostic
                                .fix()
                                .map_or(0, |fix| fix.edits().len())
                                .checked_mul(4)?,
                        )
                    }),
            )?;
            for sub in subs {
                work = Self::checked(
                    work.checked_add(4)
                        .and_then(|n| n.checked_add(sub.annotations().len().checked_mul(8)?)),
                )?;
            }
        }
        Ok(work)
    }

    pub(in crate::types::infer::builder) async fn merge_expression_prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
    ) -> RunResult<()> {
        self.work(1).await?;
        let incoming = inference.expressions.iter().len();
        let (mut quote, _) = table_merge::<(ExpressionNodeKey, Type<'db>)>(
            builder.expressions.len(),
            builder.expressions.capacity(),
            incoming,
            0,
        )
        .ok_or(RunError::Contract("expression merge quotation overflow"))?;
        let mut truthiness_backing =
            Self::checked(slots(builder.comparison_truthiness.capacity()))?
                .max(builder.source_truthiness_backing);
        let previous_backing = truthiness_backing;
        quote.work = Self::checked(
            quote
                .work
                .checked_add(truthiness_backing)
                .and_then(|n| n.checked_add(incoming.checked_mul(2)?)),
        )?;
        if let Some(extra) = &inference.extra {
            let aliases = ordered_merge::<Definition<'db>>(
                builder.implicit_aliases.len(),
                builder.implicit_aliases.capacity(),
                extra.implicit_aliases.len(),
            );
            quote = checked(quote.checked_add(checked(aliases)?))?;
            let (truthiness, backing) = table_merge::<(ExpressionNodeKey, Truthiness)>(
                builder.comparison_truthiness.len(),
                builder.comparison_truthiness.capacity(),
                extra.comparison_truthiness.iter().len(),
                truthiness_backing,
            )
            .ok_or(RunError::Contract("truthiness merge quotation overflow"))?;
            truthiness_backing = backing;
            quote = checked(quote.checked_add(truthiness))?;
            let (length, capacity, used, used_capacity) =
                builder.context.retained_diagnostics().storage();
            let (source_length, _, source_used, source_capacity) = extra.diagnostics.storage();
            let diagnostics = sequence_merge::<Diagnostic>(length, capacity, source_length)
                .and_then(|quote| {
                    quote.checked_add(
                        table_merge::<FileSuppressionId>(used, used_capacity, source_used, 0)?.0,
                    )
                });
            quote = checked(quote.checked_add(checked(diagnostics)?))?;
            quote.work = Self::checked(
                quote
                    .work
                    .checked_add(Self::checked(slots(source_capacity))?),
            )?;
            let retirement = self.diagnostic_retirement(&extra.diagnostics).await?;
            quote.work = Self::checked(quote.work.checked_add(retirement))?;
        }
        self.local(quote.work, quote.bytes, || {
            builder.source_truthiness_backing = truthiness_backing;
            builder.expressions.reserve(incoming);
            if let Some(extra) = &inference.extra {
                builder
                    .implicit_aliases
                    .reserve_exact(extra.implicit_aliases.len());
                builder
                    .comparison_truthiness
                    .reserve(extra.comparison_truthiness.iter().len());
            }
            // Retain observed backing, not the worst-case growth quote: repeated replacement
            // can rehash in place and must not exponentially inflate this history.
            builder.source_truthiness_backing = slots(builder.comparison_truthiness.capacity())
                .map_or(truthiness_backing, |observed| {
                    previous_backing.max(observed)
                });
            builder.extend_expression_prefix(inference);
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn merge_expression_suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
        cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) -> RunResult<()> {
        self.work(1).await?;
        let mut quote = StorageQuote { work: 4, bytes: 0 };
        if let Some(extra) = &inference.extra {
            for addition in [
                ordered_merge::<FunctionType<'db>>(
                    builder.called_functions.len(),
                    builder.called_functions.capacity(),
                    extra.called_functions.len(),
                ),
                table_merge::<ExpressionNodeKey>(
                    builder.string_annotations.len(),
                    builder.string_annotations.capacity(),
                    extra.string_annotations.iter().len(),
                    0,
                )
                .map(|(quote, _)| quote),
                table_merge::<(ExpressionNodeKey, Type<'db>)>(
                    builder.expected_types.len(),
                    builder.expected_types.capacity(),
                    extra.expected_types.iter().len(),
                    0,
                )
                .map(|(quote, _)| quote),
                table_merge::<(ExpressionNodeKey, TypeExpressionFlags)>(
                    builder.type_expression_flags.len(),
                    builder.type_expression_flags.capacity(),
                    extra.type_expression_flags.iter().len(),
                    0,
                )
                .map(|(quote, _)| quote),
            ] {
                quote = checked(quote.checked_add(checked(addition)?))?;
            }
            let metadata = checked(
                constraints_metadata_quote(&builder.collection_use_constraints).and_then(|quote| {
                    quote.checked_add(constraints_metadata_quote(
                        &extra.collection_use_constraints,
                    )?)
                }),
            )?;
            self.work(metadata.work).await?;
            let entries = builder
                .collection_use_constraints
                .values()
                .chain(extra.collection_use_constraints.values())
                .try_fold(0usize, |count, values| count.checked_add(values.len()));
            self.work(Self::checked(entries)?).await?;
            quote = checked(quote.checked_add(checked(constraints_merge_quote(
                &builder.collection_use_constraints,
                &extra.collection_use_constraints,
            ))?))?;
            if include_bindings && !matches!(builder.region, InferenceRegion::Scope(..)) {
                quote = checked(quote.checked_add(checked(sequence_merge::<(
                    Definition<'db>,
                    Type<'db>,
                )>(
                    builder.bindings.0.len(),
                    builder.bindings.0.capacity(),
                    extra.bindings.len(),
                ))?))?;
                // VecMap checks uniqueness against earlier entries in debug builds.
                quote.work = Self::checked(
                    builder
                        .bindings
                        .0
                        .len()
                        .checked_add(extra.bindings.len())
                        .and_then(|n| n.checked_mul(extra.bindings.len()))
                        .and_then(|n| quote.work.checked_add(n)),
                )?;
            }
        }
        self.local(quote.work, quote.bytes, || {
            if let Some(extra) = &inference.extra {
                builder
                    .called_functions
                    .reserve_exact(extra.called_functions.len());
                builder
                    .string_annotations
                    .reserve(extra.string_annotations.iter().len());
                builder
                    .expected_types
                    .reserve(extra.expected_types.iter().len());
                builder
                    .type_expression_flags
                    .reserve(extra.type_expression_flags.iter().len());
                if include_bindings && !matches!(builder.region, InferenceRegion::Scope(..)) {
                    builder.bindings.0.reserve_exact(extra.bindings.len());
                }
            }
            builder.extend_expression_suffix(inference, cycle, include_bindings);
            #[cfg(test)]
            super::observations::observe_merge(
                self.db(),
                builder.context.retained_diagnostics().storage().0,
            );
        })
        .await
    }

    pub(super) async fn expression_finalization_quote(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<StorageQuote> {
        self.work(1).await?;
        let mut quote = StorageQuote {
            work: 16,
            bytes: size_of::<ExpressionInferenceExtra<'db>>(),
        };
        for addition in [
            frozen_finish::<(ExpressionNodeKey, Type<'db>)>(
                builder.expressions.len(),
                Self::checked(slots(builder.expressions.capacity()))?,
            ),
            frozen_finish::<(ExpressionNodeKey, Truthiness)>(
                builder.comparison_truthiness.len(),
                Self::checked(slots(builder.comparison_truthiness.capacity()))?
                    .max(builder.source_truthiness_backing),
            ),
            frozen_finish::<ExpressionNodeKey>(
                builder.string_annotations.len(),
                Self::checked(slots(builder.string_annotations.capacity()))?,
            ),
            frozen_finish::<(ExpressionNodeKey, Type<'db>)>(
                builder.expected_types.len(),
                Self::checked(slots(builder.expected_types.capacity()))?,
            ),
            frozen_finish::<(ExpressionNodeKey, TypeExpressionFlags)>(
                builder.type_expression_flags.len(),
                Self::checked(slots(builder.type_expression_flags.capacity()))?,
            ),
            dense_finish::<Definition<'db>>(
                builder.implicit_aliases.len(),
                builder.implicit_aliases.capacity(),
            ),
            dense_finish::<FunctionType<'db>>(
                builder.called_functions.len(),
                builder.called_functions.capacity(),
            ),
            dense_finish::<(Definition<'db>, Type<'db>)>(
                builder.bindings.0.len(),
                builder.bindings.0.capacity(),
            ),
        ] {
            quote = checked(quote.checked_add(checked(addition)?))?;
        }
        let metadata = checked(constraints_metadata_quote(
            &builder.collection_use_constraints,
        ))?;
        self.work(metadata.work).await?;
        quote = checked(quote.checked_add(checked(constraints_finalization_quote(
            &builder.collection_use_constraints,
        ))?))?;
        let diagnostics = builder.context.retained_diagnostics();
        let (length, capacity, used, used_capacity) = diagnostics.storage();
        quote = checked(quote.checked_add(checked(dense_finish::<Diagnostic>(length, capacity))?))?;
        let suppression_slots = Self::checked(slots(used_capacity))?;
        quote.work = Self::checked(
            quote
                .work
                .checked_add(suppression_slots)
                .and_then(|n| n.checked_add(used.checked_mul(4)?)),
        )?;
        if used != 0 {
            let bytes = Self::checked(
                slots(used).and_then(|slots| slots.checked_mul(size_of::<FileSuppressionId>() + 1)),
            )?;
            quote.bytes = Self::checked(quote.bytes.checked_add(bytes))?;
        }
        quote.work = Self::checked(
            quote
                .work
                .checked_add(self.diagnostic_retirement(&diagnostics).await?),
        )?;
        Ok(quote)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn merge_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        builder
            .extend_definition_with(definition, inference, self)
            .await
    }

    pub(in crate::types::infer::builder) async fn merge_definition_prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        self.work(1).await?;
        let mut qualifiers = 0;
        let mut deferred = 0;
        let mut called = 0;
        let mut expected = 0;
        let mut strings = 0;
        let mut aliases = 0;
        let mut truthiness = 0;
        let mut flags = 0;
        let mut diagnostics = None;
        let mut constraints = None;
        match inference.extra.as_deref() {
            Some(DefinitionInferenceExtra::Qualifiers(values)) => qualifiers = values.iter().len(),
            Some(DefinitionInferenceExtra::Deferred(values)) => deferred = values.len(),
            Some(DefinitionInferenceExtra::Diagnostics(values)) => {
                diagnostics = Some(values.as_ref())
            }
            Some(DefinitionInferenceExtra::DeferredAndUndecorated(values)) => {
                deferred = values.deferred.len()
            }
            Some(DefinitionInferenceExtra::CalledFunctions(values)) => called = values.len(),
            Some(DefinitionInferenceExtra::ExpectedTypes(values)) => expected = values.iter().len(),
            Some(DefinitionInferenceExtra::StringAnnotations(values)) => {
                strings = values.iter().len()
            }
            Some(DefinitionInferenceExtra::Other(values)) => {
                qualifiers = values.qualifiers.iter().len();
                deferred = values.deferred.len();
                called = values.called_functions.len();
                expected = values.expected_types.iter().len();
                strings = values.string_annotations.iter().len();
                aliases = values.implicit_aliases.len();
                truthiness = values.comparison_truthiness.iter().len();
                flags = values.type_expression_flags.iter().len();
                diagnostics = Some(&values.diagnostics);
                constraints = Some(&values.collection_use_constraints);
            }
            Some(
                DefinitionInferenceExtra::Undecorated(_)
                | DefinitionInferenceExtra::DiscardsDictKeyAssignments,
            )
            | None => {}
        }

        let incoming_expressions = inference.expressions.iter().len();
        let incoming_declarations = inference.declarations(definition).len();
        let incoming_bindings = if matches!(builder.region, InferenceRegion::Scope(..)) {
            0
        } else {
            inference.bindings(definition).len()
        };
        let old_truthiness_backing =
            Self::checked(slots(builder.comparison_truthiness.capacity()))?
                .max(builder.source_truthiness_backing);
        let (truthiness_quote, truthiness_backing) =
            table_merge::<(ExpressionNodeKey, Truthiness)>(
                builder.comparison_truthiness.len(),
                builder.comparison_truthiness.capacity(),
                truthiness,
                old_truthiness_backing,
            )
            .ok_or(RunError::Contract(
                "definition truthiness quotation overflow",
            ))?;
        let mut quote = truthiness_quote;
        for addition in [
            table_merge::<(ExpressionNodeKey, Type<'db>)>(
                builder.expressions.len(),
                builder.expressions.capacity(),
                incoming_expressions,
                0,
            )
            .map(|(quote, _)| quote),
            sequence_merge::<(Definition<'db>, TypeAndQualifiers<'db>)>(
                builder.declarations.0.len(),
                builder.declarations.0.capacity(),
                incoming_declarations,
            ),
            sequence_merge::<(Definition<'db>, Type<'db>)>(
                builder.bindings.0.len(),
                builder.bindings.0.capacity(),
                incoming_bindings,
            ),
            sequence_merge::<Definition<'db>>(
                builder.deferred.0.len(),
                builder.deferred.0.capacity(),
                deferred,
            ),
            ordered_merge::<Definition<'db>>(
                builder.implicit_aliases.len(),
                builder.implicit_aliases.capacity(),
                aliases,
            ),
            ordered_merge::<FunctionType<'db>>(
                builder.called_functions.len(),
                builder.called_functions.capacity(),
                called,
            ),
            table_merge::<(ExpressionNodeKey, TypeQualifiers)>(
                builder.qualifiers.len(),
                builder.qualifiers.capacity(),
                qualifiers,
                0,
            )
            .map(|(quote, _)| quote),
            table_merge::<(ExpressionNodeKey, Type<'db>)>(
                builder.expected_types.len(),
                builder.expected_types.capacity(),
                expected,
                0,
            )
            .map(|(quote, _)| quote),
            table_merge::<ExpressionNodeKey>(
                builder.string_annotations.len(),
                builder.string_annotations.capacity(),
                strings,
                0,
            )
            .map(|(quote, _)| quote),
            table_merge::<(ExpressionNodeKey, TypeExpressionFlags)>(
                builder.type_expression_flags.len(),
                builder.type_expression_flags.capacity(),
                flags,
                0,
            )
            .map(|(quote, _)| quote),
        ] {
            quote = checked(quote.checked_add(checked(addition)?))?;
        }
        // VecMap and VecSet verify uniqueness against earlier entries in debug builds.
        for (length, incoming) in [
            (builder.declarations.0.len(), incoming_declarations),
            (builder.bindings.0.len(), incoming_bindings),
            (builder.deferred.0.len(), deferred),
        ] {
            quote.work = Self::checked(
                length
                    .checked_add(incoming)
                    .and_then(|n| n.checked_mul(incoming))
                    .and_then(|n| quote.work.checked_add(n)),
            )?;
        }
        quote.work = Self::checked(
            quote
                .work
                .checked_add(old_truthiness_backing)
                .and_then(|n| n.checked_add(incoming_expressions.checked_mul(2)?)),
        )?;
        if let Some(diagnostics) = diagnostics {
            let (length, capacity, used, used_capacity) =
                builder.context.retained_diagnostics().storage();
            let (incoming, _, incoming_used, incoming_capacity) = diagnostics.storage();
            quote = checked(quote.checked_add(checked(
                sequence_merge::<Diagnostic>(length, capacity, incoming).and_then(|quote| {
                    quote.checked_add(
                        table_merge::<FileSuppressionId>(used, used_capacity, incoming_used, 0)?.0,
                    )
                }),
            )?))?;
            let retirement = self.diagnostic_retirement(diagnostics).await?;
            quote.work = Self::checked(
                quote
                    .work
                    .checked_add(retirement)
                    .and_then(|n| n.checked_add(slots(incoming_capacity)?)),
            )?;
        }
        if let Some(constraints) = constraints {
            let metadata = checked(
                constraints_metadata_quote(&builder.collection_use_constraints)
                    .and_then(|quote| quote.checked_add(constraints_metadata_quote(constraints)?)),
            )?;
            self.work(metadata.work).await?;
            let entries = builder
                .collection_use_constraints
                .values()
                .chain(constraints.values())
                .try_fold(0usize, |count, values| count.checked_add(values.len()));
            self.work(Self::checked(entries)?).await?;
            quote = checked(quote.checked_add(checked(constraints_merge_quote(
                &builder.collection_use_constraints,
                constraints,
            ))?))?;
        }

        // Admit both halves before mutating the owner. A union can suspend between them, so the
        // suffix consumes already admitted storage and still checks completion before mutation.
        self.local(quote.work, quote.bytes, || {
            builder.expressions.reserve(incoming_expressions);
            builder.declarations.0.reserve_exact(incoming_declarations);
            builder.bindings.0.reserve_exact(incoming_bindings);
            builder.deferred.0.reserve_exact(deferred);
            builder.implicit_aliases.reserve_exact(aliases);
            builder.called_functions.reserve_exact(called);
            builder.qualifiers.reserve(qualifiers);
            builder.expected_types.reserve(expected);
            builder.string_annotations.reserve(strings);
            builder.type_expression_flags.reserve(flags);
            builder.comparison_truthiness.reserve(truthiness);
            builder.source_truthiness_backing = slots(builder.comparison_truthiness.capacity())
                .map_or(truthiness_backing, |observed| {
                    old_truthiness_backing.max(observed)
                });
            builder.extend_definition_prefix(definition, inference);
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn merge_definition_suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &DefinitionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
    ) -> RunResult<()> {
        self.local(1, 0, || {
            builder.extend_definition_suffix(inference, resolved_cycle)
        })
        .await
    }

    pub(super) async fn scope_finalization_quote(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<StorageQuote> {
        if builder.expression_cache.is_some() {
            return self.unavailable(SourceOperation::Finalization).await;
        }
        let mut quote = self.expression_finalization_quote(builder).await?;
        if let Some(cache) = builder.reachability_cache.get() {
            let (primary_len, primary_capacity, other_len, other_capacity) =
                cache.retained_storage();
            let work = Self::checked(
                primary_len
                    .checked_add(primary_capacity)
                    .and_then(|work| work.checked_add(other_len))
                    .and_then(|work| work.checked_add(slots(other_capacity)?))
                    .and_then(|work| work.checked_add(8)),
            )?;
            quote.work = Self::checked(quote.work.checked_add(work))?;
        }
        quote.bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<ScopeInferenceExtra<'db>>()),
        )?;
        for addition in [
            frozen_finish::<(ExpressionNodeKey, TypeQualifiers)>(
                builder.qualifiers.len(),
                Self::checked(slots(builder.qualifiers.capacity()))?,
            ),
            dense_finish::<(Definition<'db>, TypeAndQualifiers<'db>)>(
                builder.declarations.0.len(),
                builder.declarations.0.capacity(),
            ),
            dense_finish::<Definition<'db>>(
                builder.deferred.0.len(),
                builder.deferred.0.capacity(),
            ),
            dense_finish::<TypeAndRange<'db>>(
                builder.return_types_and_ranges.len(),
                builder.return_types_and_ranges.capacity(),
            ),
            dense_finish::<(ExpressionNodeKey, Type<'db>)>(
                builder.deferred_decorator_calls.len(),
                builder.deferred_decorator_calls.capacity(),
            ),
            dense_finish::<Type<'db>>(
                builder.dataclass_field_specifiers.len(),
                builder.dataclass_field_specifiers.capacity(),
            ),
        ] {
            quote = checked(quote.checked_add(checked(addition)?))?;
        }
        Ok(quote)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn record_source_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        let length = builder.deferred.0.len();
        let capacity = builder.deferred.0.capacity();
        let bytes = if length == capacity {
            Self::checked(
                length
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(size_of::<Definition<'db>>())),
            )?
        } else {
            0
        };
        self.local(
            Self::checked(capacity.checked_add(length).and_then(|n| n.checked_add(4)))?,
            bytes,
            || {
                builder.deferred.0.reserve_exact(1);
                builder.deferred.insert(definition);
            },
        )
        .await
    }

    pub(super) async fn bind_source_declaration(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: ruff_python_ast::AnyNodeRef<'_>,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let declared = self
            .initialize_value(|| TypeAndQualifiers::new(ty, TypeOrigin::Inferred, TypeQualifiers::empty()))
            .await?;
        self.bind_source_qualified_declaration(builder, node, definition, declared)
            .await
    }

    pub(super) async fn bind_source_qualified_declaration(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: ruff_python_ast::AnyNodeRef<'_>,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        let types = self
            .initialize_value(|| DeclaredAndInferredType::AreTheSame(declared))
            .await?;
        self.add_source_declaration_with_binding(builder, node, definition, &types)
            .await
    }

    /// Admits both selected entries before inserting the declaration and then the binding.
    pub(super) async fn store_source_declaration_and_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        inferred: Type<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local(
                10,
                size_of::<(usize, usize, usize, usize)>()
                    + size_of::<StorageQuote>() * 6
                    + size_of::<Option<StorageQuote>>() * 5,
                || {
                    definition_map_insert_quote::<TypeAndQualifiers<'db>>(
                        builder.declarations.0.len(),
                        builder.declarations.0.capacity(),
                    )?
                    .checked_add(definition_map_insert_quote::<Type<'db>>(
                        builder.bindings.0.len(),
                        builder.bindings.0.capacity(),
                    )?)
                },
            )
            .await?
            .ok_or(RunError::Contract("declaration and binding storage quotation overflow"))?;
        #[cfg(test)]
        super::declaration::observations::before(definition, quote);
        self.local(quote.work, quote.bytes, || {
            builder.declarations.0.reserve_exact(1);
            builder.bindings.0.reserve_exact(1);
            builder.declarations.insert(definition, declared);
            builder.bindings.insert(definition, inferred);
            #[cfg(test)]
            {
                super::observations::observe(self.db(), super::observations::Event::DefinitionStored);
                super::declaration::observations::after(self.db(), definition);
            }
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn store_annotation_qualifiers(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        annotation: &ast::Expr,
        qualifiers: TypeQualifiers,
    ) -> RunResult<()> {
        self.work(1).await?;
        if qualifiers.is_empty() {
            return Ok(());
        }
        let (quote, _) = table_merge::<(ExpressionNodeKey, TypeQualifiers)>(
            builder.qualifiers.len(),
            builder.qualifiers.capacity(),
            1,
            0,
        )
        .ok_or(RunError::Contract(
            "annotation qualifier storage quotation overflow",
        ))?;
        let bytes = Self::checked(quote.bytes.checked_add(size_of::<(ExpressionNodeKey, TypeQualifiers)>()))?;
        self.local(quote.work, bytes, || {
            builder.qualifiers.reserve(1);
            builder.store_qualifiers(annotation, qualifiers);
        })
        .await
    }
}
