//! Collects implicit attribute names through admitted source traversal and owned storage.

use std::alloc::Layout;
use std::cmp::Ordering;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use ty_python_core::scope::{FileScopeId, Scope, ScopeId};
use ty_python_core::{AttributeScopesCursor, ScopeStep};

use super::{PreparedSource, SourceAccess, SourceEffects};
use crate::types::class::implicit_attributes::{
    ImplicitMembersCursor, ImplicitNamesEffects, implicit_attribute_names_with,
};
use crate::types::fallible_sort::{SortControl, SortStep, heapsort_with};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::nominal_members as observations;
use crate::types::storage_quote::{StorageQuote, buffer_push_quote, buffer_retirement};

pub(in crate::types::infer) struct SourceImplicitNamesSource<'db> {
    prepared: PreparedSource<'db>,
    scope: FileScopeId,
}

pub(in crate::types::infer) struct SourceImplicitNameBuffer<'db> {
    scope: ScopeId<'db>,
    names: Vec<Name>,
    max_name_len: usize,
    #[cfg(test)]
    _lifetime: observations::ImplicitNamesLifetime,
}

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract(
        "implicit name storage quotation overflow",
    ))
}

fn admit_quote(endpoint: &TaskEndpoint<'_, '_>, quote: StorageQuote) -> RunResult<()> {
    endpoint.admit_work(quote.work)?;
    if quote.bytes != 0 {
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: quote.bytes,
        })?;
    }
    endpoint.check_completion()
}

fn copy_quote(length: usize) -> RunResult<StorageQuote> {
    // Name uses CharStr's exact allocation: the text, a reference count, and an optional
    // length word. This also bounds inline names without inspecting their representation.
    let bytes = checked(length.checked_add(2 * size_of::<usize>()))?;
    Layout::from_size_align(bytes, align_of::<usize>())
        .map_err(|_| RunError::Contract("implicit name allocation layout overflow"))?;
    // Pay for copying the text, retiring its allocation, and initializing/dropping the handle.
    let work = checked(
        bytes
            .checked_mul(2)
            .and_then(|work| work.checked_add(2 * size_of::<Name>())),
    )?;
    Ok(StorageQuote { work, bytes })
}

fn dedup_work(length: usize, max_name_len: usize) -> RunResult<usize> {
    // Vec::dedup compares each following element once. Each iteration can move one handle
    // and drop one duplicate; the allocation and Name destructors were funded on insertion.
    let comparison = checked(
        max_name_len
            .checked_mul(2)
            .and_then(|work| work.checked_add(2 * size_of::<Name>())),
    )?;
    let iteration = checked(
        comparison
            .checked_add(size_of::<Name>())
            .and_then(|work| work.checked_add(8 * size_of::<usize>())),
    )?;
    checked(
        length
            .saturating_sub(1)
            .checked_mul(iteration)
            .and_then(|work| work.checked_add(size_of::<Vec<Name>>())),
    )
}

fn finish_quote(length: usize, capacity: usize) -> RunResult<StorageQuote> {
    let payload = Layout::array::<Name>(length)
        .map_err(|_| RunError::Contract("implicit names boxed allocation overflow"))?
        .size();
    let bytes = if length == capacity { 0 } else { payload };
    let retirement = checked(buffer_retirement::<Name>((length, capacity, true)))?;
    // Boxing may relocate the surviving handles. Retire the old capacity and prepay
    // disposal of the final boxed slice; each Name's own allocation is already funded.
    let work = checked(
        payload
            .checked_mul(2)
            .and_then(|work| work.checked_add(retirement)),
    )?;
    Ok(StorageQuote { work, bytes })
}

struct NameSortControl<'access, 'run, 'db> {
    endpoint: &'access TaskEndpoint<'run, 'db>,
    #[cfg(test)]
    db: &'db dyn crate::Db,
    #[cfg(test)]
    scope: ScopeId<'db>,
}

impl NameSortControl<'_, '_, '_> {
    fn admit(&self, work: usize) -> RunResult<()> {
        self.endpoint.admit_work(work)?;
        self.endpoint.check_completion()
    }
}

