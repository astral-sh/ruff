//! Test helpers for working with Salsa databases

use std::fmt;
use std::marker::PhantomData;

use salsa::id::AsId;
use salsa::ingredient::Ingredient;
use salsa::storage::HasIngredientsFor;

/// Assert that the Salsa query described by the generic parameter `C`
/// was executed at least once with the input `input`
/// in the history span represented by `events`.
pub fn assert_function_query_was_run<'db, C, Db, Jar>(
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
    function_query_was_run(db, to_function, input, events, true);
}

/// Assert that there were no executions with the input `input`
/// of the Salsa query described by the generic parameter `C`
/// in the history span represented by `events`.
pub fn assert_function_query_was_not_run<'db, C, Db, Jar>(
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
    function_query_was_run(db, to_function, input, events, false);
}

fn function_query_was_run<'db, C, Db, Jar>(
    db: &'db Db,
    to_function: impl FnOnce(&C) -> &salsa::function::FunctionIngredient<C>,
    input: &C::Input<'db>,
    events: &[salsa::Event],
    should_have_run: bool,
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

    if should_have_run && !did_run {
        panic!(
            "Expected query {:?} to have run but it didn't",
            DebugIdx {
                db: PhantomData::<Db>,
                value_id: input.as_id(),
                ingredient: function_ingredient,
            }
        );
    } else if !should_have_run && did_run {
        panic!(
            "Expected query {:?} not to have run but it did",
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
