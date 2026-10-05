//! Admit return-callable scoping while canonical fields, mapping, and context construction do the work.

use std::alloc::Layout;

use rustc_hash::{FxHashMap, FxHashSet};
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::storage::{StorageQuote, ordered_merge, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects};
use crate::FxIndexMap;
use crate::types::generics::return_locations::TypeVarLocations;
use crate::types::generics::return_scoping::{
    CallableVariables, ReturnScopeEffects, ReturnScopeState, rescope_return_callables_with,
};
use crate::types::mapping::OwnedTypeMapping;
use crate::types::mapping::return_callables::{RetainedReturnCallables, RetainedReturnTypevars};
use crate::types::mapping::source::MappingSourceEffects;
use crate::types::signatures::{CallableSignature, Parameters};
use crate::types::{BoundTypeVarInstance, CallableType, GenericContext, Type};

/// Quotes insert-only handle tables, including probes, relocation, and old/new backing retirement.
fn hash_insert_quote<T>(len: usize, capacity: usize) -> Option<StorageQuote> {
    let (mut quote, replacement_slots) = table_merge::<T>(len, capacity, 1, 0)?;
    quote.work = quote.work.checked_mul(16)?.checked_add(8)?;
    quote.bytes = quote.bytes.checked_add(size_of::<T>().checked_mul(2)?)?;
    if len.checked_add(1)? > capacity {
        quote.work = quote
            .work
            .checked_add(slots(capacity)?)?
            .checked_add(replacement_slots)?;
        quote.bytes = quote.bytes.checked_add(len.checked_mul(size_of::<T>())?)?;
    }
    Layout::from_size_align(quote.bytes, align_of::<T>()).ok()?;
    Some(quote)
}

