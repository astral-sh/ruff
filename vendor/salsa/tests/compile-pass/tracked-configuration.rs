//! Named configurations retain the query and argument ingredients across visibility boundaries.

use salsa::Database;
use salsa::plumbing::function::{Configuration, IngredientImpl, InternedQueryConfiguration};
use salsa::plumbing::interned::{
    Configuration as InternedConfiguration, IngredientImpl as ArgumentIngredient,
};
use salsa::plumbing::{AsId, HasJar, ZalsaDatabase};

fn same_configuration<C: Configuration>(left: &IngredientImpl<C>, right: &IngredientImpl<C>) {
    assert!(std::ptr::eq(left, right));
}

/// Checks that named configurations reuse the original argument interner and query memos.
fn interned_identity<'db, C>(
    db: &'db dyn Database,
    query: &'db IngredientImpl<C>,
    arguments: &'db ArgumentIngredient<C>,
    input: <C as Configuration>::Input<'db>,
    ordinary: impl FnOnce() -> u32,
) where
    C: InternedQueryConfiguration + for<'a> Configuration<DbView = dyn Database, Output<'a> = u32>,
{
    assert!(std::ptr::eq(arguments, C::argument_ingredient(db.zalsa())));
    let fields: <C as InternedConfiguration>::Fields<'db> = input;
    let value = arguments.intern(db.zalsa(), db.zalsa_local(), fields, |_, fields| fields);
    let salsa_value: <C as Configuration>::SalsaStruct<'db> = value;
    let interned_value: <C as InternedConfiguration>::Struct<'db> = salsa_value;
    let id = interned_value.as_id();
    let before = query.fetch(db, db.zalsa(), db.zalsa_local(), id);
    assert_eq!(*before, ordinary());
    assert!(std::ptr::eq(
        before,
        query.fetch(db, db.zalsa(), db.zalsa_local(), id)
    ));
    assert_eq!(query.database_key_index(id).key_index(), id);
}

const fn has_original_jar<T: HasJar<Jar = T>>() {}

mod private {
    use super::*;

    #[salsa::input]
    struct Input {
        #[returns(copy)]
        value: u32,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Output(u32);

    #[salsa::tracked(returns(copy), configuration = (QueryConfiguration))]
    fn query(db: &dyn Database, input: Input) -> Output {
        Output(input.value(db))
    }

    #[salsa::tracked(
        returns(copy),
        configuration = (CycleConfiguration),
        cycle_initial = |_, _, _| 0,
        cycle_fn = |_, _, _, value, _| value,
    )]
    fn cyclic(db: &dyn Database, input: Input) -> u32 {
        cyclic(db, input).saturating_add(1).min(2)
    }

    pub(super) fn check(db: &dyn Database) {
        let named: &IngredientImpl<QueryConfiguration> = query::fn_ingredient_(db, db.zalsa());
        same_configuration(named, query::fn_ingredient_(db, db.zalsa()));
        let input = Input::new(db, 13);
        assert_eq!(query(db, input), Output(13));
        assert_eq!(
            *named.fetch(db, db.zalsa(), db.zalsa_local(), input.as_id()),
            Output(13)
        );
        let cycle: &IngredientImpl<CycleConfiguration> = cyclic::fn_ingredient_(db, db.zalsa());
        same_configuration(cycle, cyclic::fn_ingredient_(db, db.zalsa()));
        assert_eq!(cyclic(db, input), 2);
        assert_eq!(
            *cycle.fetch(db, db.zalsa(), db.zalsa_local(), input.as_id()),
            2
        );
    }
}

mod restricted {
    use super::*;

    #[salsa::input]
    pub(super) struct Input {
        #[returns(copy)]
        value: u32,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) struct Output(u32);

    #[salsa::tracked(returns(copy), configuration = (pub(super) QueryConfiguration))]
    fn query(db: &dyn Database, input: Input) -> Output {
        Output(input.value(db))
    }

    pub(super) fn named(db: &dyn Database) -> &IngredientImpl<QueryConfiguration> {
        query::fn_ingredient_(db, db.zalsa())
    }

