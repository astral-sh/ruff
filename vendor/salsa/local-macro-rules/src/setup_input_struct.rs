/// Macro for setting up a function that must intern its arguments.
#[macro_export]
macro_rules! setup_input_struct {
    (
        // Attributes on the struct
        attrs: [$(#[$attr:meta]),*],

        // Visibility of the struct
        vis: $vis:vis,

        // Name of the struct
        Struct: $Struct:ident,

        // Name user gave for `new`
        new_fn: $new_fn:ident,

        // Optional method that creates typed field requests.
        field_requests_fn: $($field_requests_fn:ident)?,

        // A series of option tuples; see `setup_tracked_struct` macro
        field_options: [$($field_option:tt),*],

        // Field names
        field_ids: [$($field_id:ident),*],

        // Names for field getter methods (typically `foo`)
        field_getters: [$($field_getter_vis:vis $field_getter_id:ident),*],

        // Names for field setter methods (typically `set_foo`)
        field_setters: [$($field_setter_vis:vis $field_setter_id:ident),*],

        // Field types
        field_tys: [$($field_ty:ty),*],

        // Indices for each field from 0..N -- must be unsuffixed (e.g., `0`, `1`).
        field_indices: [$($field_index:tt),*],

        // Attributes for each field
        field_attrs: [$([$(#[$field_attr:meta]),*]),*],

        // Fields that are required (have no default value). Each item is the fields name and type.
        required_fields: [$($required_field_id:ident $required_field_ty:ty),*],

        // Names for the field durability methods on the builder (typically `foo_durability`)
        field_durability_ids: [$($field_durability_id:ident),*],

        // Number of fields
        num_fields: $N:literal,

        // If true, this is a singleton input.
        is_singleton: $is_singleton:tt,

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
            $Builder:ident,
            $CACHE:ident,
            $Db:ident,
            $FieldRequests:ident,
            $request_zalsas:ident,
            $request_value:ident,
        ]
    ) => {
        $(#[$attr])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash)]
        $vis struct $Struct(salsa::Id);

        #[allow(clippy::all)]
        #[allow(dead_code)]
        const _: () = {
            use ::salsa::plumbing as $zalsa;
            use $zalsa::input as $zalsa_struct;

            type $Configuration = $Struct;

            impl $zalsa::HasJar for $Struct {
                type Jar = $zalsa_struct::JarImpl<$Configuration>;
                const KIND: $zalsa::JarKind = $zalsa::JarKind::Struct;
            }

            $zalsa::register_jar! {
                $zalsa::ErasedJar::erase::<$Struct>()
            }

            impl $zalsa_struct::Configuration for $Configuration {
                const LOCATION: $zalsa::Location = $zalsa::Location {
                    file: file!(),
                    line: line!(),
                };
                const DEBUG_NAME: &'static str = stringify!($Struct);
                const FIELD_DEBUG_NAMES: &'static [&'static str] = &[$(stringify!($field_id)),*];

                const PERSIST: bool = $persist;

                type Singleton = $zalsa::macro_if! {if $is_singleton {$zalsa::input::Singleton} else {$zalsa::input::NotSingleton}};

                type Struct = $Struct;

                type Fields = ($($field_ty,)*);

                type Revisions = [$zalsa::Revision; $N];
                type Durabilities = [$zalsa::Durability; $N];

                $(
                    fn heap_size(value: &Self::Fields) -> Option<usize> {
                        Some($heap_size_fn(value))
                    }
                )?

                fn serialize<S: $zalsa::serde::Serializer>(
                    fields: &Self::Fields,
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
                ) -> ::std::result::Result<Self::Fields, D::Error> {
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

                pub fn ingredient_mut(zalsa_mut: &mut $zalsa::Zalsa) -> (&mut $zalsa_struct::IngredientImpl<Self>, &mut $zalsa::Runtime) {
                    zalsa_mut.new_revision();
                    let index = zalsa_mut.lookup_jar_by_type::<$zalsa_struct::JarImpl<$Configuration>>();
                    let (ingredient, runtime) = zalsa_mut.lookup_ingredient_mut(index);
                    let ingredient = ingredient.assert_type_mut::<$zalsa_struct::IngredientImpl<Self>>();
                    (ingredient, runtime)
                }
            }

            impl $zalsa::FromId for $Struct {
                fn from_id(id: salsa::Id) -> Self {
                    Self(id)
                }
            }

            impl $zalsa::AsId for $Struct {
                fn as_id(&self) -> salsa::Id {
                    self.0
                }
            }

            unsafe impl $zalsa::SalsaValue for $Struct {}

            $zalsa::macro_if! {
                iftt ($($field_requests_fn)?) {
                    #[doc(hidden)]
                    pub struct $FieldRequests<'db> {
                        $request_zalsas: $zalsa::FieldRequestContext<'db>,
                        $request_value: $Struct,
                    }

                    impl $Struct {
                        /// Creates field requests without reading or converting stored values.
                        #[inline]
                        $vis fn $($field_requests_fn)?<'db>(
                            self,
                            context: impl ::std::convert::Into<$zalsa::FieldRequestContext<'db>>,
                        ) -> $FieldRequests<'db> {
                            $FieldRequests { $request_zalsas: context.into(), $request_value: self }
                        }
                    }

                    impl<'db> $FieldRequests<'db> {
                        $(
                            $(#[$field_attr])*
                            #[inline]
                            $field_getter_vis fn $field_getter_id(&self) -> impl $zalsa::FieldRequest<
                                'db,
                                Stored = $field_ty,
                                Output = $zalsa::return_mode_ty!($field_option, 'db, $field_ty),
                            > + 'db + use<'db> {
                                $zalsa::InputFieldRequest::<
                                    $Configuration,
                                    $field_ty,
                                    $zalsa::return_mode_ty!($field_option, 'db, $field_ty),
                                >::new(
                                    self.$request_zalsas,
                                    self.$request_value,
                                    $field_index,
                                    |fields| &fields.$field_index,
                                    |stored| $zalsa::return_mode_expression!($field_option, $field_ty, stored,),
                                    $zalsa::return_mode_field_mode!($field_option),
                                )
                            }
                        )*
                    }
                } else {}
            }

            $zalsa::macro_if! { $generate_debug_impl =>
                impl ::std::fmt::Debug for $Struct {
                    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        Self::default_debug_fmt(*self, f)
                    }
                }
            }

            impl $zalsa::SalsaStructInDb for $Struct {
                type MemoIngredientMap = $zalsa::MemoIngredientSingletonIndex;
                const LEAF_TYPE_IDS: &'static [$zalsa::ConstTypeId] = &[$zalsa::ConstTypeId::of::<$Struct>()];

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
                    if type_id == $zalsa::TypeId::of::<$Struct>() {
                        $zalsa::Some($Struct(id))
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
                impl $zalsa::serde::Serialize for $Struct {
                    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                    where
                        S: $zalsa::serde::Serializer,
                    {
                        $zalsa::serde::Serialize::serialize(&$zalsa::AsId::as_id(self), serializer)
                    }
                }

                impl<'de> $zalsa::serde::Deserialize<'de> for $Struct {
                    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                    where
                        D: $zalsa::serde::Deserializer<'de>,
                    {
                        let id = $zalsa::Id::deserialize(deserializer)?;
                        Ok($zalsa::FromId::from_id(id))
                    }
                }
            }
            impl $Struct {
                #[inline]
                pub fn $new_fn<$Db>(db: &$Db, $($required_field_id: $required_field_ty),*) -> Self
                where
                    // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                    $Db: ?Sized + salsa::Database,
                {
                    $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("input constructor ", stringify!($Struct)));
                    Self::builder($($required_field_id,)*).new(db)
                }

                pub fn builder($($required_field_id: $required_field_ty),*) -> <Self as $zalsa_struct::HasBuilder>::Builder
                {
                    builder::new_builder($($zalsa::maybe_default!($field_option, $field_ty, $field_id,)),*)
                }

                $(
                    $(#[$field_attr])*
                    $field_getter_vis fn $field_getter_id<'db, $Db>(self, db: &'db $Db) -> $zalsa::return_mode_ty!($field_option, 'db, $field_ty)
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + $zalsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("input field ", stringify!($Struct), "::", stringify!($field_getter_id)));
                        let (zalsa, zalsa_local) = db.zalsas();
                        let fields = $Configuration::ingredient_(zalsa).field(
                            zalsa,
                            zalsa_local,
                            self,
                            $field_index,
                        );
                        $zalsa::return_mode_expression!(
                            $field_option,
                            $field_ty,
                            &fields.$field_index,
                        )
                    }
                )*

                $(
                    #[must_use]
                    $field_setter_vis fn $field_setter_id<'db, $Db>(self, db: &'db mut $Db) -> impl salsa::Setter<FieldTy = $field_ty>
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + $zalsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("input setter ", stringify!($Struct), "::", stringify!($field_setter_id)));
                        let zalsa = db.zalsa_mut();
                        let (ingredient, revision) = $Configuration::ingredient_mut(zalsa);
                        $zalsa::input::SetterImpl::new(
                            revision,
                            self,
                            $field_index,
                            ingredient,
                            |fields, f| ::std::mem::replace(&mut fields.$field_index, f),
                        )
                    }
                )*

                $zalsa::macro_if! { $is_singleton =>
                    pub fn try_get<$Db>(db: &$Db) -> Option<Self>
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + salsa::Database,
                    {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("singleton input ", stringify!($Struct)));
                        let zalsa = db.zalsa();
                        $Configuration::ingredient_(zalsa).get_singleton_input(zalsa)
                    }

                    #[track_caller]
                    pub fn get<$Db>(db: &$Db) -> Self
                    where
                        // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                        $Db: ?Sized + salsa::Database,
                    {
                        Self::try_get(db).unwrap()
                    }
                }

                /// Default debug formatting for this struct (may be useful if you define your own `Debug` impl)
                pub fn default_debug_fmt(this: Self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result
                where
                    // rustc rejects trivial bounds, but it cannot see through higher-ranked bounds
                    // with its check :^)
                    $(for<'__trivial_bounds> $field_ty: ::std::fmt::Debug),*
                {
                    $zalsa::with_attached_database(|db| {
                        $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("input debug fields ", stringify!($Struct)));
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
                            .field("[salsa id]", &this.0)
                            .finish()
                    })
                }
            }

            impl $zalsa_struct::HasBuilder for $Struct {
                type Builder = builder::$Builder;
            }

            // Implement `new` here instead of inside the builder module
            // because $Configuration can't be named in `builder`.
            impl builder::$Builder {
                /// Creates the new input with the set values.
                #[must_use]
                pub fn new<$Db>(self, db: &$Db) -> $Struct
                where
                    // FIXME(rust-lang/rust#65991): The `db` argument *should* have the type `dyn Database`
                    $Db: ?Sized + ::salsa::Database
                {
                    $zalsa::assert_ordinary_execution_allowed(db.zalsas().into(), concat!("input builder ", stringify!($Struct)));
                    let (zalsa, zalsa_local) = db.zalsas();
                    let current_revision = zalsa.current_revision();
                    let ingredient = $Configuration::ingredient_(zalsa);
                    let (fields, revision, durabilities) = builder::builder_into_inner(self, current_revision);
                    ingredient.new_input(zalsa, zalsa_local, fields, revision, durabilities)
                }
            }

            mod builder {
                use super::*;

                use ::salsa::plumbing as $zalsa;
                use $zalsa::input as $zalsa_struct;

                // These are standalone functions instead of methods on `Builder` to prevent
                // that the enclosing module can call them.
                pub(super) fn new_builder($($field_id: $field_ty),*) -> $Builder {
                    $Builder {
                        fields: ($($field_id,)*),
                        durabilities: [::salsa::Durability::default(); $N],
                    }
                }

                pub(super) fn builder_into_inner(builder: $Builder, revision: $zalsa::Revision) -> (($($field_ty,)*), [$zalsa::Revision; $N], [$zalsa::Durability; $N]) {
                    (builder.fields, [revision; $N], [$(builder.durabilities[$field_index]),*])
                }

                #[must_use]
                pub struct $Builder {
                    /// The field values.
                    fields: ($($field_ty,)*),

                    /// The durabilities per field.
                    durabilities: [::salsa::Durability; $N],
                }

                impl $Builder {
                    /// Sets the durability of all fields.
                    ///
                    /// Overrides any previously set durabilities.
                    pub fn durability(mut self, durability: ::salsa::Durability) -> Self {
                        self.durabilities = [durability; $N];
                        self
                    }

                    $($zalsa::maybe_default_tt! { $field_option =>
                        /// Sets the value of the field `$field_id`.
                        #[must_use]
                        pub fn $field_id(mut self, value: $field_ty) -> Self
                        {
                            self.fields.$field_index = value;
                            self
                        }
                    })*

                    $(
                        /// Sets the durability for the field `$field_id`.
                        #[must_use]
                        pub fn $field_durability_id(mut self, durability: ::salsa::Durability) -> Self
                        {
                            self.durabilities[$field_index] = durability;
                            self
                        }
                    )*
                }
            }
        };
    };
}
