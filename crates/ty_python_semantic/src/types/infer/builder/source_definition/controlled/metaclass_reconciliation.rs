//! Admitted direct-class reconciliation and the target ancestry retained during gradual checks.

use std::alloc::Layout;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult};

use super::storage::{StorageQuote, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::FxIndexSet;
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::types::class::metaclass_reconciliation::{
    ReconciliationEffects, could_inherit_from_with,
};
use crate::types::class_base::conversion::{ClassBaseConversion, resolve_class_base_with};
use crate::types::mro::construction::base_has_cyclic_mro_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::MroCursor;
use crate::types::relation::source::RelationSourceEffects;
use crate::types::relation::source::resources::{ClassRelation, RelationResourceAccess};
use crate::types::{ClassBase, ClassLiteral, ClassType, Specialization, StaticClassLiteral, Type};

/// Bounds the storage retained after insertion, including duplicate insertion attempts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AncestorBacking {
    table_slots: usize,
    ordered_capacity: usize,
    peak_attempt: usize,
}

/// Owns the target literals until the gradual-inheritance scan returns or is abandoned.
#[derive(Debug, Default)]
pub(in crate::types::infer) struct ReconciliationAncestors<'db> {
    literals: FxIndexSet<ClassLiteral<'db>>,
    backing: AncestorBacking,
    #[cfg(test)]
    retirement_observer: Option<fn()>,
}

#[cfg(test)]
impl Drop for ReconciliationAncestors<'_> {
    fn drop(&mut self) {
        if let Some(observer) = self.retirement_observer {
            observer();
        }
    }
}

/// Borrows canonical explicit bases without resolving entries ahead of the scan.
#[derive(Debug)]
pub(in crate::types::infer) struct ReconciliationBases<'db> {
    bases: &'db [Type<'db>],
    next: usize,
}

fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract(
        "metaclass ancestor storage quotation overflow",
    ))
}

/// Adds fixed representation transfers with checked sizes and copy counts.
fn transfer_bytes(parts: &[(usize, usize)]) -> RunResult<usize> {
    parts.iter().try_fold(0usize, |total, &(size, copies)| {
        checked(size.checked_mul(copies).and_then(|bytes| total.checked_add(bytes)))
    })
}

