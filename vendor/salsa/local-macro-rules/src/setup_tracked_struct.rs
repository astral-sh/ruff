/// Macro for setting up a function that must intern its arguments.
#[macro_export]
macro_rules! setup_tracked_struct {
    (
        // Attributes on the function.
        attrs: [$(#[$attr:meta]),*],

        // Visibility of the struct.
        vis: $vis:vis,

        // Name of the struct.
        Struct: $Struct:ident,

        // Name of the `'db` lifetime that the user gave.
        db_lt: $db_lt:lifetime,

        // Name user gave for `new`.
        new_fn: $new_fn:ident,

        // Optional method that creates typed field requests.
        field_requests_fn: $($field_requests_fn:ident)?,

        // Field names.
        field_ids: [$($field_id:ident),*],

        // Tracked field names.
        tracked_ids: [$($tracked_id:ident),*],

        // Visibility and names of tracked fields.
        tracked_getters: [$($tracked_getter_vis:vis $tracked_getter_id:ident),*],

        // Visibility and names of untracked fields.
        untracked_getters: [$($untracked_getter_vis:vis $untracked_getter_id:ident),*],

        // Field types, may reference `db_lt`.
        field_tys: [$($field_ty:ty),*],

        assert_fields_are_salsa_values: {$($assert_fields_are_salsa_values:tt)*},

        // Tracked field types.
        tracked_tys: [$($tracked_ty:ty),*],

        // Untracked field types.
        untracked_tys: [$($untracked_ty:ty),*],

        // Indices for each field from 0..N -- must be unsuffixed (e.g., `0`, `1`).
        field_indices: [$($field_index:tt),*],

        // Absolute indices of any tracked fields, relative to all other fields of this struct.
        absolute_tracked_indices: [$($absolute_tracked_index:tt),*],

        // Indices of any tracked fields, relative to only tracked fields on this struct.
        relative_tracked_indices: [$($relative_tracked_index:tt),*],

        // Absolute indices of any untracked fields.
        absolute_untracked_indices: [$($absolute_untracked_index:tt),*],

        // Equality functions for tracked fields.
        tracked_field_equalities: [$($tracked_field_equality:tt),*],

        // Equality functions for untracked fields.
        untracked_field_equalities: [$($untracked_field_equality:tt),*],

        // A set of "field options" for each tracked field.
        //
        // Each field option is a tuple `(return_mode, maybe_default)` where:
        //
        // * `return_mode` is an identifier as specified in `salsa_macros::options::Option::returns`
        // * `maybe_default` is either the identifier `default` or `required`
        //
        // These are used to drive conditional logic for each field via recursive macro invocation
        // (see e.g. @return_mode below).
        tracked_options: [$($tracked_option:tt),*],

        // A set of "field options" for each untracked field.
        // (see docs for `tracked_options`).
        untracked_options: [$($untracked_option:tt),*],

        // Attrs for each field.
        tracked_field_attrs: [$([$(#[$tracked_field_attr:meta]),*]),*],
        untracked_field_attrs: [$([$(#[$untracked_field_attr:meta]),*]),*],

        // Number of tracked fields.
        num_tracked_fields: $N:literal,

        // If true, generate a debug impl.
        generate_debug_impl: $generate_debug_impl:tt,

        // The function used to implement `C::heap_size`.
        heap_size_fn: $($heap_size_fn:path)?,

        // If `true`, `serialize_fn` and `deserialize_fn` have been provided.
        persist: $persist:tt,

        // The path to the `serialize` function for the value's fields.
        serialize_fn: $($serialize_fn:path)?,

        // The path to the `serialize` function for the value's fields.
        deserialize_fn: $($deserialize_fn:path)?,

        // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
        // We have the procedural macro generate names for those items that are
        // not used elsewhere in the user's code.
        unused_names: [
            $zalsa:ident,
            $zalsa_struct:ident,
            $Configuration:ident,
            $CACHE:ident,
            $Db:ident,
            $Revision:ident,
            $FieldRequests:ident,
            $request_zalsas:ident,
            $request_value:ident,
        ]
    ) => {
        $(#[$attr])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash)]
        $vis struct $Struct<$db_lt>(
            ::salsa::Id,
            ::std::marker::PhantomData<fn() -> &$db_lt ()>
        );

        #[allow(dead_code)]
        #[allow(clippy::all)]
        const _: () = {
            use ::salsa::plumbing as $zalsa;
            use $zalsa::tracked_struct as $zalsa_struct;
            use $zalsa::Revision as $Revision;

            type $Configuration = $Struct<'static>;

            #[allow(unused_lifetimes)]
            fn _assert_fields_are_salsa_values<$db_lt>() {
                use $zalsa::{SalsaValueDispatch, SalsaValueFallback as _};
                $($assert_fields_are_salsa_values)*
            }
            let _ = _assert_fields_are_salsa_values;

            impl<$db_lt> $zalsa::HasJar for $Struct<$db_lt> {
                type Jar = $zalsa_struct::JarImpl<$Configuration>;
                const KIND: $zalsa::JarKind = $zalsa::JarKind::Struct;
            }

            $zalsa::register_jar! {
                $zalsa::ErasedJar::erase::<$Struct<'static>>()
            }

            // SAFETY: The generated assertions above prove every field can be
            // retained after erasing the database lifetime.
            unsafe impl $zalsa_struct::Configuration for $Configuration {
                const LOCATION: $zalsa::Location = $zalsa::Location {
                    file: file!(),
                    line: line!(),
                };
                const DEBUG_NAME: &'static str = stringify!($Struct);

                const TRACKED_FIELD_NAMES: &'static [&'static str] = &[
                    $(stringify!($tracked_id),)*
                ];

                const TRACKED_FIELD_INDICES: &'static [usize] = &[
                    $($relative_tracked_index,)*
                ];

                const PERSIST: bool = $persist;

                type Fields<$db_lt> = ($($field_ty,)*);

                type Revisions = [$zalsa::AtomicRevision; $N];

                type Struct<$db_lt> = $Struct<$db_lt>;

                fn untracked_fields(fields: &Self::Fields<'_>) -> impl ::std::hash::Hash {
                    ( $( &fields.$absolute_untracked_index ),* )
                }

                fn new_revisions(current_revision: $Revision) -> Self::Revisions {
                    std::array::from_fn(|_| $zalsa::AtomicRevision::new(current_revision))
                }

                fn update_fields<$db_lt>(
                    current_revision: $Revision,
                    revisions: &Self::Revisions,
                    old_fields: &mut Self::Fields<$db_lt>,
                    new_fields: Self::Fields<$db_lt>,
                ) -> bool {
                    $(
                        if $zalsa::update_field(
                            &mut old_fields.$absolute_tracked_index,
                            new_fields.$absolute_tracked_index,
                            $tracked_field_equality,
                        ) {
                            revisions[$relative_tracked_index].store(current_revision);
                        }
                    )*;

                    // If any identity field has changed, return `true`, indicating that the tracked
                    // struct itself should be considered changed.
                    $(
                        $zalsa::update_field(
                            &mut old_fields.$absolute_untracked_index,
                            new_fields.$absolute_untracked_index,
                            $untracked_field_equality,
                        )
                        |
                    )* false
                }

                $(
                    fn heap_size(value: &Self::Fields<'_>) -> Option<usize> {
                        Some($heap_size_fn(value))
                    }
                )?

                fn serialize<S: $zalsa::serde::Serializer>(
                    fields: &Self::Fields<'_>,
                    serializer: S,
                ) -> ::std::result::Result<S::Ok, S::Error> {
                    $zalsa::macro_if! {
                        if $persist {
                            $($serialize_fn(fields, serializer))?
                        } else {
                            panic!("attempted to serialize value not marked with `persist` attribute")
                        }
                    }
                }

                fn deserialize<'de, D: $zalsa::serde::Deserializer<'de>>(
                    deserializer: D,
                ) -> ::std::result::Result<Self::Fields<'static>, D::Error> {
                    $zalsa::macro_if! {
                        if $persist {
                            $($deserialize_fn(deserializer))?
                        } else {
                            panic!("attempted to deserialize value not marked with `persist` attribute")
                        }
                    }
                }
            }

            impl $Configuration {
                pub fn ingredient(db: &dyn $zalsa::Database) -> &$zalsa_struct::IngredientImpl<Self> {
                    Self::ingredient_(db.zalsa())
                }

                fn ingredient_(zalsa: &$zalsa::Zalsa) -> &$zalsa_struct::IngredientImpl<Self> {
                    static CACHE: $zalsa::IngredientCache<$zalsa_struct::IngredientImpl<$Configuration>> =
                        $zalsa::IngredientCache::new();

                    // SAFETY: The ingredient at offset 0 in `JarImpl<$Configuration>` has type
                    // `IngredientImpl<$Configuration>`.
                    unsafe {
                        CACHE.get_or_create::<$zalsa_struct::JarImpl<$Configuration>, 0>(zalsa)
                    }
                }
            }

            impl<$db_lt> $zalsa::FromId for $Struct<$db_lt> {
                #[inline]
                fn from_id(id: ::salsa::Id) -> Self {
                    $Struct(id, ::std::marker::PhantomData)
                }
            }

            impl $zalsa::AsId for $Struct<'_> {
                #[inline]
                fn as_id(&self) -> $zalsa::Id {
                    self.0
                }
            }

            impl $zalsa::SalsaStructInDb for $Struct<'_> {
                type MemoIngredientMap = $zalsa::MemoIngredientSingletonIndex;
                const LEAF_TYPE_IDS: &'static [$zalsa::ConstTypeId] = &[$zalsa::ConstTypeId::of::<$Struct<'static>>()];

                fn lookup_ingredient_index(aux: &$zalsa::Zalsa) -> $zalsa::IngredientIndices {
                    aux.lookup_jar_by_type::<$zalsa_struct::JarImpl<$Configuration>>().into()
                }

                fn entries(
                    zalsa: &$zalsa::Zalsa
                ) -> impl Iterator<Item = $zalsa::DatabaseKeyIndex> + '_ {
                    let ingredient_index = zalsa.lookup_jar_by_type::<$zalsa_struct::JarImpl<$Configuration>>();
                    <$Configuration>::ingredient_(zalsa).entries(zalsa).map(|entry| entry.key())
                }

                #[inline]
                fn cast(id: $zalsa::Id, type_id: $zalsa::TypeId) -> $zalsa::Option<Self> {
                    if type_id == $zalsa::TypeId::of::<$Struct<'static>>() {
                        $zalsa::Some(<$Struct<'static> as $zalsa::FromId>::from_id(id))
                    } else {
                        $zalsa::None
                    }
                }

                #[inline]
                unsafe fn memo_table(
                    zalsa: &$zalsa::Zalsa,
                    id: $zalsa::Id,
                    current_revision: $zalsa::Revision,
                ) -> $zalsa::MemoTableWithTypes<'_> {
                    // SAFETY: Guaranteed by caller.
                    unsafe { zalsa.table().memos::<$zalsa_struct::Value<$Configuration>>(id, current_revision) }
                }
            }

            impl $zalsa::TrackedStructInDb for $Struct<'_> {
                fn database_key_index(zalsa: &$zalsa::Zalsa, id: $zalsa::Id) -> $zalsa::DatabaseKeyIndex {
                    $Configuration::ingredient_(zalsa).database_key_index(id)
                }
            }

            $zalsa::macro_if! { $persist =>
                impl $zalsa::serde::Serialize for $Struct<'_> {
                    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                    where
                        S: $zalsa::serde::Serializer,
                    {
                        $zalsa::serde::Serialize::serialize(&$zalsa::AsId::as_id(self), serializer)
                    }
                }

                impl<'de> $zalsa::serde::Deserialize<'de> for $Struct<'_> {
                    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                    where
                        D: $zalsa::serde::Deserializer<'de>,
                    {
                        let id = $zalsa::Id::deserialize(deserializer)?;
                        Ok($zalsa::FromId::from_id(id))
                    }
                }
            }


            unsafe impl Send for $Struct<'_> {}

            unsafe impl Sync for $Struct<'_> {}

            $zalsa::macro_if! { $generate_debug_impl =>
                impl ::std::fmt::Debug for $Struct<'_> {
                    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        Self::default_debug_fmt(*self, f)
                    }
                }
            }

            unsafe impl $zalsa::SalsaValue for $Struct<'_> {}

            $zalsa::macro_if! {
                iftt ($($field_requests_fn)?) {
                    #[doc(hidden)]
                    pub struct $FieldRequests<$db_lt> {
                        $request_zalsas: $zalsa::FieldRequestContext<$db_lt>,
                        $request_value: $Struct<$db_lt>,
                    }

                    impl<$db_lt> $Struct<$db_lt> {
                        /// Creates field requests without reading or converting stored values.
                        #[inline]
                        $vis fn $($field_requests_fn)?(
                            self,
                            context: impl ::std::convert::Into<$zalsa::FieldRequestContext<$db_lt>>,
                        ) -> $FieldRequests<$db_lt> {
                            $FieldRequests { $request_zalsas: context.into(), $request_value: self }
                        }
                    }

                    impl<$db_lt> $FieldRequests<$db_lt> {
                        $(
                            $(#[$tracked_field_attr])*
                            #[inline]
                            $tracked_getter_vis fn $tracked_getter_id(&self) -> impl $zalsa::FieldRequest<
                                $db_lt,
                                Stored = $tracked_ty,
                                Output = $zalsa::return_mode_ty!($tracked_option, $db_lt, $tracked_ty),
                            > + $db_lt + use<$db_lt> {
                                $zalsa::TrackedFieldRequest::<
                                    $Configuration,
                                    $tracked_ty,
                                    $zalsa::return_mode_ty!($tracked_option, $db_lt, $tracked_ty),
                                >::new(
                                    self.$request_zalsas,
                                    self.$request_value,
                                    $zalsa::Some($relative_tracked_index),
                                    |fields| &fields.$absolute_tracked_index,
                                    |stored| $zalsa::return_mode_expression!($tracked_option, $tracked_ty, stored,),
                                    $zalsa::return_mode_field_mode!($tracked_option),
                                )
                            }
                        )*

                        $(
                            $(#[$untracked_field_attr])*
                            #[inline]
                            $untracked_getter_vis fn $untracked_getter_id(&self) -> impl $zalsa::FieldRequest<
                                $db_lt,
                                Stored = $untracked_ty,
                                Output = $zalsa::return_mode_ty!($untracked_option, $db_lt, $untracked_ty),
                            > + $db_lt + use<$db_lt> {
                                $zalsa::TrackedFieldRequest::<
                                    $Configuration,
                                    $untracked_ty,
                                    $zalsa::return_mode_ty!($untracked_option, $db_lt, $untracked_ty),
                                >::new(
                                    self.$request_zalsas,
                                    self.$request_value,
                                    $zalsa::None,
                                    |fields| &fields.$absolute_untracked_index,
                                    |stored| $zalsa::return_mode_expression!($untracked_option, $untracked_ty, stored,),
                                    $zalsa::return_mode_field_mode!($untracked_option),
                                )
                            }
                        )*
                    }
                } else {}
            }

            impl<$db_lt> $Struct<$db_lt> {
                pub fn $new_fn<$Db>(db: &$db_lt $Db, $($field_id: $field_ty),*) -> Self
                where
                    // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                    $Db: ?Sized + $zalsa::Database,
                {
                    $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("tracked constructor ", stringify!($Struct)));
                    let (zalsa, zalsa_local) = db.zalsas();
                    $Configuration::ingredient_(zalsa).new_struct(
                        zalsa,zalsa_local,
                        ($($field_id,)*)
                    )
                }

                $(
                    $(#[$tracked_field_attr])*
                    $tracked_getter_vis fn $tracked_getter_id<$Db>(self, db: &$db_lt $Db) -> $crate::return_mode_ty!($tracked_option, $db_lt, $tracked_ty)
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + $zalsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("tracked field ", stringify!($Struct), "::", stringify!($tracked_getter_id)));
                        let (zalsa, zalsa_local) = db.zalsas();
                        let fields = $Configuration::ingredient_(zalsa).tracked_field(zalsa, zalsa_local, self, $relative_tracked_index);
                        $crate::return_mode_expression!(
                            $tracked_option,
                            $tracked_ty,
                            &fields.$absolute_tracked_index,
                        )
                    }
                )*

                $(
                    $(#[$untracked_field_attr])*
                    $untracked_getter_vis fn $untracked_getter_id<$Db>(self, db: &$db_lt $Db) -> $crate::return_mode_ty!($untracked_option, $db_lt, $untracked_ty)
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + $zalsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("tracked identity field ", stringify!($Struct), "::", stringify!($untracked_getter_id)));
                        let zalsa = db.zalsa();
                        let fields = $Configuration::ingredient_(zalsa).untracked_field(zalsa, self);
                        $crate::return_mode_expression!(
                            $untracked_option,
                            $untracked_ty,
                            &fields.$absolute_untracked_index,
                        )
                    }
                )*
            }

            #[allow(unused_lifetimes)]
            impl<'_db> $Struct<'_db> {
                /// Default debug formatting for this struct (may be useful if you define your own `Debug` impl)
                pub fn default_debug_fmt(this: Self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result
                where
                    // `zalsa::with_attached_database` has a local lifetime for the database
                    // so we need this function to be higher-ranked over the db lifetime
                    // Thus the actual lifetime of `Self` does not matter here so we discard
                    // it with the `'_db` lifetime name as we cannot shadow lifetimes.
                    $(for<$db_lt> $field_ty: ::std::fmt::Debug),*
                {
                    $zalsa::with_attached_database(|db| {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("tracked debug fields ", stringify!($Struct)));
                        let zalsa = db.zalsa();
                        let fields = $Configuration::ingredient_(zalsa).leak_fields(zalsa, this);
                        let mut f = f.debug_struct(stringify!($Struct));
                        let f = f.field("[salsa id]", &$zalsa::AsId::as_id(&this));
                        $(
                            let f = f.field(stringify!($field_id), &fields.$field_index);
                        )*
                        f.finish()
                    }).unwrap_or_else(|| {
                        f.debug_struct(stringify!($Struct))
                            .field("[salsa id]", &$zalsa::AsId::as_id(&this))
                            .finish()
                    })
                }
            }
        };
    };
}
