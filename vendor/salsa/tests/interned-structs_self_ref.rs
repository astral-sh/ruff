#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.

use std::any::TypeId;
use std::convert::identity;

use salsa::plumbing::Zalsa;
use test_log::test;

#[test]
fn interning_returns_equal_keys_for_equal_data() {
    let db = salsa::DatabaseImpl::new();
    let s1 = InternedString::new(&db, "Hello, ".to_string(), identity);
    let s2 = InternedString::new(&db, "World, ".to_string(), |_| s1);
    let s1_2 = InternedString::new(&db, "Hello, ", identity);
    let s2_2 = InternedString::new(&db, "World, ", |_| s2);
    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
}
// Recursive expansion of interned macro
// #[salsa::interned]
// struct InternedString<'db> {
//     data: String,
//     other: InternedString<'db>,
// }
// ======================================

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
struct InternedString<'db>(
    salsa::Id,
    std::marker::PhantomData<&'db salsa::plumbing::interned::Value<InternedString<'static>>>,
);

#[allow(warnings)]
const _: () = {
    use salsa::plumbing as zalsa_;
    use zalsa_::interned as zalsa_struct_;

    type Configuration_ = InternedString<'static>;

    impl<'db> zalsa_::HasJar for InternedString<'db> {
        type Jar = zalsa_struct_::JarImpl<Configuration_>;
        const KIND: zalsa_::JarKind = zalsa_::JarKind::Struct;
    }

    zalsa_::register_jar! {
        zalsa_::ErasedJar::erase::<InternedString<'static>>()
    }

    #[derive(Clone, salsa::SalsaValue)]
    struct StructData<'db>(String, InternedString<'db>);

    impl<'db> Eq for StructData<'db> {}
    impl<'db> PartialEq for StructData<'db> {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }

    impl<'db> std::hash::Hash for StructData<'db> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    #[doc = r" Key to use during hash lookups. Each field is some type that implements `Lookup<T>`"]
    #[doc = r" for the owned type. This permits interning with an `&str` when a `String` is required and so forth."]
    #[derive(Hash)]
    struct StructKey<'db, T0>(T0, std::marker::PhantomData<&'db ()>);

    impl<'db, T0> zalsa_::HashEqLike<StructKey<'db, T0>> for StructData<'db>
    where
        String: zalsa_::HashEqLike<T0>,
    {
        fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
            zalsa_::HashEqLike::<T0>::hash(&self.0, &mut *h);
        }
        fn eq(&self, data: &StructKey<'db, T0>) -> bool {
            (zalsa_::HashEqLike::<T0>::eq(&self.0, &data.0) && true)
        }
    }
    // SAFETY: `StructData<'db>` contains only an owned `String` and a phantom lifetime.
    unsafe impl zalsa_struct_::Configuration for Configuration_ {
        const LOCATION: zalsa_::Location = zalsa_::Location {
            file: file!(),
            line: line!(),
        };
        const DEBUG_NAME: &'static str = "InternedString";
        type Fields<'a> = StructData<'a>;
        type Struct<'a> = InternedString<'a>;

        const PERSIST: bool = false;

        fn serialize<S>(value: &Self::Fields<'_>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: zalsa_::serde::Serializer,
        {
            panic!("attempted to serialize value not marked with `persist` attribute")
        }

        fn deserialize<'de, D>(deserializer: D) -> Result<Self::Fields<'static>, D::Error>
        where
            D: zalsa_::serde::Deserializer<'de>,
        {
            panic!("attempted to deserialize value not marked with `persist` attribute")
        }
    }
    impl Configuration_ {
        pub fn ingredient(zalsa: &zalsa_::Zalsa) -> &zalsa_struct_::IngredientImpl<Self> {
            static CACHE: zalsa_::IngredientCache<zalsa_struct_::IngredientImpl<Configuration_>> =
                zalsa_::IngredientCache::new();

            // SAFETY: The ingredient at offset 0 in `JarImpl<Configuration_>` has type
            // `IngredientImpl<Configuration_>`.
            unsafe { CACHE.get_or_create::<zalsa_struct_::JarImpl<Configuration_>, 0>(zalsa) }
        }
    }
    impl zalsa_::AsId for InternedString<'_> {
        fn as_id(&self) -> salsa::Id {
            self.0
        }
    }
    impl zalsa_::FromId for InternedString<'_> {
        fn from_id(id: salsa::Id) -> Self {
            Self(id, std::marker::PhantomData)
        }
    }
    unsafe impl Send for InternedString<'_> {}

    unsafe impl Sync for InternedString<'_> {}

    impl std::fmt::Debug for InternedString<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Self::default_debug_fmt(*self, f)
        }
    }
    impl zalsa_::SalsaStructInDb for InternedString<'_> {
        type MemoIngredientMap = zalsa_::MemoIngredientSingletonIndex;

        const LEAF_TYPE_IDS: &'static [salsa::plumbing::ConstTypeId] =
            &[salsa::plumbing::ConstTypeId::of::<InternedString>()];

        fn lookup_ingredient_index(aux: &Zalsa) -> salsa::plumbing::IngredientIndices {
            aux.lookup_jar_by_type::<zalsa_struct_::JarImpl<Configuration_>>()
                .into()
        }

        fn entries(zalsa: &zalsa_::Zalsa) -> impl Iterator<Item = zalsa_::DatabaseKeyIndex> + '_ {
            let ingredient_index =
                zalsa.lookup_jar_by_type::<zalsa_struct_::JarImpl<Configuration_>>();
            <Configuration_>::ingredient(zalsa)
                .entries(zalsa)
                .map(|entry| entry.key())
        }

        #[inline]
        fn cast(id: zalsa_::Id, type_id: TypeId) -> Option<Self> {
            if type_id == TypeId::of::<InternedString>() {
                Some(<InternedString as zalsa_::FromId>::from_id(id))
            } else {
                None
            }
        }

        #[inline]
        unsafe fn memo_table(
            zalsa: &zalsa_::Zalsa,
            id: zalsa_::Id,
            current_revision: zalsa_::Revision,
        ) -> zalsa_::MemoTableWithTypes<'_> {
            // SAFETY: Guaranteed by caller.
            unsafe {
                zalsa
                    .table()
                    .memos::<zalsa_struct_::Value<Configuration_>>(id, current_revision)
            }
        }
    }

    unsafe impl zalsa_::SalsaValue for InternedString<'_> {}
    impl<'db> InternedString<'db> {
        pub fn new<Db_, T0: zalsa_::Lookup<String> + std::hash::Hash>(
            db: &'db Db_,
            data: T0,
            other: impl FnOnce(InternedString<'db>) -> InternedString<'db>,
        ) -> Self
        where
            Db_: ?Sized + salsa::Database,
            String: zalsa_::HashEqLike<T0>,
        {
            Configuration_::ingredient(db.zalsa()).intern(
                db.zalsa(),
                db.zalsa_local(),
                StructKey::<'db>(data, std::marker::PhantomData::default()),
                |id, data| {
                    StructData(
                        zalsa_::Lookup::into_owned(data.0),
                        other(zalsa_::FromId::from_id(id)),
                    )
                },
            )
        }
        fn data<Db_>(self, db: &'db Db_) -> String
        where
            Db_: ?Sized + zalsa_::Database,
        {
            let fields = Configuration_::ingredient(db.zalsa()).fields(db.zalsa(), self);
            std::clone::Clone::clone((&fields.0))
        }
        fn other<Db_>(self, db: &'db Db_) -> InternedString<'db>
        where
            Db_: ?Sized + zalsa_::Database,
        {
            let fields = Configuration_::ingredient(db.zalsa()).fields(db.zalsa(), self);
            std::clone::Clone::clone((&fields.1))
        }
        #[doc = r" Default debug formatting for this struct (may be useful if you define your own `Debug` impl)"]
        pub fn default_debug_fmt(this: Self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            zalsa_::with_attached_database(|db| {
                let fields = Configuration_::ingredient(db.zalsa()).fields(db.zalsa(), this);
                let mut f = f.debug_struct("InternedString");
                let f = f.field("data", &fields.0);
                let f = f.field("other", &fields.1);
                f.finish()
            })
            .unwrap_or_else(|| {
                f.debug_tuple("InternedString")
                    .field(&zalsa_::AsId::as_id(&this))
                    .finish()
            })
        }
    }
};
