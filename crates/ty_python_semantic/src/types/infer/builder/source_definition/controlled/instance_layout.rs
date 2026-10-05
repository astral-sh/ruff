//! Admitted slot accumulation remains alive during MRO and slot-selector requests.

use std::alloc::Layout;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::storage::{StorageQuote, dense_finish, ordered_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects};
use crate::FxIndexSet;
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::class::slots::layout::{
    InstanceLayoutEffects, InstanceLayoutFacts, finish_slots, instance_layout_with,
};
use crate::types::class::slots::{
    InstanceDictionary, InstanceLayout, SlotSelectorEffects, own_class_binding_with,
    slot_names_with,
};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::{ClassBase, ClassLiteral, ClassType, KnownClass, StaticClassLiteral};

#[cfg(test)]
use crate::types::infer::source_runtime::tests::instance_layout as observations;

pub(in crate::types) struct ControlledSlots {
    names: FxIndexSet<Name>,
    resident_name_bytes: usize,
    #[cfg(test)]
    _lifetime: observations::LayoutLifetime,
}

fn checked(quote: Option<StorageQuote>) -> RunResult<StorageQuote> {
    quote.ok_or(RunError::Contract(
        "instance layout storage quotation overflow",
    ))
}

fn insert_quote(
    len: usize,
    capacity: usize,
    resident_name_bytes: usize,
    incoming_name_bytes: usize,
) -> Option<StorageQuote> {
    let mut quote = ordered_merge::<Name>(len, capacity, 1)?;
    let old_slots = slots(capacity)?;
    let (_, new_slots) = table_merge::<usize>(len, capacity, 1, 0)?;
    let resident_bound = resident_name_bytes.checked_add(incoming_name_bytes)?;

    // A lookup can compare the incoming name with every occupied slot. Growth may revisit
    // every retained name and cached hash; ControlledSlots records byte totals as it grows.
    let name_lookup = old_slots
        .checked_add(1)?
        .checked_mul(
            incoming_name_bytes
                .checked_add(size_of::<Name>().checked_mul(2)?)?
                .checked_add(8)?,
        )?
        .checked_add(resident_name_bytes.checked_mul(2)?)?;
    let growth = len
        .checked_add(1)?
        .checked_mul(new_slots.checked_add(1)?)?
        .checked_mul(size_of::<(usize, Name)>().checked_add(8)?)?
        .checked_add(resident_bound)?;
    // Each insertion prepays disposal of duplicate clones, retained names, and both backing
    // buffers. If a later MRO or slot-selector request refuses, dropping ControlledSlots
    // needs no further admission.
    let disposal = resident_bound
        .checked_add(
            len.checked_add(1)?
                .checked_mul(size_of::<Name>().checked_add(2)?)?,
        )?
        .checked_add(new_slots)?
        .checked_add(quote.bytes.checked_mul(2)?)?;
    quote.work = quote
        .work
        .checked_add(name_lookup)?
        .checked_add(growth)?
        .checked_add(disposal)?
        .checked_add(incoming_name_bytes)?
        .checked_add(8)?;
    Layout::from_size_align(quote.bytes, align_of::<(usize, Name)>().max(16)).ok()?;
    Some(quote)
}

