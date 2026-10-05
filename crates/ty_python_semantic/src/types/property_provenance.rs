//! Retains the source definition of an accessor after applying a method decorator.
//! Declared-variance checking uses that definition when the decorator returns a callable object.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::function::FunctionType;
use crate::types::{
    BoundMethodType, PropertyAccessorDefinitions, PropertyAccessorRole, PropertyInstanceClass,
    PropertyInstanceType, Type,
};

pub(in crate::types) struct PropertyProvenanceFacts;

pub(in crate::types) struct OrdinaryPropertyProvenanceEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousPropertyProvenanceEffects)]
    pub(in crate::types) trait PropertyProvenanceEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn definitions(&self, property: PropertyInstanceType<'db>) -> Result<PropertyAccessorDefinitions<'db>, Self::Error>;
        #[operation(source)]
        async fn bound_receiver(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn bound_callable(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn function_name(&self, function: FunctionType<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(source)]
        async fn getter(&self, property: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn setter(&self, property: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn deleter(&self, property: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn instance_class(&self, property: PropertyInstanceType<'db>) -> Result<PropertyInstanceClass<'db>, Self::Error>;
        #[operation(local)]
        async fn same_accessor(&self, retained: Option<Type<'db>>, supplied: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn intern_property(&self, getter: Option<Type<'db>>, setter: Option<Type<'db>>, deleter: Option<Type<'db>>, instance_class: PropertyInstanceClass<'db>, definitions: PropertyAccessorDefinitions<'db>) -> Result<PropertyInstanceType<'db>, Self::Error>;
    }

    #[finite_capability]
    impl PropertyProvenanceFacts {
        fn role(&self, name: &Name) -> Option<PropertyAccessorRole> {
            // Each comparison checks a fixed spelling, so it visits at most seven bytes.
            match name.as_str() {
                "getter" => Some(PropertyAccessorRole::Getter),
                "setter" => Some(PropertyAccessorRole::Setter),
                "deleter" => Some(PropertyAccessorRole::Deleter),
                _ => None,
            }
        }

        fn record<'db>(
            &self,
            definitions: &mut PropertyAccessorDefinitions<'db>,
            role: PropertyAccessorRole,
            definition: Definition<'db>,
        ) {
            let slot = match role {
                PropertyAccessorRole::Getter => &mut definitions.getter,
                PropertyAccessorRole::Setter => &mut definitions.setter,
                PropertyAccessorRole::Deleter => &mut definitions.deleter,
            };
            *slot = Some(definition);
        }
    }

    #[synchronous(with_accessor_definition_sync)]
    #[capabilities(effects = PropertyProvenanceEffects, facts = PropertyProvenanceFacts)]
    #[passive_values(PropertyAccessorRole::Getter)]
    pub(in crate::types) async fn with_accessor_definition_with<'db, E: PropertyProvenanceEffects<'db>>(
        property: PropertyInstanceType<'db>,
        decorator: Type<'db>,
        accessor: Type<'db>,
        definition: Definition<'db>,
        facts: PropertyProvenanceFacts,
        effects: &E,
    ) -> Result<PropertyInstanceType<'db>, E::Error> {
        effects.checkpoint().await?;
        let mut definitions = effects.definitions(property).await?;
        let role = match decorator {
            Type::BoundMethod(method)
                if let Type::PropertyInstance(_) = effects.bound_receiver(method).await?
                    && let Type::FunctionLiteral(function) = effects.bound_callable(method).await? =>
            {
                let name = effects.function_name(function).await?;
                let Some(role) = facts.role(name) else {
                    return Ok(property);
                };
                role
            }
            _ => PropertyAccessorRole::Getter,
        };
        let accessor_ty = match role {
            PropertyAccessorRole::Getter => effects.getter(property).await?,
            PropertyAccessorRole::Setter => effects.setter(property).await?,
            PropertyAccessorRole::Deleter => effects.deleter(property).await?,
        };
        if effects.same_accessor(accessor_ty, accessor).await? {
            facts.record(&mut definitions, role, definition);
        }
        let getter = effects.getter(property).await?;
        let setter = effects.setter(property).await?;
        let deleter = effects.deleter(property).await?;
        let instance_class = effects.instance_class(property).await?;
        effects.intern_property(getter, setter, deleter, instance_class, definitions).await
    }
}

impl<'db> SynchronousPropertyProvenanceEffects<'db> for OrdinaryPropertyProvenanceEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn definitions(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<PropertyAccessorDefinitions<'db>, Self::Error> {
        Ok(property.accessor_definitions(self.db))
    }

    fn bound_receiver(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(method.self_instance(self.db))
    }

    fn bound_callable(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(method.func(self.db))
    }

    fn function_name(&self, function: FunctionType<'db>) -> Result<&'db Name, Self::Error> {
        Ok(function.name(self.db))
    }

    fn getter(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(property.getter(self.db))
    }

    fn setter(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(property.setter(self.db))
    }

    fn deleter(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(property.deleter(self.db))
    }

    fn instance_class(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<PropertyInstanceClass<'db>, Self::Error> {
        Ok(property.instance_class(self.db))
    }

    fn same_accessor(
        &self,
        retained: Option<Type<'db>>,
        supplied: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(retained == Some(supplied))
    }

    fn intern_property(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
        instance_class: PropertyInstanceClass<'db>,
        definitions: PropertyAccessorDefinitions<'db>,
    ) -> Result<PropertyInstanceType<'db>, Self::Error> {
        Ok(PropertyInstanceType::new_internal(
            self.db,
            getter,
            setter,
            deleter,
            instance_class,
            definitions,
        ))
    }
}
