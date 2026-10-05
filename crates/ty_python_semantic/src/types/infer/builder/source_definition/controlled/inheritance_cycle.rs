use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::storage::{StorageQuote, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects};
use crate::types::class::static_literal::InheritanceCycle;
use crate::types::class::static_literal::inheritance_cycle::{
    Backing, CycleTraversalEffects, Enter, Frame, Step, Traversal, inheritance_cycle_inner_with,
};
use crate::types::{StaticClassLiteral, Type};

fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract(
        "inheritance cycle storage quotation overflow",
    ))
}

fn frame_push_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    let mut quote = sequence_merge::<Frame<'_>>(len, capacity, 1)?;
    if quote.bytes != 0 {
        Layout::from_size_align(quote.bytes, align_of::<Frame<'_>>()).ok()?;
        let replacement = quote.bytes.checked_div(size_of::<Frame<'_>>())?;
        quote.work = quote
            .work
            .checked_add(capacity)?
            .checked_add(replacement)?
            .checked_add(2)?;
        quote.bytes = quote
            .bytes
            .checked_add(len.checked_mul(size_of::<Frame<'_>>())?)?;
    }
    quote.bytes = quote
        .bytes
        .checked_add(size_of::<Frame<'_>>().checked_mul(2)?)?;
    Some(quote)
}

fn insert_quote(len: usize, capacity: usize, backing: &Backing) -> Option<(StorageQuote, Backing)> {
    let old_slots = slots(capacity)?.max(backing.table_slots);
    let peak_attempt = len.checked_add(1)?.max(backing.peak_attempt);
    // Hashbrown may reserve before detecting a duplicate. A resize requires attempted length
    // above half the old full capacity; rounding the replacement uses fewer than eight buckets
    // per attempted entry. IndexMap's ordered buffer reserves up to that table's capacity.
    // Keep this bound tied to attempted length, rather than doubling a prior upper bound.
    let table_slots = slots(peak_attempt.checked_mul(2)?)?.max(backing.table_slots);
    let ordered_capacity = table_slots.max(backing.ordered_capacity);
    let (table, quoted_slots) = table_merge::<usize>(len, capacity, 1, old_slots)?;
    let mut sequence = sequence_merge::<(usize, StaticClassLiteral<'_>)>(len, capacity, 1)?;
    let ordered_layout = Layout::array::<(usize, StaticClassLiteral<'_>)>(ordered_capacity).ok()?;
    if sequence.bytes != 0 {
        sequence.bytes = ordered_layout.size();
    }
    Layout::from_size_align(table.bytes, align_of::<usize>().max(16)).ok()?;
    let mut quote = table.checked_add(sequence)?;
    quote.work = quote
        .work
        .checked_add(old_slots.checked_add(1)?.checked_mul(8)?)?
        .checked_add(table_slots)?
        .checked_add(ordered_capacity)?
        .checked_add(8)?;
    if table.bytes != 0 || sequence.bytes != 0 {
        quote.work = quote
            .work
            .checked_add(
                len.checked_add(1)?
                    .checked_mul(quoted_slots.checked_add(1)?)?
                    .checked_mul(8)?,
            )?
            .checked_add(old_slots)?
            .checked_add(backing.ordered_capacity)?;
        quote.bytes = quote.bytes.checked_add(len.checked_mul(
            size_of::<(usize, StaticClassLiteral<'_>)>().checked_add(size_of::<usize>())?,
        )?)?;
    }
    quote.bytes = quote
        .bytes
        .checked_add(size_of::<(usize, StaticClassLiteral<'_>)>().checked_mul(2)?)?
        .checked_add(size_of::<Backing>())?;
    Some((
        quote,
        Backing {
            table_slots,
            ordered_capacity,
            peak_attempt,
        },
    ))
}

fn pop_quote(backing: &Backing) -> Option<StorageQuote> {
    Some(StorageQuote {
        work: backing
            .table_slots
            .checked_add(1)?
            .checked_mul(8)?
            .checked_add(8)?,
        bytes: size_of::<(usize, StaticClassLiteral<'_>)>().checked_mul(2)?,
    })
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn check_inheritance_cycle_program(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<()> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await
    }

    pub(in crate::types::infer) async fn infer_inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>> {
        self.check_inheritance_cycle_program(class).await?;
        inheritance_cycle_inner_with(class, self).await
    }

    #[cfg(test)]
    fn observe_cycle_traversal(&self, traversal: &Traversal<'db>, after_enter: bool) {
        crate::types::infer::source_runtime::tests::inheritance_cycle::observe_traversal(
            self.db(),
            traversal.frames.len(),
            traversal.active.len(),
            traversal.visited.len(),
            traversal.frames.capacity(),
            traversal.active_backing.table_slots,
            traversal.active_backing.ordered_capacity,
            traversal.visited_backing.table_slots,
            traversal.visited_backing.ordered_capacity,
            after_enter,
        );
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CycleTraversalEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start(&self) -> RunResult<Traversal<'db>> {
        self.local(8, size_of::<Traversal<'db>>() * 2, || {
            #[cfg(test)]
            {
                let mut traversal = Traversal::default();
                traversal.retirement_observer = Some(crate::types::infer::source_runtime::tests::inheritance_cycle::observe_traversal_drop);
                crate::types::infer::source_runtime::tests::inheritance_cycle::observe_traversal_created();
                traversal
            }
            #[cfg(not(test))]
            Traversal::default()
        })
            .await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        self.access.explicit_bases(class).await
    }

    async fn base_class(&self, base: Type<'db>) -> RunResult<Option<StaticClassLiteral<'db>>> {
        self.local(
            4,
            size_of::<Type<'db>>() + size_of::<Option<StaticClassLiteral<'db>>>(),
            || (),
        )
        .await?;
        match base {
            Type::ClassLiteral(class) => Ok(class.as_static()),
            Type::GenericAlias(alias) => self
                .field(alias.field_requests(self.db()).origin())
                .await
                .map(Some),
            _ => Ok(None),
        }
    }

    async fn push(
        &self,
        traversal: &mut Traversal<'db>,
        bases: &'db [Type<'db>],
        introduced_base: bool,
    ) -> RunResult<()> {
        self.work(4).await?;
        let quote = checked(frame_push_quote(
            traversal.frames.len(),
            traversal.frames.capacity(),
        ))?;
        self.local(quote.work, quote.bytes, || {
            traversal.push(bases, introduced_base);
            #[cfg(test)]
            self.observe_cycle_traversal(traversal, false);
        })
        .await
    }

    async fn next(&self, traversal: &mut Traversal<'db>) -> RunResult<Option<Step<'db>>> {
        self.work(4).await?;
        let quote = checked(pop_quote(&traversal.active_backing))?;
        let bytes = checked(
            quote
                .bytes
                .checked_add(size_of::<Frame<'db>>())
                .and_then(|bytes| bytes.checked_add(size_of::<Option<Step<'db>>>())),
        )?;
        self.local(quote.work, bytes, || {
            let next = traversal.next();
            #[cfg(test)]
            self.observe_cycle_traversal(traversal, false);
            next
        })
        .await
    }

    async fn enter(
        &self,
        traversal: &mut Traversal<'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.work(16).await?;
        let (active, active_backing) = checked(insert_quote(
            traversal.active.len(),
            traversal.active.capacity(),
            &traversal.active_backing,
        ))?;
        let (visited, visited_backing) = checked(insert_quote(
            traversal.visited.len(),
            traversal.visited.capacity(),
            &traversal.visited_backing,
        ))?;
        let pop = checked(pop_quote(&active_backing))?;
        let quote = checked(
            active
                .checked_add(visited)
                .and_then(|quote| quote.checked_add(pop)),
        )?;
        self.local(quote.work, quote.bytes, || {
            let entered = traversal.enter(class);
            traversal.active_backing = active_backing;
            let descend = match entered {
                Enter::Descend => {
                    traversal.visited_backing = visited_backing;
                    true
                }
                Enter::Revisited => {
                    traversal.visited_backing = visited_backing;
                    false
                }
                Enter::Active => false,
            };
            #[cfg(test)]
            self.observe_cycle_traversal(traversal, true);
            descend
        })
        .await
    }

    async fn classify(
        &self,
        traversal: &Traversal<'db>,
        root: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>> {
        self.work(4).await?;
        let work = checked(
            traversal
                .visited_backing
                .table_slots
                .checked_add(1)
                .and_then(|slots| slots.checked_mul(8)),
        )?;
        self.local(
            work,
            size_of::<Option<InheritanceCycle>>() + size_of::<StaticClassLiteral<'db>>(),
            || traversal.classify(root),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{Backing, frame_push_quote, insert_quote, pop_quote};

    #[test]
    fn retained_backing_survives_pop_and_reinsert() {
        let (_, first) = insert_quote(0, 0, &Backing::default()).unwrap();
        let (_, second) = insert_quote(0, 3, &first).unwrap();
        assert!(second.table_slots >= first.table_slots);
        assert!(second.ordered_capacity >= first.ordered_capacity);
        assert!(pop_quote(&second).unwrap().work > 0);
        assert!(insert_quote(1, 3, &second).unwrap().0.bytes > 0);
        assert!(frame_push_quote(1, 3).unwrap().bytes > 0);
    }

    #[test]
    fn storage_overflow_refuses_before_mutation() {
        assert!(insert_quote(usize::MAX, usize::MAX, &Backing::default()).is_none());
        assert!(frame_push_quote(usize::MAX, usize::MAX).is_none());
        assert!(
            pop_quote(&Backing {
                table_slots: usize::MAX,
                ..Backing::default()
            })
            .is_none()
        );
    }
}
