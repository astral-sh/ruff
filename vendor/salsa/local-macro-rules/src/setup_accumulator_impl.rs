/// Macro for setting up a function that must intern its arguments.
#[macro_export]
macro_rules! setup_accumulator_impl {
    (
        // Name of the struct
        Struct: $Struct:ident,

        // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
        // We have the procedural macro generate names for those items that are
        // not used elsewhere in the user's code.
        unused_names: [
            $zalsa:ident,
            $zalsa_struct:ident,
            $CACHE:ident,
            $ingredient:ident,
        ]
    ) => {
        #[allow(clippy::all)]
        #[allow(dead_code)]
        const _: () = {
            use salsa::plumbing as $zalsa;
            use salsa::plumbing::accumulator as $zalsa_struct;

            impl $zalsa::HasJar for $Struct {
                type Jar = $zalsa_struct::JarImpl<$Struct>;
                const KIND: $zalsa::JarKind = $zalsa::JarKind::Struct;
            }

            $zalsa::register_jar! {
                $zalsa::ErasedJar::erase::<$Struct>()
            }

            fn $ingredient(zalsa: &$zalsa::Zalsa) -> &$zalsa_struct::IngredientImpl<$Struct> {
                static $CACHE: $zalsa::IngredientCache<$zalsa_struct::IngredientImpl<$Struct>> =
                    $zalsa::IngredientCache::new();

                // SAFETY: The ingredient at offset 0 in `JarImpl<$Struct>` has type
                // `IngredientImpl<$Struct>`.
                unsafe { $CACHE.get_or_create::<$zalsa_struct::JarImpl<$Struct>, 0>(zalsa) }
            }

            impl $zalsa::Accumulator for $Struct {
                const DEBUG_NAME: &'static str = stringify!($Struct);

                fn accumulate<Db>(self, db: &Db)
                where
                    Db: ?Sized + $zalsa::Database,
                {
                    let (zalsa, zalsa_local) = db.zalsas();
                    $ingredient(zalsa).push(zalsa_local, self);
                }
            }
        };
    };
}
