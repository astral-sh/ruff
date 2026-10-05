//! Work quotations for the native equality of retained inference results.
//!
//! Both operands can come from ordinary inference. These scans inspect their stored fields,
//! without resolving interned handles or depending on metadata from controlled production.
//! Scanning is admitted separately from the returned quotation for canonical equality.

use ruff_db::diagnostic::{Annotation, Diagnostic};
use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::frozen::FrozenMap;

use super::{
    CollectionUseConstraints, DefinitionInference, DefinitionInferenceExtra, DefinitionTypes,
    ExpressionInference, FunctionDecoratorInference, ScopeInference,
};
use crate::types::{Type, TypeCheckDiagnostics};

pub(super) async fn quote_scope_comparison<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    left: &ScopeInference<'db>,
    right: &ScopeInference<'db>,
) -> RunResult<usize> {
    #[cfg(test)]
    let observed = observations::started(left, right);
    let mut quote = ComparisonQuote::new(&endpoint);
    quote.scope(left).await?;
    quote.scope(right).await?;
    #[cfg(test)]
    observations::finished(observed, quote.work);
    Ok(quote.work)
}

pub(super) async fn quote_definition_comparison<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    left: &DefinitionInference<'db>,
    right: &DefinitionInference<'db>,
) -> RunResult<usize> {
    #[cfg(test)]
    let observed = observations::started(left, right);
    let mut quote = ComparisonQuote::new(&endpoint);
    quote.definition(left).await?;
    quote.definition(right).await?;
    #[cfg(test)]
    observations::finished(observed, quote.work);
    Ok(quote.work)
}

pub(super) async fn quote_expression_comparison<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    left: &ExpressionInference<'db>,
    right: &ExpressionInference<'db>,
) -> RunResult<usize> {
    #[cfg(test)]
    let observed = observations::started(left, right);
    let mut quote = ComparisonQuote::new(&endpoint);
    quote.expression(left).await?;
    quote.expression(right).await?;
    #[cfg(test)]
    observations::finished(observed, quote.work);
    Ok(quote.work)
}

pub(super) async fn quote_function_decorator_comparison<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    left: &FunctionDecoratorInference<'db>,
    right: &FunctionDecoratorInference<'db>,
) -> RunResult<usize> {
    #[cfg(test)]
    let observed = observations::started(left, right);
    let mut quote = ComparisonQuote::new(&endpoint);
    quote.function_decorators(left).await?;
    quote.function_decorators(right).await?;
    #[cfg(test)]
    observations::finished(observed, quote.work);
    Ok(quote.work)
}

struct ComparisonQuote<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    work: usize,
}

