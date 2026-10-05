use std::any::TypeId;
use std::fmt::{Debug, Formatter};

use super::{ErasedMemo, Memo, SelectedMemo};
use crate::attempt_probe::{self, MemoReuse, QueryPolicy};
use crate::database::AsDynDatabase;
use crate::function::{Configuration, IngredientImpl};
use crate::ingredient::Ingredient;
use crate::prepared_source_probe::Stamp;
use crate::table::memo::MemoSlot;
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseKeyIndex, Id};

/// An existing complete-only memo and its canonical value, certified while the database is idle.
/// Tracked outputs remain owned by their original producer throughout admitted reads.
pub struct PreparedSourceMemo<'db, T> {
    db: &'db dyn Database,
    ingredient: &'db dyn Ingredient,
    configuration: TypeId,
    id: Id,
    stamp: Stamp,
    slot: MemoSlot<'db>,
    memo: ErasedMemo<'db>,
    value: &'db T,
}

impl<T> Clone for PreparedSourceMemo<'_, T> {
    fn clone(&self) -> Self {
        Self {
            db: self.db,
            ingredient: self.ingredient,
            configuration: self.configuration,
            id: self.id,
            stamp: self.stamp,
            slot: self.slot,
            memo: self.memo,
            value: self.value,
        }
    }
}

impl<T> Debug for PreparedSourceMemo<'_, T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedSourceMemo")
            .field("key", &self.database_key())
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparedSourceError {
    ActiveAttempt,
    ActiveQuery,
    ActiveOperation,
    ForeignIngredient,
    UnsupportedPolicy,
    MissingMemo,
    MissingValue,
    StaleStamp,
    UnverifiedMemo,
    ProvisionalMemo,
    IncompleteMemo,
    ReplacedMemo,
}

impl PreparedSourceError {
    pub(in crate::function) fn message(self) -> &'static str {
        match self {
            Self::ActiveAttempt => "prepared source certification has an active attempt",
            Self::ActiveQuery => {
                "prepared source certification has an active or borrowed query stack"
            }
            Self::ActiveOperation => "prepared source certification has an active operation",
            Self::ForeignIngredient => "prepared source has a foreign database or ingredient",
            Self::UnsupportedPolicy => "prepared source requires an explicitly complete-only query",
            Self::MissingMemo => "prepared source memo is missing",
            Self::MissingValue => "prepared source memo has no value",
            Self::StaleStamp => "prepared source database stamp changed",
            Self::UnverifiedMemo => "prepared source memo is not verified in the current revision",
            Self::ProvisionalMemo => "prepared source memo is provisional",
            Self::IncompleteMemo => "prepared source memo has incomplete attempt support",
            Self::ReplacedMemo => "prepared source memo was replaced",
        }
    }
}

