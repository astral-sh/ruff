//! Admits property-deprecation collection through the current source invocation.
//! Recursive collectors retain their declaration vectors across canonical field and overload reads;
//! stable deduplication admits set growth and boxed output before allocation.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation, storage};
use crate::types::descriptor::effects::DescriptorOperation;
use crate::types::function::OverloadLiteral;
use crate::types::property_deprecations::{
    PropertyDeprecationEffects, collect_accessor_with, deduplicate_with,
};
use crate::types::{IntersectionType, PropertyDeprecations, Type, UnionType};
use crate::{Db, FxOrderSet};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PropertyDeprecationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote =
            Self::checked(work).and_then(|work| Self::checked(bytes).map(|bytes| (work, bytes)));
        self.local_quoted(quote, action).await
    }

    async fn properties(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.allocate_future(|| crate::types::property_metadata_with(ty, self))
            .await?
            .await
    }

    async fn accessor(
        &self,
        db: &'db dyn Db,
        accessor: Type<'db>,
        functions: &mut Vec<OverloadLiteral<'db>>,
    ) -> RunResult<()> {
        self.allocate_future(|| collect_accessor_with(db, accessor, functions, self))
            .await?
            .await
    }

    async fn union_elements(
        &self,
        _db: &'db dyn Db,
        union: UnionType<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn intersection_elements(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        self.field(intersection.field_requests(db).positive()).await
    }

    async fn deduplicate(
        &self,
        functions: Vec<OverloadLiteral<'db>>,
    ) -> RunResult<Box<[OverloadLiteral<'db>]>> {
        self.allocate_future(|| deduplicate_with(functions, self))
            .await?
            .await
    }

    async fn insert(
        &self,
        functions: &mut FxOrderSet<OverloadLiteral<'db>>,
        function: OverloadLiteral<'db>,
    ) -> RunResult<()> {
        let (len, capacity) = self
            .local(2, size_of::<(usize, usize)>(), || {
                (functions.len(), functions.capacity())
            })
            .await?;
        let quote = storage::ordered_merge::<OverloadLiteral<'db>>(len, capacity, 1)
            .and_then(|mut quote| {
                quote.work = quote
                    .work
                    .checked_add(storage::slots(capacity)?)?
                    .checked_add(len)?
                    .checked_add(1)?;
                quote.bytes = quote.bytes.checked_add(size_of::<OverloadLiteral<'db>>())?;
                Some((quote.work, quote.bytes))
            })
            .ok_or(RunError::Contract(
                "property deprecation set quotation overflow",
            ));
        self.local_quoted(quote, || {
            if len == capacity {
                functions.reserve_exact(capacity.max(1));
            }
            functions.insert(function);
        })
        .await
    }

    async fn finish(
        &self,
        functions: FxOrderSet<OverloadLiteral<'db>>,
    ) -> RunResult<Box<[OverloadLiteral<'db>]>> {
        let (len, capacity) = self
            .local(2, size_of::<(usize, usize)>(), || {
                (functions.len(), functions.capacity())
            })
            .await?;
        let quote = storage::dense_finish::<OverloadLiteral<'db>>(len, capacity)
            .and_then(|quote| {
                Some((
                    quote.work.checked_add(storage::slots(capacity)?)?,
                    quote
                        .bytes
                        .checked_add(size_of::<Box<[OverloadLiteral<'db>]>>())?,
                ))
            })
            .ok_or(RunError::Contract(
                "property deprecation result quotation overflow",
            ));
        self.local_quoted(quote, || functions.into_iter().collect())
            .await
    }

    async fn intern(
        &self,
        _db: &'db dyn Db,
        _functions: &mut [Box<[OverloadLiteral<'db>]>; 3],
    ) -> RunResult<PropertyDeprecations<'db>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::PropertyMetadata,
        ))
        .await
    }

    async fn combine(
        &self,
        _db: &'db dyn Db,
        _left: PropertyDeprecations<'db>,
        _right: PropertyDeprecations<'db>,
        _intersection: bool,
    ) -> RunResult<PropertyDeprecations<'db>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::PropertyMetadata,
        ))
        .await
    }
}