/// Quotes one IndexSet insertion and prepays retirement of old and retained backing.
fn insert_quote(
    len: usize,
    capacity: usize,
    backing: &AncestorBacking,
) -> Option<(StorageQuote, AncestorBacking)> {
    let old_slots = slots(capacity)?.max(backing.table_slots);
    let peak_attempt = len.checked_add(1)?.max(backing.peak_attempt);
    // Hashbrown may reserve before detecting a duplicate. A resize requires attempted length
    // above half the old full capacity; rounding the replacement uses fewer than eight buckets
    // per attempted entry. IndexMap's ordered buffer reserves up to that table's capacity.
    // Keep this bound tied to attempted length, rather than doubling a prior upper bound.
    let table_slots = slots(peak_attempt.checked_mul(2)?)?.max(backing.table_slots);
    let ordered_capacity = table_slots.max(backing.ordered_capacity);
    let (table, quoted_slots) = table_merge::<usize>(len, capacity, 1, old_slots)?;
    let mut sequence = sequence_merge::<(usize, ClassLiteral<'_>)>(len, capacity, 1)?;
    let ordered_layout = Layout::array::<(usize, ClassLiteral<'_>)>(ordered_capacity).ok()?;
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
            size_of::<(usize, ClassLiteral<'_>)>().checked_add(size_of::<usize>())?,
        )?)?;
    }
    quote.bytes = quote
        .bytes
        .checked_add(size_of::<(usize, ClassLiteral<'_>)>().checked_mul(2)?)?
        .checked_add(size_of::<AncestorBacking>())?;
    Some((
        quote,
        AncestorBacking {
            table_slots,
            ordered_capacity,
            peak_attempt,
        },
    ))
}

/// Inserts a target literal only after its storage and cleanup quotation is accepted.
fn insert_ancestor_with<'db>(
    ancestors: &mut ReconciliationAncestors<'db>,
    class: ClassLiteral<'db>,
    admit: impl FnOnce(StorageQuote) -> RunResult<()>,
) -> RunResult<()> {
    let (quote, backing) = checked(insert_quote(
        ancestors.literals.len(),
        ancestors.literals.capacity(),
        &ancestors.backing,
    ))?;
    admit(quote)?;
    ancestors.literals.insert(class);
    ancestors.backing = backing;
    Ok(())
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReconciliationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type MroCursor<'state> = MroCursor<'db> where Self: 'state;
    type Ancestors<'state> = ReconciliationAncestors<'db> where Self: 'state;
    type Bases<'state> = ReconciliationBases<'db> where Self: 'state;

    async fn checkpoint(&self) -> RunResult<()> {
        let bytes = transfer_bytes(&[
            (size_of::<ClassType<'db>>(), 4),
            (size_of::<Option<Type<'db>>>(), 2),
            (size_of::<bool>(), 4),
        ])?;
        self.local(16, bytes, || ()).await
    }

    async fn is_subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<bool> {
        self.reconciliation_relation(env, source, target, ClassRelation::Subtyping)
            .await
    }

    async fn could_inherit(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| could_inherit_from_with(env, source, target, self))
            .await?
            .await
    }

    async fn is_assignable(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<bool> {
        self.reconciliation_relation(env, source, target, ClassRelation::Assignability)
            .await
    }

    async fn is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        self.local(
            4,
            transfer_bytes(&[
                (size_of::<ClassType<'db>>(), 1),
                (size_of::<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>>(), 1),
                (size_of::<bool>(), 1),
            ])?,
            || (),
        )
        .await?;
        RelationSourceEffects::class_is_final(self, class).await
    }

    async fn has_cyclic_mro(&self, class: ClassType<'db>) -> RunResult<bool> {
        self.local(3, transfer_bytes(&[(size_of::<ClassBase<'db>>(), 1), (size_of::<bool>(), 1)])?, || ())
            .await?;
        base_has_cyclic_mro_with(MroFieldReads::new(self.db()), ClassBase::Class(class), self)
            .await
    }

    async fn mro_start(&self, class: ClassType<'db>) -> RunResult<Self::MroCursor<'_>> {
        self.local(1, size_of::<ClassType<'db>>(), || ()).await?;
        RelationSourceEffects::class_mro_start(self, class).await
    }

    async fn mro_next<'state>(
        &'state self,
        cursor: &mut Self::MroCursor<'state>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.local(
            3,
            transfer_bytes(&[
                (size_of::<Option<ClassBase<'db>>>(), 2),
                (size_of::<MroCursor<'db>>(), 1),
            ])?,
            || (),
        )
        .await?;
        RelationSourceEffects::class_mro_next(self, cursor).await
    }

    async fn literal(&self, class: ClassType<'db>) -> RunResult<ClassLiteral<'db>> {
        self.local(2, transfer_bytes(&[(size_of::<ClassType<'db>>(), 1), (size_of::<ClassLiteral<'db>>(), 1)])?, || ())
            .await?;
        match class {
            ClassType::NonGeneric(literal) => Ok(literal),
            ClassType::Generic(alias) => {
                let origin = self.field(alias.field_requests(self.db()).origin()).await?;
                self.initialize_value(|| ClassLiteral::Static(origin)).await
            }
        }
    }

    async fn ancestors_start(&self) -> RunResult<Self::Ancestors<'_>> {
        self.local(8, transfer_bytes(&[(size_of::<ReconciliationAncestors<'db>>(), 2)])?, || {
            ReconciliationAncestors {
                literals: FxIndexSet::default(),
                backing: AncestorBacking::default(),
                #[cfg(test)]
                retirement_observer: crate::types::infer::source_runtime::tests::inner_metaclass::reconciliation::observe_ancestors_created(self.db()),
            }
        })
        .await
    }

    async fn ancestors_insert<'state>(
        &'state self,
        ancestors: &mut Self::Ancestors<'state>,
        class: ClassLiteral<'db>,
    ) -> RunResult<()> {
        self.local(
            64,
            transfer_bytes(&[
                (size_of::<Option<(StorageQuote, AncestorBacking)>>(), 1),
                (size_of::<(StorageQuote, AncestorBacking)>(), 1),
                (size_of::<StorageQuote>(), 4),
                (size_of::<AncestorBacking>(), 1),
                (size_of::<Layout>(), 2),
                (size_of::<usize>(), 8),
            ])?,
            || (),
        )
        .await?;
        let endpoint = self.access.endpoint();
        // The set remains in the reducer while local_call drains children after refusal.
        endpoint
            .local_call(|| {
                insert_ancestor_with(ancestors, class, |quote| {
                    endpoint.admit_work(quote.work)?;
                    endpoint.admit(ExecutionWork::Resource { requested_bytes: quote.bytes })?;
                    endpoint.check_completion()
                })
            })
            .await;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::inner_metaclass::reconciliation::observe_ancestors_ready(
            self.db(),
            ancestors.literals.len(),
            ancestors.literals.capacity(),
            ancestors.backing.table_slots,
            ancestors.backing.ordered_capacity,
            ancestors.backing.peak_attempt,
        );
        Ok(())
    }

    async fn ancestors_contains<'state>(
        &'state self,
        ancestors: &Self::Ancestors<'state>,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.local(4, size_of::<usize>(), || ()).await?;
        let work = Self::checked(
            ancestors.backing.table_slots.checked_add(1)
                .and_then(|slots| slots.checked_mul(8))
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, transfer_bytes(&[(size_of::<ClassLiteral<'db>>(), 1), (size_of::<bool>(), 1)])?, || {
            ancestors.literals.contains(&class)
        })
        .await
    }

    async fn bases_start(&self, class: ClassLiteral<'db>) -> RunResult<Self::Bases<'_>> {
        self.local(2, size_of::<ClassLiteral<'db>>(), || ()).await?;
        let class = match class {
            ClassLiteral::Static(class) => class,
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => {
                return self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::ExplicitBases)).await;
            }
        };
        let bases = self.access.explicit_bases(class).await?;
        self.local(3, transfer_bytes(&[(size_of::<ReconciliationBases<'db>>(), 2)])?, || {
            ReconciliationBases { bases, next: 0 }
        })
        .await
    }

    async fn bases_next<'state>(
        &'state self,
        bases: &mut Self::Bases<'state>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(4, transfer_bytes(&[(size_of::<Option<Type<'db>>>(), 2), (size_of::<usize>(), 1)])?, || {
            let next = bases.bases.get(bases.next).copied();
            bases.next += usize::from(next.is_some());
            next
        })
        .await
    }

    async fn resolve_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
        base: Type<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        let conversion = self.local(
            4,
            transfer_bytes(&[
                (size_of::<Type<'db>>(), 1),
                (size_of::<ClassBaseConversion<'db>>(), 2),
                (size_of::<Option<ClassLiteral<'db>>>(), 1),
                (size_of::<Option<ClassBase<'db>>>(), 1),
            ])?,
            || ClassBaseConversion::from_explicit_type(base),
        ).await?;
        resolve_class_base_with(conversion, env, Some(class), self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Runs one fresh direct-class condition with admitted operand and resource-handle copies.
    async fn reconciliation_relation(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
        relation: ClassRelation,
    ) -> RunResult<bool> {
        let bytes = transfer_bytes(&[
            (size_of::<ClassType<'db>>(), 2),
            (size_of::<ClassRelation>(), 1),
            (size_of::<A::Resources>(), 1),
        ])?;
        let resources = self.local(4, bytes, || self.access.resources()).await?;
        resources.class_condition(self.db(), env, source, target, relation, self).await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use salsa::attempt_probe::Incomplete;
    use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RunError, RunResult};

    use super::{AncestorBacking, ReconciliationAncestors, StorageQuote, insert_ancestor_with, insert_quote};
    use crate::db::tests::setup_db;
    use crate::types::{ClassLiteral, KnownClass};
    use crate::{Db, ProgramEnvironment};

    /// Applies independent cumulative work and byte allowances to an insertion admission.
    #[derive(Debug)]
    struct Allowance {
        work: Cell<usize>,
        bytes: Cell<usize>,
    }

    impl Allowance {
        const fn new(work: usize, bytes: usize) -> Self {
            Self { work: Cell::new(work), bytes: Cell::new(bytes) }
        }

        fn quote(&self, quote: StorageQuote) -> RunResult<()> {
            self.admit(ExecutionWork::Work { units: quote.work })?;
            self.admit(ExecutionWork::Resource { requested_bytes: quote.bytes })
        }
    }

    impl ExecutionAdmission for Allowance {
        fn admit(&self, work: ExecutionWork) -> RunResult<()> {
            let (remaining, amount, refusal) = match work {
                ExecutionWork::Work { units } => (&self.work, units, Incomplete::Allowance),
                ExecutionWork::Resource { requested_bytes }
                | ExecutionWork::Task { requested_bytes } => {
                    (&self.bytes, requested_bytes, Incomplete::RequestedAllocation)
                }
                ExecutionWork::Poll => return Ok(()),
            };
            remaining.set(remaining.get().checked_sub(amount).ok_or(RunError::Refused(refusal))?);
            Ok(())
        }
    }

    /// Retrieves distinct existing class identities for storage controls without inferring test classes.
    fn literal<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        known: KnownClass,
    ) -> ClassLiteral<'db> {
        ClassLiteral::Static(known.try_to_class_literal(db, env).expect("builtin class must exist"))
    }

    thread_local! { static RETIRED: Cell<usize> = const { Cell::new(0) }; }

    fn retired() {
        RETIRED.with(|count| count.set(count.get() + 1));
    }

    /// Verifies that byte refusal preserves a populated set with spare capacity and its backing bound.
    /// A funded retry writes the entry; dropping the owner then retires it exactly once.
    #[test]
    fn spare_capacity_still_requires_bytes_before_mutation() {
        let db = setup_db();
        let env = db.program_environment();
        let first = literal(&db, &env, KnownClass::Int);
        let second = literal(&db, &env, KnownClass::Str);
        let allowance = Allowance::new(1_000_000, 16 * 1024 * 1024);
        let mut ancestors = ReconciliationAncestors::default();
        ancestors.retirement_observer = Some(retired);
        insert_ancestor_with(&mut ancestors, first, |quote| allowance.quote(quote)).unwrap();
        assert!(ancestors.literals.capacity() > ancestors.literals.len());
        let backing = ancestors.backing;
        let capacity = ancestors.literals.capacity();
        let before_retired = RETIRED.with(Cell::get);
        allowance.bytes.set(0);

        assert_eq!(
            insert_ancestor_with(&mut ancestors, second, |quote| allowance.quote(quote)),
            Err(RunError::Refused(Incomplete::RequestedAllocation))
        );
        assert_eq!(ancestors.literals.len(), 1);
        assert!(ancestors.literals.contains(&first));
        assert!(!ancestors.literals.contains(&second));
        assert_eq!(ancestors.literals.capacity(), capacity);
        assert_eq!(ancestors.backing, backing);
        assert_eq!(RETIRED.with(Cell::get), before_retired);

        allowance.bytes.set(16 * 1024 * 1024);
        insert_ancestor_with(&mut ancestors, second, |quote| allowance.quote(quote)).unwrap();
        assert!(ancestors.literals.contains(&second));
        assert!(allowance.bytes.get() < 16 * 1024 * 1024);
        drop(ancestors);
        assert_eq!(RETIRED.with(Cell::get), before_retired + 1);
    }

    /// Verifies that a full-set duplicate can enlarge the retained bound once without compounding it.
    /// Repeated attempts share one set so the assertions cover its complete allocation history.
    #[test]
    fn duplicate_at_capacity_retains_a_stable_attempt_bound() {
        let db = setup_db();
        let env = db.program_environment();
        let first = literal(&db, &env, KnownClass::Int);
        let second = literal(&db, &env, KnownClass::Str);
        let third = literal(&db, &env, KnownClass::Object);
        let allowance = Allowance::new(1_000_000, 16 * 1024 * 1024);
        let mut ancestors = ReconciliationAncestors::default();
        insert_ancestor_with(&mut ancestors, first, |quote| allowance.quote(quote)).unwrap();
        insert_ancestor_with(&mut ancestors, second, |quote| allowance.quote(quote)).unwrap();
        insert_ancestor_with(&mut ancestors, third, |quote| allowance.quote(quote)).unwrap();
        assert_eq!(ancestors.literals.len(), ancestors.literals.capacity());
        let full = ancestors.backing;
        insert_ancestor_with(&mut ancestors, first, |quote| allowance.quote(quote)).unwrap();
        let after_duplicate = ancestors.backing;
        assert!(after_duplicate.peak_attempt > full.peak_attempt);
        assert!(after_duplicate.table_slots >= full.table_slots);
        assert!(after_duplicate.ordered_capacity >= ancestors.literals.capacity());

        for _ in 0..8 {
            insert_ancestor_with(&mut ancestors, first, |quote| allowance.quote(quote)).unwrap();
            assert_eq!(ancestors.literals.len(), 3);
            assert_eq!(ancestors.backing, after_duplicate);
        }
    }

    /// Verifies that unrepresentable storage bounds refuse before any insertion can be admitted.
    #[test]
    fn overflowing_storage_bounds_have_no_quote() {
        assert!(insert_quote(usize::MAX, 0, &AncestorBacking::default()).is_none());
        assert!(insert_quote(0, usize::MAX, &AncestorBacking::default()).is_none());
        assert!(insert_quote(0, 0, &AncestorBacking {
            peak_attempt: usize::MAX,
            ..AncestorBacking::default()
        }).is_none());
    }
}