    pub(super) fn check(db: &dyn Database) {
        same_configuration(named(db), query::fn_ingredient_(db, db.zalsa()));
        assert_eq!(query(db, Input::new(db, 17)), Output(17));
    }
}

pub mod public {
    use super::*;

    #[salsa::input]
    pub struct Input {
        #[returns(copy)]
        pub value: u32,
    }

    #[salsa::interned]
    pub struct Interned<'db> {
        #[returns(copy)]
        pub value: u32,
    }

    #[salsa::tracked(returns(copy), configuration = (pub SingleConfiguration))]
    pub fn single(db: &dyn Database, input: Input) -> u32 {
        input.value(db)
    }

    #[salsa::tracked(returns(copy), configuration = (pub PairConfiguration))]
    fn pair<'db>(db: &'db dyn Database, input: Input, other: Interned<'db>) -> u32 {
        input.value(db) + other.value(db)
    }

    #[salsa::tracked(returns(copy), configuration = (NarrowConfiguration))]
    pub fn narrow_alias(_db: &dyn Database, value: u32, _unit: ()) -> u32 {
        value + 1
    }

    #[salsa::tracked(returns(copy), configuration = (ConstantConfiguration))]
    fn constant(_db: &dyn Database) -> u32 {
        29
    }

    pub fn named_single(db: &dyn Database) -> &IngredientImpl<SingleConfiguration> {
        single::fn_ingredient_(db, db.zalsa())
    }

    pub fn named_pair(db: &dyn Database) -> &IngredientImpl<PairConfiguration> {
        pair::fn_ingredient_(db, db.zalsa())
    }

    pub fn pair_arguments(db: &dyn Database) -> &ArgumentIngredient<PairConfiguration> {
        pair::intern_ingredient_(db.zalsa())
    }

    pub fn ordinary_pair<'db>(db: &'db dyn Database, input: Input, other: Interned<'db>) -> u32 {
        pair(db, input, other)
    }

    pub fn check(db: &dyn Database) {
        same_configuration(named_single(db), single::fn_ingredient_(db, db.zalsa()));
        same_configuration(named_pair(db), pair::fn_ingredient_(db, db.zalsa()));
        let narrow: &IngredientImpl<NarrowConfiguration> =
            narrow_alias::fn_ingredient_(db, db.zalsa());
        same_configuration(narrow, narrow_alias::fn_ingredient_(db, db.zalsa()));
        interned_identity::<NarrowConfiguration>(
            db,
            narrow,
            narrow_alias::intern_ingredient_(db.zalsa()),
            (40, ()),
            || narrow_alias(db, 40, ()),
        );
        let constant_ingredient: &IngredientImpl<ConstantConfiguration> =
            constant::fn_ingredient_(db, db.zalsa());
        same_configuration(
            constant_ingredient,
            constant::fn_ingredient_(db, db.zalsa()),
        );
        interned_identity::<ConstantConfiguration>(
            db,
            constant_ingredient,
            constant::intern_ingredient_(db.zalsa()),
            (),
            || constant(db),
        );
    }
}

mod consumer {
    use super::*;

    pub(super) fn check(db: &dyn Database) {
        let single: &IngredientImpl<public::SingleConfiguration> = public::named_single(db);
        let pair: &IngredientImpl<public::PairConfiguration> = public::named_pair(db);
        let _: &IngredientImpl<restricted::QueryConfiguration> = restricted::named(db);
        let input = public::Input::new(db, 23);
        let other = public::Interned::new(db, 31);
        assert_eq!(public::single(db, input), 23);
        assert_eq!(
            *single.fetch(db, db.zalsa(), db.zalsa_local(), input.as_id()),
            23
        );
        interned_identity::<public::PairConfiguration>(
            db,
            pair,
            public::pair_arguments(db),
            (input, other),
            || public::ordinary_pair(db, input, other),
        );
        // The alias is private, but callers still have the public function and marker.
        //
        assert_eq!(public::narrow_alias(db, 40, ()), 41);
        has_original_jar::<public::narrow_alias>();
        has_original_jar::<public::pair>();
    }
}

pub fn main() {
    let db = salsa::DatabaseImpl::new();
    private::check(&db);
    restricted::check(&db);
    public::check(&db);
    consumer::check(&db);
}