fn finish_quote(len: usize, capacity: usize, resident_name_bytes: usize) -> Option<StorageQuote> {
    let mut quote = dense_finish::<Name>(len, capacity)?;
    // Collection moves the ordered names into a boxed slice and retires the index table and
    // ordered backing. The returned slice's disposal is funded before ownership transfers.
    quote.work = quote
        .work
        .checked_add(slots(capacity)?)?
        .checked_add(
            capacity
                .checked_mul(size_of::<(usize, Name)>())?
                .checked_mul(2)?,
        )?
        .checked_add(len.checked_mul(size_of::<Name>().checked_mul(3)?.checked_add(8)?)?)?
        .checked_add(resident_name_bytes)?
        .checked_add(quote.bytes.checked_mul(2)?)?;
    Layout::from_size_align(quote.bytes, align_of::<Name>()).ok()?;
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<InstanceLayout> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        self.allocate_future(|| instance_layout_with(class, InstanceLayoutFacts, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InstanceLayoutEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type MroCursor = MroCursor<'db>;
    type Slots = ControlledSlots;
    type NamesCursor = std::slice::Iter<'db, Name>;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(8).await
    }

    async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_protocol_with(class, self).await
    }

    async fn unknown(&self) -> RunResult<InstanceLayout> {
        self.local(
            size_of::<InstanceLayout>() * 2 + 1,
            0,
            InstanceLayout::unknown,
        )
        .await
    }

    async fn new_slots(&self) -> RunResult<Self::Slots> {
        self.local(size_of::<ControlledSlots>() * 2 + 1, 0, || {
            ControlledSlots {
                names: FxIndexSet::default(),
                resident_name_bytes: 0,
                #[cfg(test)]
                _lifetime: observations::LayoutLifetime::new(),
            }
        })
        .await
    }

    async fn start_mro(&self, class: StaticClassLiteral<'db>) -> RunResult<Self::MroCursor> {
        self.local(1, size_of::<MroCursor<'db>>() * 2, || {
            MroCursor::new(class.into(), None)
        })
        .await
    }

    async fn next_mro_base(
        &self,
        cursor: &mut Self::MroCursor,
    ) -> RunResult<Option<ClassBase<'db>>> {
        let base = mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await?;
        #[cfg(test)]
        observations::observe_mro_advance(self.db(), self.access.endpoint(), base.is_some())?;
        Ok(base)
    }

    async fn class_literal(&self, class: ClassType<'db>) -> RunResult<ClassLiteral<'db>> {
        match class {
            ClassType::NonGeneric(class) => self.local(1, 0, || class).await,
            ClassType::Generic(alias) => Ok(ClassLiteral::Static(
                self.field(
                    alias
                        .field_requests(self.access.endpoint().field_request_context())
                        .origin(),
                )
                .await?,
            )),
        }
    }

    async fn slot_names(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<Self::NamesCursor>> {
        let names = slot_names_with(class, self).await?;
        self.local(size_of::<Self::NamesCursor>() * 2 + 1, 0, || {
            names.map(<[Name]>::iter)
        })
        .await
    }

    async fn next_name(&self, cursor: &mut Self::NamesCursor) -> RunResult<Option<&'db Name>> {
        self.local(2, 0, || cursor.next()).await
    }

    async fn is_dictionary_name(&self, name: &Name) -> RunResult<bool> {
        let bytes = self.local(1, 0, || name.as_str().len()).await?;
        let work = Self::checked(bytes.checked_add(9))?;
        self.local(work, 0, || name == "__dict__").await
    }

    async fn insert_slot(&self, slots: &mut Self::Slots, name: &Name) -> RunResult<()> {
        let (len, capacity, resident, incoming) = self
            .local(4, 0, || {
                (
                    slots.names.len(),
                    slots.names.capacity(),
                    slots.resident_name_bytes,
                    name.as_str().len(),
                )
            })
            .await?;
        let resident_bound = Self::checked(resident.checked_add(incoming))?;
        let quote = checked(insert_quote(len, capacity, resident, incoming))?;
        self.local(quote.work, quote.bytes, || {
            if slots.names.insert(name.clone()) {
                slots.resident_name_bytes = resident_bound;
            }
        })
        .await
    }

    async fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        own_class_binding_with(class, "__slots__", self).await
    }

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        SlotSelectorEffects::known(self, class).await
    }

    async fn finish(
        &self,
        slots: Self::Slots,
        dictionary: InstanceDictionary,
    ) -> RunResult<InstanceLayout> {
        let quote = checked(
            self.local(3, 0, || {
                finish_quote(
                    slots.names.len(),
                    slots.names.capacity(),
                    slots.resident_name_bytes,
                )
            })
            .await?,
        )?;
        self.local(quote.work, quote.bytes, || {
            finish_slots(slots.names, dictionary)
        })
        .await
    }
}