impl<'db, T> PreparedSourceMemo<'db, T> {
    /// Checks the idle boundary before generated key or ingredient discovery.
    #[doc(hidden)]
    pub fn check_idle(db: &dyn Database) -> Result<(), PreparedSourceError> {
        attempt_probe::check_structural_access(db)
            .map_err(|_| PreparedSourceError::ActiveAttempt)?;
        if attempt_probe::current().is_some() {
            return Err(PreparedSourceError::ActiveAttempt);
        }
        if db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.is_empty())
            != Some(true)
        {
            return Err(PreparedSourceError::ActiveQuery);
        }
        if attempt_probe::stack_depths() != (0, 0) {
            return Err(PreparedSourceError::ActiveOperation);
        }
        db.unwind_if_revision_cancelled();
        Ok(())
    }

    /// Certifies the existing memo for the declaring query's generated key without fetching it.
    #[doc(hidden)]
    pub fn certify<C>(
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
        id: Id,
    ) -> Result<Self, PreparedSourceError>
    where
        C: Configuration<Output<'db> = T>,
    {
        Self::check_idle(db.as_dyn_database())?;
        if C::ATTEMPT_POLICY != QueryPolicy::CompleteOnly {
            return Err(PreparedSourceError::UnsupportedPolicy);
        }
        if !db
            .zalsa()
            .ingredients()
            .nth(ingredient.index.as_u32() as usize)
            .is_some_and(|known| std::ptr::addr_eq(known, ingredient as &dyn Ingredient))
            || ingredient.view_caster.get().is_none()
        {
            return Err(PreparedSourceError::ForeignIngredient);
        }
        let stamp = Stamp::current(db.as_dyn_database());
        let memo_index = ingredient.memo_ingredient_index(db.zalsa(), id);
        let slot = ingredient.memo_slot(db.zalsa(), id, memo_index);
        let memo = slot.get_erased().ok_or(PreparedSourceError::MissingMemo)?;
        if memo.type_id != TypeId::of::<Memo<C>>() {
            return Err(PreparedSourceError::ForeignIngredient);
        }
        let value = memo
            .downcast::<C>()
            .value()
            .ok_or(PreparedSourceError::MissingValue)?;
        let result = Self {
            db: db.as_dyn_database(),
            ingredient,
            configuration: TypeId::of::<C>(),
            id,
            stamp,
            slot,
            memo,
            value,
        };
        result.check_current()?;
        result.check_publication()?;
        Ok(result)
    }

    pub fn database_key(&self) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.ingredient.ingredient_index(), self.id)
    }

    /// Checks that the certified memo is still current while idle, preserving native revision cancellation.
    pub fn check_current(&self) -> Result<(), PreparedSourceError> {
        Self::check_idle(self.db)?;
        self.check_current_inner()
    }

    /// Borrows the certified value for idle preparation and catalog consistency checks.
    pub fn value(&self) -> Result<&'db T, PreparedSourceError> {
        self.check_current()?;
        Ok(self.value)
    }

    pub(in crate::function) fn belongs_to(&self, db: &dyn Database) -> bool {
        std::ptr::eq(self.db.zalsa(), db.zalsa())
            && std::ptr::eq(self.db.zalsa_local(), db.zalsa_local())
    }

    pub(in crate::function) fn check_current_inner(&self) -> Result<(), PreparedSourceError> {
        // At the certified stamp, publication facts cannot be revoked on this allocation:
        // a published final memo cannot become provisional, and changing the value or explicit
        // incomplete support requires exclusive access excluded by the certificate's shared borrow.
        // A later publication can still replace it, so reads must check the stamp and slot.
        if !self.stamp.belongs_to(self.db) {
            return Err(PreparedSourceError::StaleStamp);
        }
        let current = self
            .slot
            .get_erased()
            .ok_or(PreparedSourceError::MissingMemo)?;
        if current.data != self.memo.data || current.type_id != self.memo.type_id {
            return Err(PreparedSourceError::ReplacedMemo);
        }
        Ok(())
    }

    /// Establishes the allocation's publication facts before returning a new certificate.
    fn check_publication(&self) -> Result<(), PreparedSourceError> {
        let header = self.memo.header();
        if header.verified_at.load() != self.db.zalsa().current_revision() {
            return Err(PreparedSourceError::UnverifiedMemo);
        }
        if header.may_be_provisional() {
            return Err(PreparedSourceError::ProvisionalMemo);
        }
        if header.attempt_reuse(self.db.zalsa()) != MemoReuse::Ordinary {
            return Err(PreparedSourceError::IncompleteMemo);
        }
        if !self.memo.has_value() {
            return Err(PreparedSourceError::MissingValue);
        }
        Ok(())
    }

    pub(in crate::function) fn recover<C>(
        &self,
    ) -> Result<
        (
            &'db C::DbView,
            &'db IngredientImpl<C>,
            Id,
            SelectedMemo<'db, C>,
        ),
        PreparedSourceError,
    >
    where
        C: Configuration<Output<'db> = T>,
    {
        self.check_current_inner()?;
        if self.configuration != TypeId::of::<C>()
            || self.ingredient.type_id() != TypeId::of::<IngredientImpl<C>>()
            || self.memo.type_id != TypeId::of::<Memo<C>>()
        {
            return Err(PreparedSourceError::ForeignIngredient);
        }
        // SAFETY: The retained ingredient's concrete type was checked above. Certification
        // paired it with this exact database and initialized its database view caster.
        let ingredient = unsafe { self.ingredient.assert_type_unchecked::<IngredientImpl<C>>() };
        let caster = ingredient
            .view_caster
            .get()
            .ok_or(PreparedSourceError::ForeignIngredient)?;
        // SAFETY: Certification checked ingredient membership in the retained database.
        let db = unsafe { caster.downcast_unchecked(self.db.into()) };
        let selected = SelectedMemo::new(self.memo.downcast::<C>())
            .ok_or(PreparedSourceError::MissingValue)?;
        Ok((db, ingredient, self.id, selected))
    }
}
