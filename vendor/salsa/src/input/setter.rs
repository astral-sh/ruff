use std::marker::PhantomData;

use crate::input::{Configuration, IngredientImpl};
use crate::{Durability, Runtime};

/// Setter for a field of an input.
pub trait Setter: Sized {
    type FieldTy;

    /// Sets the durability that the field will have after this write.
    ///
    /// Setting the durability to [`Durability::NEVER_CHANGE`] is allowed, but
    /// the field cannot be changed again afterward.
    fn with_durability(self, durability: Durability) -> Self;

    /// Sets the field to `value` and returns its previous value.
    ///
    /// # Panics
    ///
    /// Panics if the field's current durability is [`Durability::NEVER_CHANGE`].
    fn to(self, value: Self::FieldTy) -> Self::FieldTy;
}

#[must_use]
pub struct SetterImpl<'setter, C: Configuration, S, F> {
    runtime: &'setter mut Runtime,
    id: C::Struct,
    ingredient: &'setter mut IngredientImpl<C>,
    durability: Option<Durability>,
    field_index: usize,
    setter: S,
    phantom: PhantomData<fn(F)>,
}

impl<'setter, C, S, F> SetterImpl<'setter, C, S, F>
where
    C: Configuration,
    S: FnOnce(&mut C::Fields, F) -> F,
{
    pub fn new(
        runtime: &'setter mut Runtime,
        id: C::Struct,
        field_index: usize,
        ingredient: &'setter mut IngredientImpl<C>,
        setter: S,
    ) -> Self {
        SetterImpl {
            runtime,
            id,
            field_index,
            ingredient,
            durability: None,
            setter,
            phantom: PhantomData,
        }
    }
}

impl<C, S, F> Setter for SetterImpl<'_, C, S, F>
where
    C: Configuration,
    S: FnOnce(&mut C::Fields, F) -> F,
{
    type FieldTy = F;

    fn with_durability(mut self, durability: Durability) -> Self {
        self.durability = Some(durability);
        self
    }

    fn to(self, value: F) -> F {
        let Self {
            runtime,
            id,
            ingredient,
            durability,
            field_index,
            setter,
            phantom: _,
        } = self;

        ingredient.set_field(runtime, id, field_index, durability, |tuple| {
            setter(tuple, value)
        })
    }
}
