//! Work quotations for the native equality of retained use-def maps.
//!
//! Both operands can have been produced by ordinary semantic indexing. Metadata scans are
//! admitted separately from the returned quotation for Salsa's structural comparison.

use std::sync::Arc;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{Bindings, ConstraintTables, MultiBindingsByUse, UseDefMap, UseDefMapExtra};
use crate::LoopHeader;

impl<'db> UseDefMap<'db> {
    /// Quotes the structural comparison of two canonical query results.
    ///
    /// The borrowed owners remain alive across admitted metadata scans. This reads retained
    /// fields, including shared binding/declaration tables, without resolving Salsa handles
    /// or assuming that either operand was produced by controlled execution. The caller must
    /// admit the returned work before invoking native equality.
    pub async fn quote_comparison<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Arc<Self>,
        right: &Arc<Self>,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = ComparisonQuote {
            endpoint: &endpoint,
            work: 0,
        };
        quote.map(left).await?;
        quote.map(right).await?;
        Ok(quote.work)
    }
}

struct ComparisonQuote<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    work: usize,
}

impl<'run, 'db: 'run> ComparisonQuote<'_, 'run, 'db> {
    async fn scan(&self, work: usize) -> RunResult<()> {
        // Only borrowed cursors cross admission boundaries. No scan allocates scratch
        // storage or invokes equality, and refusal leaves both native operands intact.
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

    async fn map(&mut self, map: &UseDefMap<'db>) -> RunResult<()> {
        self.scan(1).await?;
        // Covers the outer Arc, optional table/extra discriminants and terminal
        // reachability ID. DefinitionEntry contains only a tag and a Salsa handle.
        self.add(1)?;
        self.dense(map.all_definitions.states.len())?;
        if let Some(tables) = map.constraint_tables.as_deref() {
            self.constraints(tables).await?;
        }

        self.scan(1).await?;
        // Arc equality may traverse these tables across generations. End offsets, packed
        // definition IDs, narrowing/reachability IDs and declaration flags are all shallow.
        self.dense(map.interned_bindings.ends.len())?;
        self.dense(map.interned_bindings.live_bindings.len())?;
        self.dense(map.interned_declarations.ends.len())?;
        self.dense(map.interned_declarations.live_declarations.len())?;

        // FrozenMap equality compares its sorted entries. These entries contain a
        // Definition handle, a binding ID and an optional declaration ID. The other
        // dense entries contain text ranges, flags and scoped IDs, never referenced data.
        self.dense(map.range_reachability.len())?;
        self.dense(map.definitions_by_definition.iter().len())?;
        self.dense(map.symbol_states.len())?;
        if let Some(extra) = map.extra.as_deref() {
            self.extra(extra).await?;
        }
        Ok(())
    }

    async fn constraints(&mut self, tables: &ConstraintTables<'db>) -> RunResult<()> {
        self.scan(1).await?;
        // PredicateNode variants contain only fixed-size IDs, booleans and Salsa handles.
        // PatternPredicate's stored pattern tree is behind its handle, so equality does
        // not traverse it. SubjectElementPatternPredicate adds an ExpressionNodeKey.
        self.dense(tables.predicates.len())?;
        self.dense(tables.predicate_narrowing_targets.0.len())?;
        self.add(checked(tables.reachability_constraints.comparison_work())?)?;
        self.add(checked(tables.narrowing_constraints.comparison_work())?)?;
        Ok(())
    }

    async fn extra(&mut self, extra: &UseDefMapExtra) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        self.dense(extra.bindings_by_use.len())?;
        self.dense(extra.if_chain_start_by_use.iter().len())?;
        self.dense(extra.member_states.len())?;
        // Snapshots retain either a narrowing ID or an interned binding ID; the binding
        // table itself is already included above. Boolean roots are fixed-size NodeIndex IDs.
        self.dense(extra.enclosing_snapshots.len())?;
        self.dense(extra.boolean_test_roots.len())?;
        self.multi_bindings(&extra.multi_bindings_by_use).await?;

        self.dense(extra.loop_headers.len())?;
        let mut headers = extra.loop_headers.iter();
        while headers.len() != 0 {
            let chunk = headers.len().min(64);
            self.scan(chunk).await?;
            for header in headers.by_ref().take(chunk) {
                self.loop_header(header).await?;
            }
        }
        Ok(())
    }

    async fn multi_bindings(&mut self, bindings: &MultiBindingsByUse) -> RunResult<()> {
        self.dense(bindings.0.len())?;
        for chunk in bindings.0.chunks(64) {
            self.scan(chunk.len()).await?;
            for (_, states) in chunk {
                self.dense(states.len())?;
                for chunk in states.chunks(64) {
                    self.scan(chunk.len()).await?;
                    for state in chunk {
                        self.bindings(state)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn bindings(&mut self, bindings: &Bindings) -> RunResult<()> {
        // SmallVec equality compares its initialized slice. The optional unbound
        // narrowing constraint is one additional fixed-size discriminant/ID pair.
        self.add(1)?;
        self.dense(bindings.as_slice().len())
    }

    async fn loop_header(&mut self, header: &LoopHeader) -> RunResult<()> {
        self.scan(1).await?;
        // LoopHeader::add_binding only inserts into this private map; there are no
        // removals or failed fallible reserves. Its capacity therefore bounds its backing
        // slots. Keys are fixed-size ScopedPlaceId values, including their enum tags.
        let slots = table_slots(header.bindings.capacity())?;
        self.add(table_comparison(header.bindings.len(), slots)?)?;
        // Advancing a hash-map cursor can cross empty slots, so admit the full traversal
        // before creating that cursor, then admit each chunk's vector metadata reads.
        self.scan(checked(slots.checked_add(1))?).await?;
        #[expect(
            clippy::iter_over_hash_type,
            reason = "the quotation sums independent retained binding payloads"
        )]
        let mut values = header.bindings.values();
        while values.len() != 0 {
            let chunk = values.len().min(64);
            self.scan(chunk).await?;
            for bindings in values.by_ref().take(chunk) {
                // Native map equality compares a value only after finding its unique
                // matching key. Sum the slice work once per value, separately from probes.
                self.dense(bindings.len())?;
            }
        }
        Ok(())
    }
}

fn checked(work: Option<usize>) -> RunResult<usize> {
    work.ok_or(RunError::Contract(
        "use-def map equality quotation overflow",
    ))
}

fn table_slots(capacity: usize) -> RunResult<usize> {
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

fn table_comparison(len: usize, slots: usize) -> RunResult<usize> {
    // HashMap equality checks lengths first. Summing both operands covers its sparse
    // traversal and a full-table probe for each fixed-size key, even under collisions.
    checked(
        len.checked_add(1)
            .and_then(|count| count.checked_mul(slots.checked_add(1)?)),
    )
}
