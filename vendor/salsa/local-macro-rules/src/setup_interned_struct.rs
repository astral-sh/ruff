/// Macro for setting up a function that must intern its arguments.
#[macro_export]
macro_rules! setup_interned_struct {
    (
        // Attributes on the struct
        attrs: [$(#[$attr:meta]),*],

        // Visibility of the struct
        vis: $vis:vis,

        // Name of the struct
        Struct: $Struct:ident,

        // Name of the struct data. This is a parameter because `std::concat_idents`
        // is unstable and taking an additional dependency is unnecessary.
        StructData: $StructDataIdent:ident,

        // Name of the struct type with a `'static` argument (unless this type has no db lifetime,
        // in which case this is the same as `$Struct`)
        StructWithStatic: $StructWithStatic:ty,

        // Name of the `'db` lifetime that the user gave
        db_lt: $db_lt:lifetime,

        // optional db lifetime argument.
        db_lt_arg: $($db_lt_arg:lifetime)?,

        // the salsa ID
        id: $Id:path,

        // The minimum number of revisions to keep the value interned.
        revisions: $($revisions:expr)?,

        // the lifetime used in the desugared interned struct.
        // if the `db_lt_arg`, is present, this is `db_lt_arg`, but otherwise,
        // it is `'static`.
        interior_lt: $interior_lt:lifetime,

        // Name user gave for `new`
        new_fn: $new_fn:ident,

        // Opt-in method for borrowing the stored field types without return-mode callbacks.
        field_view_fn: $($field_view_fn:ident)?,

        field_requests_fn: $($field_requests_fn:ident)?,

        // A series of option tuples; see `setup_tracked_struct` macro
        field_options: [$($field_option:tt),*],

        // Field names
        field_ids: [$($field_id:ident),*],

        // Names for field setter methods (typically `set_foo`)
        field_getters: [$($field_getter_vis:vis $field_getter_id:ident),*],

        // Field types
        field_tys: [$($field_ty:ty),*],

        // Indices for each field from 0..N -- must be unsuffixed (e.g., `0`, `1`).
        field_indices: [$($field_index:tt),*],

        // Indexed types for each field (T0, T1, ...)
        field_indexed_tys: [$($indexed_ty:ident),*],

        // Attrs for each field.
        field_attrs: [$([$(#[$field_attr:meta]),*]),*],

        // Number of fields
        num_fields: $N:literal,

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

        // Ensures that every field can be rebound to the database lifetime.
        assert_fields_are_salsa_values: {$($assert_fields_are_salsa_values:tt)*},

        // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
        // We have the procedural macro generate names for those items that are
        // not used elsewhere in the user's code.
        unused_names: [
            $zalsa:ident,
            $zalsa_struct:ident,
            $Configuration:ident,
            $CACHE:ident,
            $Db:ident,
            $FieldView:ident,
            $view_fields:ident,
            $view_value:ident,
            $view_tuple:ident,
            $FieldRequests:ident,
            $request_context:ident,
            $request_value:ident,
        ]
    ) => {
        $(#[$attr])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash)]
        $vis struct $Struct< $($db_lt_arg)? >(
            $Id,
            std::marker::PhantomData<fn() -> &$interior_lt ()>
        );

        #[allow(clippy::all)]
        #[allow(dead_code)]
        const _: () = {
            use ::salsa::plumbing as $zalsa;
            use $zalsa::interned as $zalsa_struct;

            type $Configuration = $StructWithStatic;

            impl<$($db_lt_arg)?> $zalsa::HasJar for $Struct<$($db_lt_arg)?> {
                type Jar = $zalsa_struct::JarImpl<$Configuration>;
                const KIND: $zalsa::JarKind = $zalsa::JarKind::Struct;
            }

            $zalsa::register_jar! {
                $zalsa::ErasedJar::erase::<$StructWithStatic>()
            }

            #[allow(unused_lifetimes)]
            fn _assert_fields_are_salsa_values<$db_lt>() {
                use $zalsa::{SalsaValueDispatch, SalsaValueFallback as _};
                $($assert_fields_are_salsa_values)*
            }
            let _ = _assert_fields_are_salsa_values;

            type $StructDataIdent<$db_lt> = ($($field_ty,)*);

            /// Key to use during hash lookups. Each field is some type that implements `Lookup<T>`
            /// for the owned type. This permits interning with an `&str` when a `String` is required and so forth.
            #[derive(Hash)]
            struct StructKey<$db_lt, $($indexed_ty),*>(
                $($indexed_ty,)*
                ::std::marker::PhantomData<&$db_lt ()>,
            );

            impl<$db_lt, $($indexed_ty,)*> $zalsa::HashEqLike<StructKey<$db_lt, $($indexed_ty),*>>
                for $StructDataIdent<$db_lt>
                where
                $($field_ty: $zalsa::HashEqLike<$indexed_ty>),*
                {

                fn hash<H: ::std::hash::Hasher>(&self, h: &mut H) {
                    $($zalsa::HashEqLike::<$indexed_ty>::hash(&self.$field_index, &mut *h);)*
                }

                fn eq(&self, data: &StructKey<$db_lt, $($indexed_ty),*>) -> bool {
                    ($($zalsa::HashEqLike::<$indexed_ty>::eq(&self.$field_index, &data.$field_index) && )* true)
                }
            }

            impl<$db_lt, $($indexed_ty: $zalsa::Lookup<$field_ty>),*> $zalsa::Lookup<$StructDataIdent<$db_lt>>
                for StructKey<$db_lt, $($indexed_ty),*> {

                #[allow(unused_unit)]
                fn into_owned(self) -> $StructDataIdent<$db_lt> {
                    ($($zalsa::Lookup::into_owned(self.$field_index),)*)
                }
            }

            // SAFETY: The generated assertions above prove every field can be
            // retained after erasing the database lifetime.
            unsafe impl $zalsa::interned::Configuration for $StructWithStatic {
                const LOCATION: $zalsa::Location = $zalsa::Location {
                    file: file!(),
                    line: line!(),
                };
                const DEBUG_NAME: &'static str = stringify!($Struct);
                const PERSIST: bool = $persist;

                $(
                    const REVISIONS: ::core::num::NonZeroUsize = ::core::num::NonZeroUsize::new($revisions).unwrap();
                )?

                type Fields<'a> = $StructDataIdent<'a>;
                type Struct<'db> = $Struct< $($db_lt_arg)? >;

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

            static $CACHE: $zalsa::IngredientCache<$zalsa_struct::IngredientImpl<$Configuration>> =
                $zalsa::IngredientCache::new();

            impl $Configuration {
                pub fn ingredient(zalsa: &$zalsa::Zalsa) -> &$zalsa_struct::IngredientImpl<Self> {
                    // SAFETY: The ingredient at offset 0 in `JarImpl<$Configuration>` has type
                    // `IngredientImpl<$Configuration>`.
                    unsafe {
                        $CACHE.get_or_create::<$zalsa_struct::JarImpl<$Configuration>, 0>(zalsa)
                    }
                }
            }

            impl< $($db_lt_arg)? > $zalsa::AsId for $Struct< $($db_lt_arg)? > {
                fn as_id(&self) -> ::salsa::Id {
                    self.0.as_id()
                }
            }

            impl< $($db_lt_arg)? > $zalsa::FromId for $Struct< $($db_lt_arg)? > {
                fn from_id(id: ::salsa::Id) -> Self {
                    Self(<$Id>::from_id(id), ::std::marker::PhantomData)
                }
            }

            unsafe impl< $($db_lt_arg)? > Send for $Struct< $($db_lt_arg)? > {}

            unsafe impl< $($db_lt_arg)? > Sync for $Struct< $($db_lt_arg)? > {}

            $zalsa::macro_if! { $generate_debug_impl =>
                impl< $($db_lt_arg)? > ::std::fmt::Debug for $Struct< $($db_lt_arg)? > {
                    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        Self::default_debug_fmt(*self, f)
                    }
                }
            }

            impl< $($db_lt_arg)? > $zalsa::SalsaStructInDb for $Struct< $($db_lt_arg)? > {
                type MemoIngredientMap = $zalsa::MemoIngredientSingletonIndex;
                const LEAF_TYPE_IDS: &'static [$zalsa::ConstTypeId] = &[$zalsa::ConstTypeId::of::<$Struct>()];

                fn lookup_ingredient_index(aux: &$zalsa::Zalsa) -> $zalsa::IngredientIndices {
                    aux.lookup_jar_by_type::<$zalsa_struct::JarImpl<$Configuration>>().into()
                }

                fn entries(
                    zalsa: &$zalsa::Zalsa
                ) -> impl Iterator<Item = $zalsa::DatabaseKeyIndex> + '_ {
                    let ingredient_index = zalsa.lookup_jar_by_type::<$zalsa_struct::JarImpl<$Configuration>>();
                    <$Configuration>::ingredient(zalsa).entries(zalsa).map(|entry| entry.key())
                }

                #[inline]
                fn cast(id: $zalsa::Id, type_id: $zalsa::TypeId) -> $zalsa::Option<Self> {
                    if type_id == $zalsa::TypeId::of::<$Struct>() {
                        $zalsa::Some(<$Struct as $zalsa::FromId>::from_id(id))
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

            $zalsa::macro_if! { $persist =>
                impl<$($db_lt_arg)?> $zalsa::serde::Serialize for $Struct<$($db_lt_arg)?> {
                    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                    where
                        S: $zalsa::serde::Serializer,
                    {
                        $zalsa::serde::Serialize::serialize(&$zalsa::AsId::as_id(self), serializer)
                    }
                }

                impl<'de, $($db_lt_arg)?> $zalsa::serde::Deserialize<'de> for $Struct<$($db_lt_arg)?> {
                    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                    where
                        D: $zalsa::serde::Deserializer<'de>,
                    {
                        let id = $zalsa::Id::deserialize(deserializer)?;
                        Ok($zalsa::FromId::from_id(id))
                    }
                }
            }


            unsafe impl< $($db_lt_arg)? > $zalsa::SalsaValue for $Struct< $($db_lt_arg)? > {}

            impl<$db_lt> $Struct< $($db_lt_arg)? >  {
                pub fn $new_fn<$Db, $($indexed_ty: $zalsa::Lookup<$field_ty> + ::std::hash::Hash,)*>(db: &$db_lt $Db,  $($field_id: $indexed_ty),*) -> Self
                where
                    // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                    $Db: ?Sized + ::salsa::Database,
                    $(
                        $field_ty: $zalsa::HashEqLike<$indexed_ty>,
                    )*
                {
                    $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("interned constructor ", stringify!($Struct)));
                    let (zalsa, zalsa_local) = db.zalsas();
                    $Configuration::ingredient(zalsa).intern(zalsa, zalsa_local,
                        StructKey::<$db_lt>($($field_id,)* ::std::marker::PhantomData::default()), |_, data| $zalsa::Lookup::into_owned(data))
                }

                $(
                    $(#[$field_attr])*
                    $field_getter_vis fn $field_getter_id<$Db>(self, db: &'db $Db) -> $zalsa::return_mode_ty!($field_option, 'db, $field_ty)
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + $zalsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("interned field ", stringify!($Struct), "::", stringify!($field_getter_id)));
                        let zalsa = db.zalsa();
                        let fields = $Configuration::ingredient(zalsa).fields(zalsa, self);
                        $zalsa::return_mode_expression!(
                            $field_option,
                            $field_ty,
                            &fields.$field_index,
                        )
                    }
                )*
            }

            $zalsa::macro_if! {
                iftt ($($field_view_fn)?) {
                    #[doc(hidden)]
                    pub struct $FieldView<$db_lt> {
                        $view_fields: ::salsa::FieldReads<$db_lt>,
                        $view_value: $Struct<$($db_lt_arg)?>,
                    }

                    impl<$db_lt> $Struct<$($db_lt_arg)?> {
                        /// Borrow the stored field types without ordinary getter conversions.
                        #[inline]
                        $vis fn $($field_view_fn)?(self, $view_fields: ::salsa::FieldReads<$db_lt>) -> $FieldView<$db_lt> {
                            $FieldView { $view_fields, $view_value: self }
                        }
                    }

                    impl<$db_lt> $FieldView<$db_lt> {
                        $(
                            $(#[$field_attr])*
                            #[inline]
                            $field_getter_vis fn $field_getter_id(&self) -> &$db_lt $field_ty {
                                let $view_tuple = $zalsa::interned_fields::<$Configuration>(
                                    self.$view_fields, self.$view_value, &$CACHE,
                                );
                                &$view_tuple.$field_index
                            }
                        )*
                    }
                } else {}
            }

            $zalsa::macro_if! {
                iftt ($($field_requests_fn)?) {
                    #[doc(hidden)]
                    pub struct $FieldRequests<$db_lt> {
                        $request_context: $zalsa::FieldRequestContext<$db_lt>,
                        $request_value: $Struct<$($db_lt_arg)?>,
                    }

                    impl<$db_lt> $Struct<$($db_lt_arg)?> {
                        /// Creates field requests without reading or converting stored values.
                        #[inline]
                        $vis fn $($field_requests_fn)?(
                            self,
                            context: impl ::std::convert::Into<$zalsa::FieldRequestContext<$db_lt>>,
                        ) -> $FieldRequests<$db_lt> {
                            $FieldRequests { $request_context: context.into(), $request_value: self }
                        }
                    }

                    impl<$db_lt> $FieldRequests<$db_lt> {
                        $(
                            $(#[$field_attr])*
                            #[inline]
                            $field_getter_vis fn $field_getter_id(&self) -> impl $zalsa::FieldRequest<
                                $db_lt,
                                Stored = $field_ty,
                                Output = $zalsa::return_mode_ty!($field_option, $db_lt, $field_ty),
                            > + $db_lt + use<$db_lt> {
                                $zalsa::InternedFieldRequest::<
                                    $Configuration,
                                    $field_ty,
                                    $zalsa::return_mode_ty!($field_option, $db_lt, $field_ty),
                                >::new(
                                    self.$request_context,
                                    self.$request_value,
                                    |fields| &fields.$field_index,
                                    |stored| $zalsa::return_mode_expression!($field_option, $field_ty, stored,),
                                    $zalsa::return_mode_field_mode!($field_option),
                                )
                            }
                        )*
                    }
                } else {}
            }

            // Duplication can be dropped here once we no longer allow the `no_lifetime` hack
            $zalsa::macro_if! {
                iftt ($($db_lt_arg)?) {
                    impl $Struct<'_> {
                        /// Default debug formatting for this struct (may be useful if you define your own `Debug` impl)
                        pub fn default_debug_fmt(this: Self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result
                        where
                            // rustc rejects trivial bounds, but it cannot see through higher-ranked bounds
                            // with its check :^)
                            $(for<$db_lt> $field_ty: ::std::fmt::Debug),*
                        {
                            $zalsa::with_attached_database(|db| {
                                $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("interned debug fields ", stringify!($Struct)));
                                let zalsa = db.zalsa();
                                let fields = $Configuration::ingredient(zalsa).fields(zalsa, this);
                                let mut f = f.debug_struct(stringify!($Struct));
                                $(
                                    let f = f.field(stringify!($field_id), &fields.$field_index);
                                )*
                                f.finish()
                            }).unwrap_or_else(|| {
                                f.debug_tuple(stringify!($Struct))
                                    .field(&$zalsa::AsId::as_id(&this))
                                    .finish()
                            })
                        }
                    }
                } else {
                    impl $Struct {
                        /// Default debug formatting for this struct (may be useful if you define your own `Debug` impl)
                        pub fn default_debug_fmt(this: Self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result
                        where
                            // rustc rejects trivial bounds, but it cannot see through higher-ranked bounds
                            // with its check :^)
                            $(for<$db_lt> $field_ty: ::std::fmt::Debug),*
                        {
                            $zalsa::with_attached_database(|db| {
                                $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("interned debug fields ", stringify!($Struct)));
                                let zalsa = db.zalsa();
                                let fields = $Configuration::ingredient(zalsa).fields(zalsa, this);
                                let mut f = f.debug_struct(stringify!($Struct));
                                $(
                                    let f = f.field(stringify!($field_id), &fields.$field_index);
                                )*
                                f.finish()
                            }).unwrap_or_else(|| {
                                f.debug_tuple(stringify!($Struct))
                                    .field(&$zalsa::AsId::as_id(&this))
                                    .finish()
                            })
                        }
                    }
                }
            }
        };
    };
}
