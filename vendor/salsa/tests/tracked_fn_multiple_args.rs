#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on multiple salsa struct args
//! compiles and executes successfully.

use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, RegistryBuilder, Route, RunResult,
};
use salsa::plumbing::function::{Configuration, IngredientImpl, InternedQueryConfiguration};
use salsa::plumbing::interned::{
    Configuration as InternedConfiguration, IngredientImpl as ArgumentIngredient,
};
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::{Database, Id};

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::interned]
struct MyInterned<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, input: MyInput, interned: MyInterned<'db>) -> u32 {
    input.field(db) + interned.field(db)
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn scalar(_db: &dyn Database, value: u32, _unit: ()) -> u32 {
    value + 1
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn constant(_db: &dyn Database) -> u32 {
    7
}

fn input_to_fields<'db, C: InternedQueryConfiguration>(
    input: <C as Configuration>::Input<'db>,
) -> <C as InternedConfiguration>::Fields<'db> {
    input
}

fn fields_to_input<'db, C: InternedQueryConfiguration>(
    fields: <C as InternedConfiguration>::Fields<'db>,
) -> <C as Configuration>::Input<'db> {
    fields
}

fn salsa_struct_to_struct<'db, C: InternedQueryConfiguration>(
    value: <C as Configuration>::SalsaStruct<'db>,
) -> <C as InternedConfiguration>::Struct<'db> {
    value
}

fn struct_to_salsa_struct<'db, C: InternedQueryConfiguration>(
    value: <C as InternedConfiguration>::Struct<'db>,
) -> <C as Configuration>::SalsaStruct<'db> {
    value
}

fn lifetime_ingredient(
    db: &dyn Database,
) -> &IngredientImpl<
    impl InternedQueryConfiguration
    + for<'a> Configuration<
        DbView = dyn Database,
        Input<'a> = (MyInput, MyInterned<'a>),
        Output<'a> = u32,
    >,
> {
    tracked_fn::fn_ingredient_(db, db.zalsa())
}

fn scalar_ingredient(
    db: &dyn Database,
) -> &IngredientImpl<
    impl InternedQueryConfiguration
    + for<'a> Configuration<DbView = dyn Database, Input<'a> = (u32, ()), Output<'a> = u32>,
> {
    scalar::fn_ingredient_(db, db.zalsa())
}

fn constant_ingredient(
    db: &dyn Database,
) -> &IngredientImpl<
    impl InternedQueryConfiguration
    + for<'a> Configuration<DbView = dyn Database, Input<'a> = (), Output<'a> = u32>,
> {
    constant::fn_ingredient_(db, db.zalsa())
}

struct Admit;

impl ExecutionAdmission for Admit {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn same_route<C: Configuration>(route: &Route<'_, C>, ingredient: &IngredientImpl<C>, id: Id) {
    assert_eq!(route.database_key(id), ingredient.database_key_index(id));
}

fn exercise_bridge<'db, C>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    input: <C as Configuration>::Input<'db>,
    ordinary: impl FnOnce() -> u32,
) -> (
    &'db ArgumentIngredient<C>,
    Id,
    <C as Configuration>::Input<'db>,
)
where
    C: InternedQueryConfiguration + for<'a> Configuration<DbView = dyn Database, Output<'a> = u32>,
{
    let arguments = C::argument_ingredient(db.zalsa());
    let input = fields_to_input::<C>(input_to_fields::<C>(input));
    // This is ordinary key construction before the attempt, not admitted interning.
    let id = arguments.intern_id(
        db.zalsa(),
        db.zalsa_local(),
        input_to_fields::<C>(input),
        |_, fields| fields,
    );
    let value = arguments.intern(
        db.zalsa(),
        db.zalsa_local(),
        input_to_fields::<C>(C::id_to_input(db.zalsa(), id)),
        |_, fields| fields,
    );
    let value = salsa_struct_to_struct::<C>(struct_to_salsa_struct::<C>(value));
    assert_eq!(value.as_id(), id);
    let before = ingredient.fetch(db, db.zalsa(), db.zalsa_local(), id);
    assert_eq!(*before, ordinary());
    let after = ingredient.fetch(db, db.zalsa(), db.zalsa_local(), id);
    assert!(std::ptr::eq(before, after));
    let stored = arguments
        .entries(db.zalsa())
        .map(|entry| entry.key())
        .collect::<Vec<_>>();
    assert_eq!(stored, [arguments.database_key_index(id)]);

    let outcome = try_with_attempt(db, 1_000, || -> RunResult<()> {
        let mut registry = RegistryBuilder::new(db, &Admit)?;
        let route = registry.reserve(db, ingredient)?;
        same_route(&route, ingredient, id);
        let query_key = route.database_key(id);
        let argument_key = arguments.database_key_index(id);
        assert_eq!(query_key.key_index(), argument_key.key_index());
        assert_ne!(
            query_key.ingredient_index(),
            argument_key.ingredient_index()
        );
        Ok(())
    });
    assert!(
        matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
        "{outcome:?}"
    );
    (arguments, id, C::id_to_input(db.zalsa(), id))
}

#[test]
fn generated_query_bridge_preserves_types_keys_and_routes() {
    let db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let interned = MyInterned::new(&db, 33);
    let (arguments, id, decoded) =
        exercise_bridge(&db, lifetime_ingredient(&db), (input, interned), || {
            tracked_fn(&db, input, interned)
        });
    assert!(std::ptr::addr_eq(
        arguments,
        tracked_fn::intern_ingredient_(db.zalsa())
    ));
    assert!(decoded == (input, interned));
    assert_eq!(
        id,
        tracked_fn::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (input, interned),
            |_, fields| fields
        )
    );

    let (arguments, id, decoded) = exercise_bridge(&db, scalar_ingredient(&db), (41, ()), || {
        scalar(&db, 41, ())
    });
    assert!(std::ptr::addr_eq(
        arguments,
        scalar::intern_ingredient_(db.zalsa())
    ));
    assert_eq!(decoded, (41, ()));
    assert_eq!(
        id,
        scalar::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (41, ()),
            |_, fields| fields
        )
    );

    let (arguments, id, decoded) =
        exercise_bridge(&db, constant_ingredient(&db), (), || constant(&db));
    assert!(std::ptr::addr_eq(
        arguments,
        constant::intern_ingredient_(db.zalsa())
    ));
    assert_eq!(decoded, ());
    assert_eq!(
        id,
        constant::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (),
            |_, fields| fields
        )
    );
}

#[test]
fn execute() {
    let db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let interned = MyInterned::new(&db, 33);
    assert_eq!(tracked_fn(&db, input, interned), 55);
}


#[path = "compile-pass/tracked-configuration.rs"]
mod configuration_names;

#[test]
fn named_configurations_preserve_ingredients_and_visibility() {
    configuration_names::main();
}
