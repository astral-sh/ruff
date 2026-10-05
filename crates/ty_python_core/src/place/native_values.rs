//! Work quotations for the native equality of retained place tables.

use std::sync::Arc;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::PlaceTable;

impl PlaceTable {
    /// Quote structural equality of two retained tables, including ordinary-produced values.
    ///
    /// Metadata scanning is admitted in bounded chunks. The returned work must be admitted
    /// separately before comparing the tables. Both `Arc`s stay borrowed across suspension
    /// or refusal; this quotation does not cover their cloning or final destruction.
    pub async fn quote_comparison<'run, 'db: 'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Arc<Self>,
        right: &Arc<Self>,
    ) -> RunResult<usize> {
        let mut quote = ComparisonQuote {
            endpoint: &endpoint,
            work: 0,
        };
        quote.table(left).await?;
        quote.table(right).await?;
        Ok(quote.work)
    }
}

struct ComparisonQuote<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    work: usize,
}

impl<'run, 'db: 'run> ComparisonQuote<'_, 'run, 'db> {
    async fn scan(&self, work: usize) -> RunResult<()> {
        // Only borrowed iterators and fixed-size counters cross the admission boundary.
        self.endpoint
            .local_call(|| self.endpoint.admit_work(work))
            .await;
        self.endpoint.checkpoint()?.await
    }

    fn add(&mut self, work: usize) -> RunResult<()> {
        self.work = checked(self.work.checked_add(work))?;
        Ok(())
    }

    async fn entries(
        &mut self,
        mut entries: impl ExactSizeIterator<Item = Option<usize>>,
    ) -> RunResult<()> {
        // Account for vector length comparison and entry iteration independently of payloads.
        self.add(checked(entries.len().checked_add(1))?)?;
        while entries.len() != 0 {
            let chunk = entries.len().min(64);
            self.scan(chunk).await?;
            for work in entries.by_ref().take(chunk) {
                self.add(checked(work)?)?;
            }
        }
        Ok(())
    }

    async fn table(&mut self, table: &PlaceTable) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        // Native equality compares these two vectors; their reverse indexes are excluded.
        // Count both operands without relying on Arc identity or shared string storage.
        self.entries(table.symbols.comparison_entry_work()).await?;
        self.entries(table.members.comparison_entry_work()).await
    }
}

fn checked(work: Option<usize>) -> RunResult<usize> {
    work.ok_or(RunError::Contract(
        "place-table equality quotation overflow",
    ))
}
