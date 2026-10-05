//! Typed descriptions of every passive memo owned by an interned ingredient.

use std::marker::PhantomData;

use super::{RunError, RunResult};
use crate::function::memo::{
    KeyRetirementError, Memo, inspect_passive_memo_bounded, inspect_passive_singleton_bounded,
};
use crate::function::{Configuration, IngredientImpl, PassiveMemoProfile};
use crate::ingredient::Ingredient;
use crate::quote::QuoteFuel;
use crate::table::memo::MemoTableWithTypesMut;
use crate::zalsa::{IngredientIndex, MemoIngredientIndex, Zalsa};

/// One genuine function memo, checked against its owning interned ingredient.
/// The query key may be a supertype containing the owner; the installed slot must
/// still contain this function's exact memo type.
#[doc(hidden)]
pub struct PassiveMemo<'db, I, C, P>
where
    I: crate::interned::Configuration,
    C: Configuration,
{
    memo: &'db IngredientImpl<C>,
    index: MemoIngredientIndex,
    marker: PhantomData<fn() -> (I, P)>,
}

impl<'db, I, C, P> PassiveMemo<'db, I, C, P>
where
    I: crate::interned::Configuration,
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    pub(super) fn new(
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
        memo: &'db IngredientImpl<C>,
    ) -> Result<Self, KeyRetirementError> {
        if !zalsa
            .ingredients()
            .nth(owner.ingredient_index().as_u32() as usize)
            .is_some_and(|actual| std::ptr::addr_eq(actual, owner as &dyn Ingredient))
            || !zalsa
                .ingredients()
                .nth(memo.index.as_u32() as usize)
                .is_some_and(|actual| std::ptr::addr_eq(actual, memo as &dyn Ingredient))
        {
            return Err(KeyRetirementError::Mapping);
        }
        let index = memo.passive_memo_index(zalsa, owner)?;
        Ok(Self {
            memo,
            index,
            marker: PhantomData,
        })
    }

    fn validate(
        &self,
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
    ) -> Result<(), KeyRetirementError> {
        let actual = Self::new(zalsa, owner, self.memo)?;
        if actual.index != self.index {
            return Err(KeyRetirementError::Mapping);
        }
        Ok(())
    }

    pub(super) fn validate_singleton(
        &self,
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
    ) -> Result<(), KeyRetirementError> {
        self.validate(zalsa, owner)?;
        if self.memo.query_key_memo_index(zalsa, owner)? != self.index {
            return Err(KeyRetirementError::Mapping);
        }
        Ok(())
    }

    pub(super) fn inspect_singleton(
        &self,
        inspection: PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        let quote = inspect_passive_singleton_bounded::<C, P>(inspection.table, self.index, fuel)?;
        Ok(PassiveRetirement {
            present: u8::from(quote.present),
            output_units: quote.output_units,
        })
    }
}

/// Combines typed schema fragments whose complete owner coverage is checked at registration.
#[doc(hidden)]
pub struct PassiveMemoGroup<L, R> {
    left: L,
    right: R,
}

impl<L, R> PassiveMemoGroup<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

/// Complete typed memo schemas; only the runtime supplies implementations.
#[doc(hidden)]
pub trait PassiveMemoSchema<'db, I: crate::interned::Configuration>: sealed::Ops<'db, I> {}

/// A selected memo table available only to the runtime's typed schema inspector.
#[doc(hidden)]
pub struct PassiveMemoInspection<'a> {
    table: MemoTableWithTypesMut<'a>,
}

impl<'a> PassiveMemoInspection<'a> {
    pub(super) fn new(table: MemoTableWithTypesMut<'a>) -> Self {
        Self { table }
    }
}

/// The selected generation's occupancy and accepted output quote, never a live-table lookup.
#[doc(hidden)]
pub struct PassiveRetirement<P = u8> {
    present: P,
    output_units: usize,
}

impl PassiveRetirement {
    pub(super) fn empty() -> Self {
        Self {
            present: 0,
            output_units: 0,
        }
    }
}

impl<P> PassiveRetirement<P> {
    pub(super) fn output_units(&self) -> usize {
        self.output_units
    }
}

