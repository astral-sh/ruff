//! Test helpers for working with Salsa databases

use std::fmt;
use std::marker::PhantomData;

use salsa::id::AsId;
use salsa::ingredient::Ingredient;
use salsa::storage::HasIngredientsFor;

/// Assert that calling `to_function` with `input` as an argument
/// will result in a (re-)execution of the query specified by the generic parameter `C`
pub fn assert_will_run_function_query<'db, C, Db, Jar>(
    db: &'db Db,
    to_function: impl FnOnce(&C) -> &salsa::function::FunctionIngredient<C>,
    input: &C::Input<'db>,
    events: &[salsa::Event],
) where
    C: salsa::function::Configuration<Jar = Jar>
        + salsa::storage::IngredientsFor<Jar = Jar, Ingredients = C>,
    Jar: HasIngredientsFor<C>,
    Db: salsa::DbWithJar<Jar>,
    C::Input<'db>: AsId,
{
    will_run_function_query(db, to_function, input, events, true);
}

/// Assert that calling `to_function` with `input` as an argument
/// will *not* result in a (re-)execution of the query specified by the generic parameter `C`
pub fn assert_will_not_run_function_query<'db, C, Db, Jar>(
    db: &'db Db,
    to_function: impl FnOnce(&C) -> &salsa::function::FunctionIngredient<C>,
    input: &C::Input<'db>,
    events: &[salsa::Event],
) where
    C: salsa::function::Configuration<Jar = Jar>
        + salsa::storage::IngredientsFor<Jar = Jar, Ingredients = C>,
    Jar: HasIngredientsFor<C>,
    Db: salsa::DbWithJar<Jar>,
    C::Input<'db>: AsId,
{
    will_run_function_query(db, to_function, input, events, false);
}

fn will_run_function_query<'db, C, Db, Jar>(
    db: &'db Db,
    to_function: impl FnOnce(&C) -> &salsa::function::FunctionIngredient<C>,
    input: &C::Input<'db>,
    events: &[salsa::Event],
    should_run: bool,
) where
    C: salsa::function::Configuration<Jar = Jar>
        + salsa::storage::IngredientsFor<Jar = Jar, Ingredients = C>,
    Jar: HasIngredientsFor<C>,
    Db: salsa::DbWithJar<Jar>,
    C::Input<'db>: AsId,
{
    let (jar, _) =
        <_ as salsa::storage::HasJar<<C as salsa::storage::IngredientsFor>::Jar>>::jar(db);
    let ingredient = jar.ingredient();

    let function_ingredient = to_function(ingredient);

    let ingredient_index =
        <salsa::function::FunctionIngredient<C> as Ingredient<Db>>::ingredient_index(
            function_ingredient,
        );

    let did_run = events.iter().any(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            database_key.ingredient_index() == ingredient_index
                && database_key.key_index() == input.as_id()
        } else {
            false
        }
    });

    if should_run && !did_run {
        panic!(
            "Expected query {:?} to run but it didn't",
            DebugIdx {
                db: PhantomData::<Db>,
                value_id: input.as_id(),
                ingredient: function_ingredient,
            }
        );
    } else if !should_run && did_run {
        panic!(
            "Expected query {:?} not to run but it did",
            DebugIdx {
                db: PhantomData::<Db>,
                value_id: input.as_id(),
                ingredient: function_ingredient,
            }
        );
    }
}

struct DebugIdx<'a, I, Db>
where
    I: Ingredient<Db>,
{
    value_id: salsa::Id,
    ingredient: &'a I,
    db: PhantomData<Db>,
}

impl<'a, I, Db> fmt::Debug for DebugIdx<'a, I, Db>
where
    I: Ingredient<Db>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        self.ingredient.fmt_index(Some(self.value_id), f)
    }
}