/// Quotes the replacement map's ordered entries and index table before growth or insertion.
fn renaming_quote(len: usize, capacity: usize, incoming: usize) -> Option<StorageQuote> {
    type Pair<'db> = (BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>);
    type Entry<'db> = (usize, Pair<'db>);
    let mut quote = ordered_merge::<Pair<'_>>(len, capacity, incoming)?;
    quote.work = quote
        .work
        .checked_mul(16)?
        .checked_add(incoming.checked_mul(8)?)?;
    quote.bytes = quote
        .bytes
        .checked_add(incoming.checked_mul(size_of::<Entry<'_>>().checked_mul(2)?)?)?;
    if len.checked_add(incoming)? > capacity {
        let (_, replacement_slots) = table_merge::<usize>(len, capacity, incoming, 0)?;
        quote.work = quote
            .work
            .checked_add(slots(capacity)?)?
            .checked_add(replacement_slots)?;
        quote.bytes = quote
            .bytes
            .checked_add(len.checked_mul(size_of::<Entry<'_>>())?)?;
    }
    Layout::from_size_align(quote.bytes, align_of::<Entry<'_>>()).ok()?;
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Moves eligible function variables into returned-callable contexts and returns the adjusted
    /// function context and return type. Runs on the current canonical signature's endpoint.
    pub(super) async fn scope_return_callables(
        &self,
        context: GenericContext<'db>,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
        definition: Definition<'db>,
    ) -> RunResult<(Option<GenericContext<'db>>, Type<'db>)> {
        self.type_parameter_future(|| {
            rescope_return_callables_with(context, parameters, return_type, definition, self)
        })
        .await?
        .await
    }

    /// Applies a collection quote before a callback can mutate or transfer its borrowed owner.
    async fn return_scope_storage<T>(
        &self,
        quote: Option<StorageQuote>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_quoted_with_fixed_transfers(
            quote
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract(
                    "return scope storage quotation overflow",
                )),
            action,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReturnScopeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Renamings = RetainedReturnTypevars<'run, 'db>;
    type Replacements = RetainedReturnCallables<'run, 'db>;

    async fn locations(
        &self,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
    ) -> RunResult<TypeVarLocations<'db>> {
        let locations = self.return_typevar_locations(parameters, return_type).await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::signature_annotations::return_scope_stage(
            self.db(),
            crate::types::infer::source_runtime::tests::signature_annotations::ReturnScopeStage::Locations,
        );
        Ok(locations)
    }

    async fn state(
        &self,
        mut locations: TypeVarLocations<'db>,
    ) -> RunResult<ReturnScopeState<'db>> {
        let work = self
            .local_with_fixed_transfers(8, 0, || {
                slots(locations.found_inside_callable_return.capacity())
                    .and_then(|slots| slots.checked_add(8))
            })
            .await?;
        self.return_scope_storage(work.map(|work| StorageQuote { work, bytes: 0 }), || ReturnScopeState {
            outside: std::mem::take(&mut locations.found_outside_callable_return),
            callables: std::mem::take(&mut locations.found_inside_callable_return).into_iter(),
            moved: FxHashSet::default(),
            replacements: FxHashMap::default(),
        })
        .await
    }

    async fn next_callable(
        &self,
        state: &mut ReturnScopeState<'db>,
    ) -> RunResult<Option<CallableVariables<'db>>> {
        let next = self
            .local_with_fixed_transfers(4, 0, || state.callables.next())
            .await?;
        let Some((callable, variables)) = next else {
            return Ok(None);
        };
        let quote = self
            .local_with_fixed_transfers(8, 0, || {
                Some(StorageQuote {
                    work: variables.len().checked_add(4)?,
                    bytes: size_of_val(&variables).checked_mul(4)?,
                })
            })
            .await?;
        self.return_scope_storage(quote, || ()).await?;
        self.local_with_fixed_transfers(4, 0, || {
            Some(CallableVariables {
                callable,
                variables: variables.into_iter(),
            })
        })
        .await
    }

    async fn candidates(
        &self,
        callable: &CallableVariables<'db>,
    ) -> RunResult<Vec<BoundTypeVarInstance<'db>>> {
        let count = self
            .local_with_fixed_transfers(2, 0, || callable.variables.len())
            .await?;
        let quote = self
            .local_with_fixed_transfers(24, 0, || {
                let mut quote = sequence_merge::<BoundTypeVarInstance<'db>>(0, 0, count)?;
                quote.work = quote
                    .work
                    .checked_add(count.checked_mul(2)?)?
                    .checked_add(4)?;
                Layout::array::<BoundTypeVarInstance<'db>>(count).ok()?;
                Some(quote)
            })
            .await?;
        self.return_scope_storage(quote, || Vec::with_capacity(count))
            .await
    }

    async fn next_variable(
        &self,
        callable: &mut CallableVariables<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local_with_fixed_transfers(3, 0, || callable.variables.next())
            .await
    }

    async fn outside(
        &self,
        state: &ReturnScopeState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let work = self
            .local_with_fixed_transfers(8, 0, || {
                slots(state.outside.capacity())
                    .and_then(|slots| slots.checked_mul(8)?.checked_add(8))
            })
            .await?;
        self.return_scope_storage(work.map(|work| StorageQuote { work, bytes: 0 }), || {
            state.outside.contains(&variable)
        })
        .await
    }

    async fn is_bound_by(
        &self,
        variable: BoundTypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                variable.identity_request(self.access.endpoint().field_request_context())
            })
            .await?;
        let identity = self.field_with_profile(request, &FixedFieldCopy).await?;
        self.local_with_fixed_transfers(3, 0, || identity.binding_context.definition() == Some(definition))
            .await
    }

    async fn keep(
        &self,
        candidates: &mut Vec<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(3, size_of::<BoundTypeVarInstance<'db>>(), || {
            candidates.push(variable)
        })
        .await
    }

    async fn is_empty(&self, candidates: &[BoundTypeVarInstance<'db>]) -> RunResult<bool> {
        self.local_with_fixed_transfers(1, 0, || candidates.is_empty())
            .await
    }

    async fn next_candidate(
        &self,
        candidates: &[BoundTypeVarInstance<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local_with_fixed_transfers(5, 0, || {
            let variable = candidates.get(*cursor).copied();
            if variable.is_some() {
                *cursor += 1;
            }
            variable
        })
        .await
    }

    async fn moved(
        &self,
        state: &mut ReturnScopeState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(48, 0, || {
                hash_insert_quote::<BoundTypeVarInstance<'db>>(
                    state.moved.len(),
                    state.moved.capacity(),
                )
            })
            .await?;
        self.return_scope_storage(quote, || {
            state.moved.insert(variable);
        })
        .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::signature_annotations::return_scope_stage(
            self.db(),
            crate::types::infer::source_runtime::tests::signature_annotations::ReturnScopeStage::MovedVariable,
        );
        Ok(())
    }

    async fn renamings(
        &self,
        candidates: &[BoundTypeVarInstance<'db>],
    ) -> RunResult<FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>> {
        let quote = self
            .local_with_fixed_transfers(48, 0, || renaming_quote(0, 0, candidates.len()))
            .await?;
        self.return_scope_storage(quote, || {
            FxIndexMap::with_capacity_and_hasher(candidates.len(), Default::default())
        })
        .await
    }

    async fn rename(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.rename_return_typevar(variable).await
    }

    async fn insert_renaming(
        &self,
        renamings: &mut FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
        original: BoundTypeVarInstance<'db>,
        renamed: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(48, 0, || {
                renaming_quote(renamings.len(), renamings.capacity(), 1)
            })
            .await?;
        self.return_scope_storage(quote, || {
            renamings.insert(original, renamed);
        })
        .await
    }

    async fn retain_renamings(
        &self,
        renamings: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<Self::Renamings> {
        self.local_with_fixed_transfers(
            8,
            size_of_val(&renamings)
                .checked_mul(4)
                .ok_or(RunError::Contract(
                    "return renaming transfer quotation overflow",
                ))?,
            || (),
        )
        .await?;
        let retained = self.type_parameter_future(|| self.retain_return_typevars(renamings))
            .await?
            .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::signature_annotations::return_scope_stage(
            self.db(),
            crate::types::infer::source_runtime::tests::signature_annotations::ReturnScopeStage::RetainedRenamings,
        );
        Ok(retained)
    }

    async fn callable(&self, variables: &CallableVariables<'db>) -> RunResult<CallableType<'db>> {
        self.local_with_fixed_transfers(1, 0, || variables.callable)
            .await
    }

    async fn map_signatures(
        &self,
        callable: CallableType<'db>,
        renamings: &Self::Renamings,
    ) -> RunResult<CallableSignature<'db>> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                callable
                    .field_requests(self.access.endpoint().field_request_context())
                    .signatures()
            })
            .await?;
        let signatures = self.field_with_profile(request, &FixedFieldBorrow).await?;
        let mapping = self
            .local_with_fixed_transfers(2, 0, || OwnedTypeMapping::ReturnCallables(*renamings))
            .await?;
        self.type_parameter_future(|| {
            self.apply_callable_signature_mapping(signatures, self.program, mapping)
        })
        .await?
        .await
    }

    async fn renamed_context(&self, renamings: &Self::Renamings) -> RunResult<GenericContext<'db>> {
        self.type_parameter_future(|| self.renamed_return_context(*renamings)).await?.await
    }

    async fn inherit_context(
        &self,
        signatures: &CallableSignature<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<CallableSignature<'db>> {
        self.type_parameter_future(|| self.inherit_callable_generic_context(signatures, context)).await?.await
    }

    async fn with_signatures(
        &self,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> RunResult<CallableType<'db>> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                callable
                    .field_requests(self.access.endpoint().field_request_context())
                    .kind()
            })
            .await?;
        let kind = self.field_with_profile(request, &FixedFieldCopy).await?;
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                callable
                    .field_requests(self.access.endpoint().field_request_context())
                    .deprecated()
            })
            .await?;
        let deprecated = self.field_with_profile(request, &FixedFieldCopy).await?;
        self.local_with_fixed_transfers(
            8,
            size_of::<CallableSignature<'db>>()
                .checked_mul(4)
                .ok_or(RunError::Contract(
                    "return callable transfer quotation overflow",
                ))?,
            || (),
        )
        .await?;
        self.type_parameter_future(|| {
            MappingSourceEffects::owned_mapped_callable(self, signatures, kind, deprecated)
        })
        .await?
        .await
    }

    async fn insert_replacement(
        &self,
        state: &mut ReturnScopeState<'db>,
        callable: CallableType<'db>,
        replacement: CallableType<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(48, 0, || {
                hash_insert_quote::<(CallableType<'db>, CallableType<'db>)>(
                    state.replacements.len(),
                    state.replacements.capacity(),
                )
            })
            .await?;
        self.return_scope_storage(quote, || {
            state.replacements.insert(callable, replacement);
        })
        .await
    }

    async fn retain_replacements(
        &self,
        state: &mut ReturnScopeState<'db>,
    ) -> RunResult<Self::Replacements> {
        let values = self
            .local_with_fixed_transfers(4, 0, || std::mem::take(&mut state.replacements))
            .await?;
        self.local_with_fixed_transfers(
            8,
            size_of_val(&values)
                .checked_mul(4)
                .ok_or(RunError::Contract(
                    "return replacement transfer quotation overflow",
                ))?,
            || (),
        )
        .await?;
        let retained = self.type_parameter_future(|| self.retain_return_callables(values))
            .await?
            .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::signature_annotations::return_scope_stage(
            self.db(),
            crate::types::infer::source_runtime::tests::signature_annotations::ReturnScopeStage::RetainedReplacements,
        );
        Ok(retained)
    }

    async fn map_return(
        &self,
        return_type: Type<'db>,
        replacements: &Self::Replacements,
    ) -> RunResult<Type<'db>> {
        let mapping = self
            .local_with_fixed_transfers(2, 0, || {
                OwnedTypeMapping::RescopeReturnCallables(*replacements)
            })
            .await?;
        self.type_parameter_future(|| self.apply_mapping(return_type, self.program, mapping))
            .await?
            .await
    }

    async fn trim_context(
        &self,
        context: GenericContext<'db>,
        state: &ReturnScopeState<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.type_parameter_future(|| self.trim_return_context(context, &state.moved)).await?.await
    }
}