fn retirement_error(error: KeyRetirementError) -> RunError {
    RunError::Contract(error.value_message())
}

mod sealed {
    use super::*;

    pub trait Ops<'db, I: crate::interned::Configuration> {
        type Presence: Copy;
        const LEN: usize;

        fn empty_presence() -> Self::Presence;

        fn empty_retirement() -> PassiveRetirement<Self::Presence> {
            PassiveRetirement {
                present: Self::empty_presence(),
                output_units: 0,
            }
        }

        fn validate(
            &self,
            zalsa: &Zalsa,
            owner: &crate::interned::IngredientImpl<I>,
        ) -> RunResult<()> {
            if !zalsa
                .ingredients()
                .nth(owner.ingredient_index().as_u32() as usize)
                .is_some_and(|actual| std::ptr::addr_eq(actual, owner as &dyn Ingredient))
                || owner.memo_table_types().len() != Self::LEN
            {
                return Err(retirement_error(KeyRetirementError::Mapping));
            }
            self.validate_members(zalsa, owner)
        }

        fn validate_members(
            &self,
            zalsa: &Zalsa,
            owner: &crate::interned::IngredientImpl<I>,
        ) -> RunResult<()>;

        fn inspect(
            &self,
            mut inspection: PassiveMemoInspection<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<PassiveRetirement<Self::Presence>, KeyRetirementError> {
            fuel.consume(1)?;
            if inspection.table.memo_type_count() != Self::LEN {
                return Err(KeyRetirementError::Mapping);
            }
            self.inspect_members(&mut inspection, fuel)
        }

        fn inspect_members(
            &self,
            inspection: &mut PassiveMemoInspection<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<PassiveRetirement<Self::Presence>, KeyRetirementError>;

        fn contains_index(&self, index: MemoIngredientIndex) -> bool;

        fn visit_indices<F: FnMut(MemoIngredientIndex) -> RunResult<()>>(
            &self,
            visitor: &mut F,
        ) -> RunResult<()>;

        fn discard_function_at(
            &self,
            slot: usize,
            selected: &PassiveRetirement<Self::Presence>,
        ) -> Option<IngredientIndex>;
    }
}

pub(super) use sealed::Ops as SchemaOps;

impl<'db, I: crate::interned::Configuration> PassiveMemoSchema<'db, I> for () {}

impl<'db, I: crate::interned::Configuration> sealed::Ops<'db, I> for () {
    type Presence = u8;
    const LEN: usize = 0;

    fn empty_presence() -> Self::Presence {
        0
    }

    fn validate(&self, zalsa: &Zalsa, owner: &crate::interned::IngredientImpl<I>) -> RunResult<()> {
        if !zalsa
            .ingredients()
            .nth(owner.ingredient_index().as_u32() as usize)
            .is_some_and(|actual| std::ptr::addr_eq(actual, owner as &dyn Ingredient))
            || owner.memo_table_types().len() != 0
        {
            return Err(retirement_error(KeyRetirementError::Mapping));
        }
        Ok(())
    }

    fn inspect(
        &self,
        inspection: PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        if inspection.table.memo_type_count() != 0 {
            return Err(KeyRetirementError::Mapping);
        }
        Ok(PassiveRetirement::empty())
    }

    fn validate_members(
        &self,
        _zalsa: &Zalsa,
        _owner: &crate::interned::IngredientImpl<I>,
    ) -> RunResult<()> {
        Ok(())
    }

    fn inspect_members(
        &self,
        _inspection: &mut PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        Ok(PassiveRetirement::empty())
    }

    fn contains_index(&self, _index: MemoIngredientIndex) -> bool {
        false
    }

    fn visit_indices<F: FnMut(MemoIngredientIndex) -> RunResult<()>>(
        &self,
        _visitor: &mut F,
    ) -> RunResult<()> {
        Ok(())
    }

