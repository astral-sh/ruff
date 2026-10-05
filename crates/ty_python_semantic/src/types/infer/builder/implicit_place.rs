//! Selection of values supplied outside a name's explicit lexical bindings.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use crate::place::{
    Place, PlaceAndQualifiers, builtins_module_scope, class_body_implicit_symbol,
    explicit_global_symbol, implicit_builtins_symbol, module_type_implicit_global_symbol,
};
use crate::place_load::ImplicitPlaceLoad;
use crate::types::Type;
use crate::types::infer::original_class_type;
use crate::{Db, ProgramEnvironment};

pub(in crate::types::infer) struct ImplicitPlaceFacts;

shared_semantic_family! {
    #[synchronous(SynchronousImplicitPlaceEffects)]
    pub(in crate::types::infer) trait ImplicitPlaceEffects<'db> {
        type Error;

        #[operation(child)]
        async fn original_class(&self, definition: Definition<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn class_body_symbol(&self, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn explicit_global(&self, file: ProgramFile<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn module_global(&self, file: ProgramFile<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn standard_builtins_scope(&self) -> Result<Option<ScopeId<'db>>, Self::Error>;
        #[operation(child)]
        async fn builtin(&self, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ImplicitPlaceFacts {
        fn class_place<'db>(&self, class: Option<Type<'db>>) -> PlaceAndQualifiers<'db> {
            class.map_or_else(|| Place::Undefined.into(), |class| Place::bound(class).into())
        }

        fn definitely_bound<'db>(&self, place: PlaceAndQualifiers<'db>) -> PlaceAndQualifiers<'db> {
            if place.place.is_definitely_bound() { place } else { Place::Undefined.into() }
        }

        fn undefined<'db>(&self) -> PlaceAndQualifiers<'db> {
            Place::Undefined.into()
        }

        fn same_scope<'db>(&self, scope: ScopeId<'db>, builtins: Option<ScopeId<'db>>) -> bool {
            builtins == Some(scope)
        }
    }

    #[synchronous(implicit_place_sync)]
    #[capabilities(effects = ImplicitPlaceEffects, facts = ImplicitPlaceFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn implicit_place_with<'db, E: ImplicitPlaceEffects<'db>>(
        scope: ScopeId<'db>,
        implicit: ImplicitPlaceLoad<'db>,
        effects: &E,
        facts: ImplicitPlaceFacts,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        match implicit {
            ImplicitPlaceLoad::DunderClass(definition) => {
                let class = effects.original_class(definition).await?;
                Ok(facts.class_place(class))
            }
            ImplicitPlaceLoad::ClassBodySymbol(name) => {
                let place = effects.class_body_symbol(&name).await?;
                Ok(facts.definitely_bound(place))
            }
            ImplicitPlaceLoad::ExplicitGlobalSymbol { file, name } => effects.explicit_global(file, &name).await,
            ImplicitPlaceLoad::ModuleImplicitGlobal { file, name } => effects.module_global(file, &name).await,
            ImplicitPlaceLoad::Builtin(name) => {
                let builtins = effects.standard_builtins_scope().await?;
                if facts.same_scope(scope, builtins) {
                    Ok(facts.undefined())
                } else {
                    effects.builtin(&name).await
                }
            }
        }
    }
}

pub(in crate::types::infer) struct InlineImplicitPlaceEffects<'env, 'db> {
    pub(in crate::types::infer) db: &'db dyn Db,
    pub(in crate::types::infer) env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousImplicitPlaceEffects<'db> for InlineImplicitPlaceEffects<'_, 'db> {
    type Error = Infallible;

    fn original_class(&self, definition: Definition<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(original_class_type(self.db, definition).map(Type::from))
    }

    fn class_body_symbol(&self, name: &str) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class_body_implicit_symbol(self.db, self.env, name))
    }

    fn explicit_global(
        &self,
        file: ProgramFile<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(explicit_global_symbol(self.db, file, name))
    }

    fn module_global(
        &self,
        file: ProgramFile<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(module_type_implicit_global_symbol(self.db, file, name))
    }

    fn standard_builtins_scope(&self) -> Result<Option<ScopeId<'db>>, Infallible> {
        Ok(builtins_module_scope(self.db, self.env))
    }

    fn builtin(&self, name: &str) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(implicit_builtins_symbol(self.db, self.env, name))
    }
}