impl<'a, 'run, 'db: 'run> ComparisonQuote<'a, 'run, 'db> {
    fn new(endpoint: &'a TaskEndpoint<'run, 'db>) -> Self {
        Self { endpoint, work: 0 }
    }

    async fn scan(&self, work: usize) -> RunResult<()> {
        // Acceptance happens before the caller touches the admitted entries. Only borrowed
        // cursors cross this boundary; all quotation futures have inline, fixed-size state.
        self.endpoint
            .local_call(|| self.endpoint.admit_work(work))
            .await;
        self.endpoint.checkpoint()?.await
    }

    fn add(&mut self, work: usize) -> RunResult<()> {
        self.work = checked(self.work.checked_add(work))?;
        Ok(())
    }

    fn dense(&mut self, len: usize) -> RunResult<()> {
        self.add(checked(len.checked_add(1))?)
    }

    fn ty(&mut self, ty: Type<'db>) -> RunResult<()> {
        self.add(checked(ty.inline_payload_bytes().checked_add(1))?)
    }

    fn optional_type(&mut self, ty: Option<Type<'db>>) -> RunResult<()> {
        self.add(1)?;
        if let Some(ty) = ty {
            self.ty(ty)?;
        }
        Ok(())
    }

    async fn types(
        &mut self,
        mut types: impl ExactSizeIterator<Item = Type<'db>>,
    ) -> RunResult<()> {
        while types.len() != 0 {
            let chunk = types.len().min(64);
            self.scan(chunk).await?;
            for ty in types.by_ref().take(chunk) {
                self.ty(ty)?;
            }
        }
        Ok(())
    }

    async fn type_map(&mut self, types: &FrozenMap<ExpressionNodeKey, Type<'db>>) -> RunResult<()> {
        self.dense(types.iter().len())?;
        self.types(types.values().copied()).await
    }

    async fn scope(&mut self, inference: &ScopeInference<'db>) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        // FrozenValueMap compares its key/index entries and its separate deduplicated values.
        // Every retained value has an entry, so visiting values through all entries bounds both.
        self.dense(inference.expressions.iter().len())?;
        self.types(inference.expressions.iter().map(|(_, ty)| ty))
            .await?;
        if let Some(extra) = inference.extra.as_deref() {
            self.scan(1).await?;
            self.add(1)?;
            self.dense(extra.implicit_aliases.len())?;
            self.dense(extra.string_annotations.iter().len())?;
            self.dense(extra.qualifiers.iter().len())?;
            self.type_map(&extra.expected_types).await?;
            self.dense(extra.type_expression_flags.iter().len())?;
            self.constraints(&extra.collection_use_constraints).await?;
            self.optional_type(extra.cycle_recovery)?;
            self.diagnostics(&extra.diagnostics).await?;
        }
        Ok(())
    }

    async fn definition(&mut self, inference: &DefinitionInference<'db>) -> RunResult<()> {
        self.scan(1).await?;
        // Includes the optional debug scope handle and the types/extra discriminants.
        self.add(1)?;
        self.type_map(&inference.expressions).await?;
        match &inference.types {
            DefinitionTypes::Empty => {}
            DefinitionTypes::Binding(ty) => self.ty(*ty)?,
            DefinitionTypes::Declaration(ty) | DefinitionTypes::BindingAndDeclaration(ty) => {
                // TypeAndQualifiers also has fixed-size origin, qualifiers and provenance.
                self.add(1)?;
                self.ty(ty.inner_type())?;
            }
            DefinitionTypes::Other(types) => {
                self.scan(1).await?;
                self.dense(types.bindings.len())?;
                self.types(types.bindings.iter().map(|(_, ty)| *ty)).await?;
                self.dense(types.declarations.len())?;
                self.types(types.declarations.iter().map(|(_, ty)| ty.inner_type()))
                    .await?;
            }
        }
        if let Some(extra) = inference.extra.as_deref() {
            self.scan(1).await?;
            self.add(1)?;
            match extra {
                DefinitionInferenceExtra::Qualifiers(qualifiers) => {
                    self.dense(qualifiers.iter().len())?;
                }
                DefinitionInferenceExtra::Deferred(definitions) => {
                    self.dense(definitions.len())?;
                }
                DefinitionInferenceExtra::Diagnostics(diagnostics) => {
                    self.diagnostics(diagnostics).await?;
                }
                DefinitionInferenceExtra::Undecorated(ty) => self.ty(**ty)?,
                DefinitionInferenceExtra::DeferredAndUndecorated(extra) => {
                    self.dense(extra.deferred.len())?;
                    self.ty(extra.undecorated_type)?;
                }
                DefinitionInferenceExtra::CalledFunctions(functions) => {
                    self.dense(functions.len())?;
                }
                DefinitionInferenceExtra::ExpectedTypes(types) => self.type_map(types).await?,
                DefinitionInferenceExtra::StringAnnotations(annotations) => {
                    self.dense(annotations.iter().len())?;
                }
                DefinitionInferenceExtra::DiscardsDictKeyAssignments => {}
                DefinitionInferenceExtra::Other(extra) => {
                    self.scan(1).await?;
                    self.add(1)?;
                    self.dense(extra.implicit_aliases.len())?;
                    self.dense(extra.comparison_truthiness.iter().len())?;
                    self.dense(extra.string_annotations.iter().len())?;
                    self.type_map(&extra.expected_types).await?;
                    self.dense(extra.called_functions.len())?;
                    self.dense(extra.type_expression_flags.iter().len())?;
                    self.constraints(&extra.collection_use_constraints).await?;
                    self.optional_type(extra.cycle_recovery)?;
                    self.dense(extra.deferred.len())?;
                    self.diagnostics(&extra.diagnostics).await?;
                    self.optional_type(extra.undecorated_type)?;
                    self.type_map(&extra.deferred_decorator_calls).await?;
                    self.dense(extra.qualifiers.iter().len())?;
                }
            }
        }
        Ok(())
    }

    async fn expression(&mut self, inference: &ExpressionInference<'db>) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        self.type_map(&inference.expressions).await?;
        if let Some(extra) = inference.extra.as_deref() {
            self.scan(1).await?;
            self.add(1)?;
            self.dense(extra.implicit_aliases.len())?;
            self.dense(extra.string_annotations.iter().len())?;
            self.type_map(&extra.expected_types).await?;
            self.dense(extra.type_expression_flags.iter().len())?;
            self.dense(extra.comparison_truthiness.iter().len())?;
            self.constraints(&extra.collection_use_constraints).await?;
            self.dense(extra.bindings.len())?;
            self.types(extra.bindings.iter().map(|(_, ty)| *ty)).await?;
            self.diagnostics(&extra.diagnostics).await?;
            self.dense(extra.called_functions.len())?;
            self.optional_type(extra.cycle_recovery)?;
        }
        Ok(())
    }

    async fn function_decorators(
        &mut self,
        inference: &FunctionDecoratorInference<'db>,
    ) -> RunResult<()> {
        self.scan(7).await?;
        // The known-decorator flags and unknown-decorator bit have fixed-size equality.
        self.add(2)?;
        self.type_map(&inference.expression_types).await?;
        self.dense(inference.bindings.len())?;
        self.types(inference.bindings.iter().map(|(_, ty)| *ty))
            .await?;
        self.dense(inference.called_functions.len())?;
        self.dense(inference.implicit_aliases.len())?;
        self.diagnostics(&inference.diagnostics).await
    }

    async fn constraints(&mut self, constraints: &CollectionUseConstraints<'db>) -> RunResult<()> {
        self.scan(1).await?;
        let slots = inference_table_slots(constraints.capacity())?;
        // These private maps only insert/extend/shrink. Normalization replaces each entire
        // IndexSet with a freshly collected set. Ordinary and controlled producers neither
        // remove entries nor retain tables after a failed fallible reserve. Thus capacity
        // bounds their backing tables; that is not true of arbitrary mutated hash tables.
        self.scan(checked(slots.checked_add(1))?).await?;
        self.add(table_comparison(constraints.len(), slots, 0)?)?;
        #[expect(
            clippy::iter_over_hash_type,
            reason = "the quotation sums independent retained constraint payloads"
        )]
        for types in constraints.values() {
            self.scan(1).await?;
            let slots = inference_table_slots(types.capacity())?;
            let mut payload = 0usize;
            let mut values = types.iter();
            while values.len() != 0 {
                let chunk = values.len().min(64);
                self.scan(chunk).await?;
                for ty in values.by_ref().take(chunk) {
                    payload = checked(payload.checked_add(ty.inline_payload_bytes()))?;
                }
            }
            self.add(table_comparison(types.len(), slots, payload)?)?;
        }
        Ok(())
    }

    async fn diagnostics(&mut self, diagnostics: &TypeCheckDiagnostics) -> RunResult<()> {
        self.scan(1).await?;
        let (len, _, used_len, used_capacity) = diagnostics.storage();
        // Suppressions only insert/extend/shrink, and their keys are fixed-size text ranges.
        self.add(table_comparison(
            used_len,
            inference_table_slots(used_capacity)?,
            0,
        )?)?;
        self.dense(len)?;
        self.scan(checked(len.checked_add(1))?).await?;
        for diagnostic in diagnostics {
            self.diagnostic(diagnostic).await?;
        }
        Ok(())
    }

    async fn diagnostic(&mut self, diagnostic: &Diagnostic) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        // Arc equality can compare DiagnosticInner even when both operands are shared.
        // DiagnosticId::Lint also compares the contents of its static lint-name string.
        self.add(diagnostic.id().as_str().len())?;
        self.add(diagnostic.headline_message().len())?;
        self.add(diagnostic.documentation_url().map_or(0, str::len))?;
        self.add(diagnostic.custom_concise_message().map_or(0, str::len))?;
        self.add(
            diagnostic
                .secondary_code()
                .map_or(0, |code| code.as_str().len()),
        )?;
        self.annotations(diagnostic.annotations()).await?;
        let subs = diagnostic.sub_diagnostics();
        self.dense(subs.len())?;
        self.scan(checked(subs.len().checked_add(1))?).await?;
        for sub in subs {
            self.scan(1).await?;
            self.add(sub.headline_message().len())?;
            self.annotations(sub.annotations()).await?;
        }
        if let Some(fix) = diagnostic.fix() {
            let edits = fix.edits();
            self.dense(edits.len())?;
            for chunk in edits.chunks(64) {
                self.scan(chunk.len()).await?;
                for edit in chunk {
                    self.add(edit.content().map_or(0, str::len))?;
                }
            }
        }
        Ok(())
    }

    async fn annotations(&mut self, annotations: &[Annotation]) -> RunResult<()> {
        self.dense(annotations.len())?;
        for chunk in annotations.chunks(64) {
            self.scan(chunk.len()).await?;
            for annotation in chunk {
                self.add(annotation.get_message().map_or(0, str::len))?;
                self.dense(annotation.tags().len())?;
                if let Some(file) = annotation.get_span().as_ruff_file() {
                    // SourceFile equality compares name and code, but not its lazy line index.
                    self.add(file.name().len())?;
                    self.add(file.source_text().len())?;
                }
            }
        }
        Ok(())
    }
}