    fn discard_function_at(
        &self,
        _slot: usize,
        _selected: &PassiveRetirement,
    ) -> Option<IngredientIndex> {
        None
    }
}

impl<'db, I, C, P> PassiveMemoSchema<'db, I> for (PassiveMemo<'db, I, C, P>,)
where
    I: crate::interned::Configuration,
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
}

impl<'db, I, C, P> sealed::Ops<'db, I> for (PassiveMemo<'db, I, C, P>,)
where
    I: crate::interned::Configuration,
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    type Presence = u8;
    const LEN: usize = 1;

    fn empty_presence() -> Self::Presence {
        0
    }

    fn validate(&self, zalsa: &Zalsa, owner: &crate::interned::IngredientImpl<I>) -> RunResult<()> {
        self.0
            .validate_singleton(zalsa, owner)
            .map_err(retirement_error)
    }

    fn inspect(
        &self,
        inspection: PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        self.0.inspect_singleton(inspection, fuel)
    }

    fn validate_members(
        &self,
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
    ) -> RunResult<()> {
        self.0.validate(zalsa, owner).map_err(retirement_error)
    }

    fn inspect_members(
        &self,
        inspection: &mut PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        let quote =
            inspect_passive_memo_bounded::<C, P>(&mut inspection.table, self.0.index, fuel)?;
        Ok(PassiveRetirement {
            present: u8::from(quote.present),
            output_units: quote.output_units,
        })
    }

    fn contains_index(&self, index: MemoIngredientIndex) -> bool {
        self.0.index == index
    }

    fn visit_indices<F: FnMut(MemoIngredientIndex) -> RunResult<()>>(
        &self,
        visitor: &mut F,
    ) -> RunResult<()> {
        visitor(self.0.index)
    }

    fn discard_function_at(
        &self,
        slot: usize,
        selected: &PassiveRetirement,
    ) -> Option<IngredientIndex> {
        (self.0.index.as_usize() == slot && selected.present & 1 != 0).then_some(self.0.memo.index)
    }
}

impl<'db, I, A, PA, B, PB> PassiveMemoSchema<'db, I>
    for (PassiveMemo<'db, I, A, PA>, PassiveMemo<'db, I, B, PB>)
where
    I: crate::interned::Configuration,
    A: Configuration,
    B: Configuration,
    PA: PassiveMemoProfile<A>,
    PB: PassiveMemoProfile<B>,
{
}

impl<'db, I, A, PA, B, PB> sealed::Ops<'db, I>
    for (PassiveMemo<'db, I, A, PA>, PassiveMemo<'db, I, B, PB>)
where
    I: crate::interned::Configuration,
    A: Configuration,
    B: Configuration,
    PA: PassiveMemoProfile<A>,
    PB: PassiveMemoProfile<B>,
{
    type Presence = u8;
    const LEN: usize = 2;

    fn empty_presence() -> Self::Presence {
        0
    }

    fn validate(&self, zalsa: &Zalsa, owner: &crate::interned::IngredientImpl<I>) -> RunResult<()> {
        self.0.validate(zalsa, owner).map_err(retirement_error)?;
        self.1.validate(zalsa, owner).map_err(retirement_error)?;
        if owner.memo_table_types().len() != Self::LEN || self.0.index == self.1.index {
            return Err(retirement_error(KeyRetirementError::Mapping));
        }
        Ok(())
    }

    fn inspect(
        &self,
        mut inspection: PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        if inspection.table.memo_type_count() != Self::LEN {
            return Err(KeyRetirementError::Mapping);
        }
        self.inspect_members(&mut inspection, fuel)
    }

    fn validate_members(
        &self,
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
    ) -> RunResult<()> {
        self.0.validate(zalsa, owner).map_err(retirement_error)?;
        self.1.validate(zalsa, owner).map_err(retirement_error)?;
        if self.0.index == self.1.index {
            return Err(retirement_error(KeyRetirementError::Mapping));
        }
        Ok(())
    }

    fn inspect_members(
        &self,
        inspection: &mut PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement, KeyRetirementError> {
        fuel.consume(1)?;
        let table = &mut inspection.table;
        if self.0.index == self.1.index
            || !table.has_memo_type::<Memo<A>>(self.0.index)
            || !table.has_memo_type::<Memo<B>>(self.1.index)
        {
            return Err(KeyRetirementError::Mapping);
        }
        let first = inspect_passive_memo_bounded::<A, PA>(table, self.0.index, fuel)?;
        let second = inspect_passive_memo_bounded::<B, PB>(table, self.1.index, fuel)?;
        let output_units = first
            .output_units
            .checked_add(second.output_units)
            .ok_or_else(|| KeyRetirementError::WorkOverflow)?;
        Ok(PassiveRetirement {
            present: u8::from(first.present) | (u8::from(second.present) << 1),
            output_units,
        })
    }

    fn contains_index(&self, index: MemoIngredientIndex) -> bool {
        self.0.index == index || self.1.index == index
    }

    fn visit_indices<F: FnMut(MemoIngredientIndex) -> RunResult<()>>(
        &self,
        visitor: &mut F,
    ) -> RunResult<()> {
        visitor(self.0.index)?;
        visitor(self.1.index)
    }

    fn discard_function_at(
        &self,
        slot: usize,
        selected: &PassiveRetirement,
    ) -> Option<IngredientIndex> {
        if self.0.index.as_usize() == slot && selected.present & 1 != 0 {
            Some(self.0.memo.index)
        } else if self.1.index.as_usize() == slot && selected.present & 2 != 0 {
            Some(self.1.memo.index)
        } else {
            None
        }
    }
}

fn check_disjoint<'db, I, L, R>(left: &L, right: &R) -> RunResult<()>
where
    I: crate::interned::Configuration,
    L: SchemaOps<'db, I>,
    R: SchemaOps<'db, I>,
{
    left.visit_indices(&mut |index| {
        if right.contains_index(index) {
            Err(retirement_error(KeyRetirementError::Mapping))
        } else {
            Ok(())
        }
    })
}

impl<'db, I, L, R> PassiveMemoSchema<'db, I> for PassiveMemoGroup<L, R>
where
    I: crate::interned::Configuration,
    L: PassiveMemoSchema<'db, I>,
    R: PassiveMemoSchema<'db, I>,
{
}

impl<'db, I, L, R> sealed::Ops<'db, I> for PassiveMemoGroup<L, R>
where
    I: crate::interned::Configuration,
    L: PassiveMemoSchema<'db, I>,
    R: PassiveMemoSchema<'db, I>,
{
    type Presence = (L::Presence, R::Presence);
    const LEN: usize = L::LEN + R::LEN;

    fn empty_presence() -> Self::Presence {
        (L::empty_presence(), R::empty_presence())
    }

    fn validate_members(
        &self,
        zalsa: &Zalsa,
        owner: &crate::interned::IngredientImpl<I>,
    ) -> RunResult<()> {
        self.left.validate_members(zalsa, owner)?;
        self.right.validate_members(zalsa, owner)?;
        check_disjoint::<I, _, _>(&self.left, &self.right)
    }

    fn inspect_members(
        &self,
        inspection: &mut PassiveMemoInspection<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<PassiveRetirement<Self::Presence>, KeyRetirementError> {
        fuel.consume(1)?;
        let left = self.left.inspect_members(inspection, fuel)?;
        let right = self.right.inspect_members(inspection, fuel)?;
        let output_units = left
            .output_units
            .checked_add(right.output_units)
            .ok_or_else(|| KeyRetirementError::WorkOverflow)?;
        Ok(PassiveRetirement {
            present: (left.present, right.present),
            output_units,
        })
    }

    fn contains_index(&self, index: MemoIngredientIndex) -> bool {
        self.left.contains_index(index) || self.right.contains_index(index)
    }

    fn visit_indices<F: FnMut(MemoIngredientIndex) -> RunResult<()>>(
        &self,
        visitor: &mut F,
    ) -> RunResult<()> {
        self.left.visit_indices(visitor)?;
        self.right.visit_indices(visitor)
    }

    fn discard_function_at(
        &self,
        slot: usize,
        selected: &PassiveRetirement<Self::Presence>,
    ) -> Option<IngredientIndex> {
        // Event selection consults only occupancy; the combined work quote is admitted once.
        let left = PassiveRetirement {
            present: selected.present.0,
            output_units: 0,
        };
        self.left.discard_function_at(slot, &left).or_else(|| {
            let right = PassiveRetirement {
                present: selected.present.1,
                output_units: 0,
            };
            self.right.discard_function_at(slot, &right)
        })
    }
}
