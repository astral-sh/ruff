//! Shared collection-use constraint merging and passive storage quotations.

use crate::types::infer::CollectionUseConstraints;

pub(in crate::types::infer::builder) fn merge_collection_constraints<'db>(
    target: &mut CollectionUseConstraints<'db>,
    source: &CollectionUseConstraints<'db>,
) {
    target.reserve(source.len());
    #[expect(
        clippy::iter_over_hash_type,
        reason = "constraints for distinct collection definitions are merged independently"
    )]
    for (definition, source_types) in source {
        let target_types = target.entry(*definition).or_default();
        // Rebuilding an absent set preserves its order without cloning its hidden hash-table
        // capacity. The same reservation covers the worst case where every source type is new.
        target_types.reserve_exact(source_types.len());
        target_types.extend(source_types.iter().copied());
    }
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types::infer::builder) use quotes::{
    constraints_finalization_quote, constraints_merge_quote, constraints_metadata_quote,
};

#[cfg(feature = "experimental-analysis")]
mod quotes {
    use ty_python_core::definition::Definition;

    use super::CollectionUseConstraints;
    use crate::FxIndexSet;
    use crate::types::Type;
    use crate::types::constraints::control::hash_slots;
    use crate::types::infer::builder::source_definition::controlled::storage::StorageQuote;

    fn slots(capacity: usize) -> Option<usize> {
        hash_slots::<std::convert::Infallible>(capacity).ok()
    }

    // These quotations cover the pinned insertion-only table operations and their requested
    // storage. They do not bound allocator latency or collision-dependent native probes.
    fn table_allocation(required: usize, entry_bytes: usize) -> Option<StorageQuote> {
        if required == 0 {
            return Some(StorageQuote::default());
        }
        let slots = slots(required.checked_mul(2)?)?;
        let bytes = entry_bytes.checked_add(1)?.checked_mul(slots)?;
        if bytes > isize::MAX as usize {
            return None;
        }
        Some(StorageQuote { work: slots, bytes })
    }

    fn table_growth(
        len: usize,
        capacity: usize,
        additional: usize,
        entry_bytes: usize,
    ) -> Option<StorageQuote> {
        let required = len.checked_add(additional)?;
        if required <= capacity {
            return Some(StorageQuote::default());
        }
        table_allocation(required, entry_bytes)?.checked_add(StorageQuote {
            work: slots(capacity)?.checked_add(len)?,
            bytes: 0,
        })
    }

    /// Quotes the outer traversal before inspecting the inner sets' lengths and payloads.
    pub(in crate::types::infer::builder) fn constraints_metadata_quote(
        constraints: &CollectionUseConstraints<'_>,
    ) -> Option<StorageQuote> {
        Some(StorageQuote {
            work: slots(constraints.capacity())?
                .checked_add(constraints.len())?
                .checked_add(1)?,
            bytes: 0,
        })
    }

    pub(in crate::types::infer::builder) fn constraints_merge_quote<'db>(
        target: &CollectionUseConstraints<'db>,
        source: &CollectionUseConstraints<'db>,
    ) -> Option<StorageQuote> {
        let mut quote = table_growth(
            target.len(),
            target.capacity(),
            source.len(),
            size_of::<(Definition<'db>, FxIndexSet<Type<'db>>)>(),
        )?
        .checked_add(StorageQuote {
            work: source.len().checked_mul(4)?.checked_add(1)?,
            bytes: 0,
        })?;
        #[expect(
            clippy::iter_over_hash_type,
            reason = "the quotation adds independent contributions for each definition"
        )]
        for (definition, source_types) in source {
            let target_types = target.get(definition);
            let old_len = target_types.map_or(0, FxIndexSet::len);
            let old_capacity = target_types.map_or(0, FxIndexSet::capacity);
            let required = old_len.checked_add(source_types.len())?;
            let mut work = old_len.checked_add(source_types.len())?.checked_add(3)?;
            for ty in target_types.into_iter().flatten().chain(source_types) {
                work = work.checked_add(ty.inline_payload_bytes())?;
            }
            quote = quote.checked_add(StorageQuote { work, bytes: 0 })?;
            if required > old_capacity {
                // IndexSet owns a hash table of indices and an ordered buffer whose entries
                // retain a cached usize hash beside Type. Include alignment padding rather
                // than assuming the private Bucket layout matches a Rust tuple.
                let entry_bytes = size_of::<Type<'db>>()
                    .checked_add(size_of::<usize>())?
                    .checked_add(align_of::<Type<'db>>().max(align_of::<usize>()))?;
                let bytes = required.checked_mul(entry_bytes)?;
                if bytes > isize::MAX as usize {
                    return None;
                }
                quote = quote
                    .checked_add(table_allocation(required, size_of::<usize>())?)?
                    .checked_add(StorageQuote {
                        work: slots(old_capacity.checked_mul(2)?)?
                            .checked_add(old_len)?
                            .checked_add(required)?,
                        bytes,
                    })?;
            }
        }
        Some(quote)
    }

    pub(in crate::types::infer::builder) fn constraints_finalization_quote<'db>(
        target: &CollectionUseConstraints<'db>,
    ) -> Option<StorageQuote> {
        let mut quote = constraints_metadata_quote(target)?;
        // The original finalizer shrinks only the outer map. Inner sets remain owned by the
        // result; their dense Copy elements can be retired without semantic calls.
        #[expect(
            clippy::iter_over_hash_type,
            reason = "retirement costs are independent of definition iteration order"
        )]
        for types in target.values() {
            quote.work = quote.work.checked_add(types.len())?.checked_add(2)?;
        }
        if target.len() < target.capacity() {
            quote = quote.checked_add(table_allocation(
                target.len(),
                size_of::<(Definition<'db>, FxIndexSet<Type<'db>>)>(),
            )?)?;
        }
        Some(quote)
    }
}