fn checked(work: Option<usize>) -> RunResult<usize> {
    work.ok_or(RunError::Contract("inference equality quotation overflow"))
}

fn inference_table_slots(capacity: usize) -> RunResult<usize> {
    if capacity == 0 {
        Ok(0)
    } else {
        checked(
            capacity
                .checked_add(1)
                .and_then(|slots| slots.checked_mul(4))
                .and_then(|slots| slots.checked_add(32)),
        )
    }
}

fn table_comparison(len: usize, slots: usize, payload: usize) -> RunResult<usize> {
    // Native map/set equality checks lengths before looking up elements. Adding this bound
    // for both operands covers one sparse traversal and a full table probe for every key.
    // Each variable-size key can be hashed once and compared against every candidate key.
    checked(
        len.checked_add(1)
            .and_then(|count| count.checked_mul(slots.checked_add(payload)?.checked_add(1)?)),
    )
}

#[cfg(test)]
pub(in crate::types::infer) mod observations {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug)]
    pub(in crate::types::infer) struct Comparison {
        pub left: usize,
        pub right: usize,
        pub work: Option<usize>,
        pub started_remaining: Option<usize>,
        pub finished_remaining: Option<usize>,
    }

    thread_local! {
        static SELECTED: Cell<Option<usize>> = const { Cell::new(None) };
        static COMPARISON: Cell<Option<Comparison>> = const { Cell::new(None) };
    }

    pub(in crate::types::infer) fn reset(address: usize) {
        SELECTED.set(Some(address));
        COMPARISON.set(None);
    }

    pub(in crate::types::infer) fn snapshot() -> Option<Comparison> {
        COMPARISON.get()
    }

    pub(super) fn started<T>(left: &T, right: &T) -> bool {
        let left = std::ptr::from_ref(left).addr();
        let right = std::ptr::from_ref(right).addr();
        if SELECTED
            .get()
            .is_some_and(|selected| selected == left || selected == right)
        {
            COMPARISON.set(Some(Comparison {
                left,
                right,
                work: None,
                started_remaining: remaining(),
                finished_remaining: None,
            }));
            true
        } else {
            false
        }
    }

    pub(super) fn finished(observed: bool, work: usize) {
        if observed && let Some(mut comparison) = COMPARISON.get() {
            comparison.work = Some(work);
            comparison.finished_remaining = remaining();
            COMPARISON.set(Some(comparison));
        }
    }

    fn remaining() -> Option<usize> {
        salsa::with_attached_database(salsa::attempt_probe::remaining_allowance_for_diagnostics)
            .flatten()
    }
}