impl SortControl<Name> for NameSortControl<'_, '_, '_> {
    type Error = RunError;

    fn checkpoint(&self, step: SortStep) -> RunResult<()> {
        self.admit(step.work())
    }

    fn compare(&self, left: &Name, right: &Name) -> RunResult<Ordering> {
        self.admit(2 * size_of::<Name>())?;
        let bytes = checked(left.len().checked_add(right.len()))?;
        #[cfg(test)]
        observations::implicit_names_sort_comparison(self.db, self.scope);
        self.admit(checked(bytes.checked_add(2 * size_of::<Name>()))?)?;
        Ok(left.cmp(right))
    }

    fn before_swap(&self) -> RunResult<()> {
        // Two index bounds checks and three fixed-size element moves.
        self.admit(2 + 3 * size_of::<Name>())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_implicit_attribute_names(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<Box<[Name]>> {
        self.allocate_future(|| implicit_attribute_names_with(scope, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitNamesEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Source = SourceImplicitNamesSource<'db>;
    type Buffer = SourceImplicitNameBuffer<'db>;

    async fn source(&self, scope: ScopeId<'db>) -> RunResult<Self::Source> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        if prepared.file != file {
            return Err(RunError::Contract(
                "prepared implicit names file is foreign",
            ));
        }
        let file_scope = self
            .field(
                scope
                    .read_fields(self.access.endpoint().field_request_context())
                    .file_scope_id(),
            )
            .await?;
        self.work(size_of::<Self::Source>()).await?;
        Ok(SourceImplicitNamesSource {
            prepared,
            scope: file_scope,
        })
    }

    async fn scopes<'source>(
        &self,
        source: &'source Self::Source,
    ) -> RunResult<AttributeScopesCursor<'source>> {
        self.local(
            size_of::<AttributeScopesCursor<'_>>() + size_of::<Scope>(),
            0,
            || source.prepared.index.attribute_scopes_cursor(source.scope),
        )
        .await
    }

    async fn next_scope(
        &self,
        cursor: &mut AttributeScopesCursor<'_>,
    ) -> RunResult<Option<ScopeStep<FileScopeId>>> {
        let (work, bytes) = AttributeScopesCursor::step_cost();
        self.local(
            work + 1,
            bytes + size_of::<Option<ScopeStep<FileScopeId>>>(),
            || match cursor.step() {
                ScopeStep::Done => None,
                step => Some(step),
            },
        )
        .await
    }

    async fn members<'source>(
        &self,
        source: &'source Self::Source,
        scope: FileScopeId,
    ) -> RunResult<ImplicitMembersCursor<'source>> {
        self.local(size_of::<ImplicitMembersCursor<'_>>() + 2, 0, || {
            ImplicitMembersCursor::new(source.prepared.index.place_table(scope))
        })
        .await
    }

    async fn next_member<'table>(
        &self,
        cursor: &mut ImplicitMembersCursor<'table>,
    ) -> RunResult<Option<ScopeStep<&'table str>>> {
        self.local(cursor.step_work() + 1, 0, || match cursor.step() {
            ScopeStep::Done => None,
            step => Some(step),
        })
        .await
    }

    async fn buffer(&self, scope: ScopeId<'db>) -> RunResult<Self::Buffer> {
        self.local(2 * size_of::<Self::Buffer>(), 0, || {
            SourceImplicitNameBuffer {
                scope,
                names: Vec::new(),
                max_name_len: 0,
                #[cfg(test)]
                _lifetime: observations::ImplicitNamesLifetime::new(scope),
            }
        })
        .await
    }

    async fn append(&self, buffer: &mut Self::Buffer, name: &str) -> RunResult<()> {
        let (length, capacity, name_len) = self
            .local(3, 0, || {
                (buffer.names.len(), buffer.names.capacity(), name.len())
            })
            .await?;
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                let growth = buffer_push_quote::<Name>((length, capacity, true)).ok_or(
                    RunError::Contract("implicit names growth quotation overflow"),
                )?;
                let quote = growth
                    .checked_add(copy_quote(name_len)?)
                    .and_then(|quote| quote.checked_add(StorageQuote { work: 2, bytes: 0 }))
                    .ok_or(RunError::Contract(
                        "implicit names insertion quotation overflow",
                    ))?;
                admit_quote(endpoint, quote)?;
                buffer.names.push(Name::new(name));
                buffer.max_name_len = buffer.max_name_len.max(name_len);
                #[cfg(test)]
                observations::implicit_name_inserted(self.db(), buffer.scope, buffer.names.len());
                Ok(())
            })
            .await)
    }

    async fn sort(&self, buffer: &mut Self::Buffer) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        #[cfg(not(test))]
        let _ = buffer.scope;
        Ok(endpoint
            .local_call(|| {
                heapsort_with(
                    &mut buffer.names,
                    &NameSortControl {
                        endpoint,
                        #[cfg(test)]
                        db: self.db(),
                        #[cfg(test)]
                        scope: buffer.scope,
                    },
                )
            })
            .await)
    }

    async fn dedup(&self, buffer: &mut Self::Buffer) -> RunResult<()> {
        let (length, max_name_len) = self
            .local(2, 0, || (buffer.names.len(), buffer.max_name_len))
            .await?;
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                admit_quote(
                    endpoint,
                    StorageQuote {
                        work: dedup_work(length, max_name_len)?,
                        bytes: 0,
                    },
                )?;
                buffer.names.dedup();
                Ok(())
            })
            .await)
    }

    async fn finish(&self, buffer: Self::Buffer) -> RunResult<Box<[Name]>> {
        let (length, capacity) = self
            .local(2, 0, || (buffer.names.len(), buffer.names.capacity()))
            .await?;
        let mut owner = Some(buffer);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                admit_quote(endpoint, finish_quote(length, capacity)?)?;
                owner
                    .take()
                    .map(|buffer| buffer.names.into_boxed_slice())
                    .ok_or(RunError::Contract("implicit names buffer already consumed"))
            })
            .await)
    }
}
