//! Abstract-method discovery follows the class MRO and records each unimplemented method's definition.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    BindingWithConstraintsIterator, PlaceTable, UseDefMap, place_table, use_def_map,
};

use super::{AbstractMethod, might_be_explicitly_abstract};
use crate::place::{
    DefinedPlace, Place, PlaceWithDefinition, place_from_bindings, place_from_declarations,
};
use crate::types::function::{AbstractMethodKind, FunctionDecorators};
use crate::types::mro::MroIterator;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{
    ClassBase, ClassLiteral, ClassType, FunctionType, PropertyInstanceType, StaticClassLiteral,
    Type,
};
use crate::{Db, FxIndexMap, ProgramEnvironment, TypeQualifiers};

pub(in crate::types) type AbstractMethodMap<'db> = FxIndexMap<Name, AbstractMethod<'db>>;

pub(in crate::types) struct AbstractScope<'db> {
    pub(in crate::types) literal: StaticClassLiteral<'db>,
    pub(in crate::types) places: &'db PlaceTable,
    pub(in crate::types) uses: &'db UseDefMap<'db>,
    pub(in crate::types) implicit: bool,
}

#[derive(Clone, Copy)]
pub(in crate::types) enum Retention<'a, 'db> {
    Dynamic(ClassType<'db>),
    Declarations(&'a AbstractScope<'db>),
    Slots(StaticClassLiteral<'db>),
}

#[derive(Clone, Copy)]
pub(in crate::types) enum Accessor {
    Getter,
    Setter,
    Deleter,
}

pub(in crate::types) trait CandidateEffects<'db> {
    type Error;
    async fn kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;
    async fn decorators(
        &self,
        definition: Definition<'db>,
    ) -> Result<(FunctionDecorators, bool), Self::Error>;
}

pub(in crate::types) async fn might_be_explicitly_abstract_with<'db, E: CandidateEffects<'db>>(
    definition: Definition<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let DefinitionKind::Function(function) = effects.kind(definition).await? else {
        return Ok(true);
    };
    if !function.has_decorators() {
        return Ok(false);
    }
    let (decorators, unknown) = effects.decorators(definition).await?;
    Ok(decorators.contains(FunctionDecorators::ABSTRACT_METHOD) || unknown)
}

pub(in crate::types) trait DiscoveryEffects<'db> {
    type Error;
    type Mro;
    async fn empty(&self) -> Result<AbstractMethodMap<'db>, Self::Error>;
    async fn environment(
        &self,
        class: ClassType<'db>,
    ) -> Result<ProgramEnvironment<'db>, Self::Error>;
    async fn mro(&self, class: ClassType<'db>) -> Result<Self::Mro, Self::Error>;
    async fn next_base(&self, mro: &mut Self::Mro) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
    async fn scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<AbstractScope<'db>, Self::Error>;
    async fn retain(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        env: &ProgramEnvironment<'db>,
        retention: Retention<'_, 'db>,
    ) -> Result<(), Self::Error>;
    async fn remove(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        name: &str,
    ) -> Result<(), Self::Error>;
    async fn dynamic_member_is_undefined(
        &self,
        class: ClassType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn synthesized(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn class_var(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn next_symbol(
        &self,
        scope: &AbstractScope<'db>,
        cursor: &mut usize,
    ) -> Result<Option<ScopedSymbolId>, Self::Error>;
    async fn name(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<&'db Name, Self::Error>;
    async fn contains(
        &self,
        methods: &AbstractMethodMap<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn reachable(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<BindingWithConstraintsIterator<'db, 'db>, Self::Error>;
    async fn next_definition(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'db, 'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error>;
    async fn candidate(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
    async fn binding(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error>;
    async fn abstract_kind(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
    ) -> Result<Option<AbstractMethodKind>, Self::Error>;
    async fn function_kind(
        &self,
        function: FunctionType<'db>,
        class: ClassType<'db>,
    ) -> Result<Option<AbstractMethodKind>, Self::Error>;
    async fn bound_function(
        &self,
        method: crate::types::BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn accessor(
        &self,
        property: PropertyInstanceType<'db>,
        accessor: Accessor,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn insert(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        name: &Name,
        method: AbstractMethod<'db>,
    ) -> Result<(), Self::Error>;
    async fn slot(&self, class: StaticClassLiteral<'db>, name: &str) -> Result<bool, Self::Error>;
    async fn finish(&self, methods: &mut AbstractMethodMap<'db>) -> Result<(), Self::Error>;
}

pub(in crate::types) async fn type_as_abstract_method_with<'db, E: DiscoveryEffects<'db>>(
    ty: Type<'db>,
    class: ClassType<'db>,
    effects: &E,
) -> Result<Option<AbstractMethodKind>, E::Error> {
    match ty {
        Type::FunctionLiteral(function) => effects.function_kind(function, class).await,
        Type::BoundMethod(method) => {
            let function = effects.bound_function(method).await?;
            effects.abstract_kind(function, class).await
        }
        Type::PropertyInstance(property) => {
            // A property is abstract if any of its accessors is abstract.
            for accessor in [Accessor::Getter, Accessor::Setter, Accessor::Deleter] {
                if let Some(ty) = effects.accessor(property, accessor).await?
                    && let Some(kind) = effects.abstract_kind(ty, class).await?
                {
                    return Ok(Some(kind));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

pub(in crate::types) async fn retain_method_with<'db, E: DiscoveryEffects<'db>>(
    name: &str,
    env: &ProgramEnvironment<'db>,
    retention: Retention<'_, 'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    match retention {
        Retention::Dynamic(class) => effects.dynamic_member_is_undefined(class, env, name).await,
        Retention::Declarations(scope) => {
            if effects.synthesized(scope.literal, env, name).await? {
                return Ok(false);
            }
            Ok(!effects.class_var(scope, env, name).await?)
        }
        Retention::Slots(class) => Ok(!effects.slot(class, name).await?),
    }
}

pub(in crate::types) async fn abstract_methods_with<'db, E: DiscoveryEffects<'db>>(
    class: ClassType<'db>,
    effects: &E,
) -> Result<AbstractMethodMap<'db>, E::Error> {
    let mut methods = effects.empty().await?;
    let env = effects.environment(class).await?;
    let mut mro = effects.mro(class).await?;
    // Iterate through the MRO in reverse order,
    // skipping `object` (we know it doesn't define any abstract methods)
    effects.next_base(&mut mro).await?;
    while let Some(base) = effects.next_base(&mut mro).await? {
        let ClassBase::Class(class) = base else {
            continue;
        };
        let ClassLiteral::Static(literal) = effects.literal(class).await? else {
            // Currently we do not recognize dynamic classes as being able to define abstract methods,
            // but we do recognise them as being able to override abstract methods defined in static classes.
            effects
                .retain(&mut methods, &env, Retention::Dynamic(class))
                .await?;
            continue;
        };
        let scope = effects.scope(literal).await?;
        // Treat abstract methods from superclasses as having been overridden
        // if this class has a synthesized method by that name,
        // or this class has a `ClassVar` declaration by that name
        effects
            .retain(&mut methods, &env, Retention::Declarations(&scope))
            .await?;
        let mut symbols = 0;
        while let Some(symbol) = effects.next_symbol(&scope, &mut symbols).await? {
            let name = effects.name(&scope, symbol).await?;
            // Avoid inferring signatures for methods that cannot introduce abstractness.
            // Inspect all reachable definitions: an earlier overload can be abstract even
            // when the final implementation is concrete.
            if !scope.implicit && !effects.contains(&methods, name).await? {
                let mut reachable = effects.reachable(&scope, symbol).await?;
                let mut candidate = false;
                while let Some(definition) = effects.next_definition(&mut reachable).await? {
                    if effects.candidate(definition).await? {
                        candidate = true;
                        break;
                    }
                }
                if !candidate {
                    continue;
                }
            }
            let place = effects.binding(&scope, &env, symbol).await?;
            let Place::Defined(DefinedPlace { ty, .. }) = place.place else {
                continue;
            };
            let Some(definition) = place.first_definition else {
                continue;
            };
            if let Some(kind) = effects.abstract_kind(ty, class).await? {
                effects
                    .insert(
                        &mut methods,
                        name,
                        AbstractMethod {
                            defining_class: class,
                            definition,
                            kind,
                        },
                    )
                    .await?;
            } else {
                // If this method is concrete, remove it from the map of abstract methods.
                effects.remove(&mut methods, name).await?;
            }
        }
        // Slot descriptors override abstract properties. Dataclass-generated slots can also
        // replace abstract properties defined in this class's body.
        effects
            .retain(&mut methods, &env, Retention::Slots(literal))
            .await?;
    }
    effects.finish(&mut methods).await?;
    Ok(methods)
}

pub(in crate::types) struct OrdinaryDiscoveryEffects<'db>(pub(in crate::types) &'db dyn Db);

impl<'db> CandidateEffects<'db> for OrdinaryDiscoveryEffects<'db> {
    type Error = Infallible;
    async fn kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Infallible> {
        Ok(definition.kind(self.0))
    }
    async fn decorators(
        &self,
        definition: Definition<'db>,
    ) -> Result<(FunctionDecorators, bool), Infallible> {
        let decorators = crate::types::infer::function_known_decorators(self.0, definition);
        Ok((
            decorators.known_decorators(),
            decorators.has_unknown_decorators(),
        ))
    }
}

impl<'db> DiscoveryEffects<'db> for OrdinaryDiscoveryEffects<'db> {
    type Error = Infallible;
    type Mro = MroIterator<'db>;
    async fn empty(&self) -> Result<AbstractMethodMap<'db>, Infallible> {
        Ok(FxIndexMap::default())
    }
    async fn environment(
        &self,
        class: ClassType<'db>,
    ) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_file(
            class.class_literal(self.0).program_file(self.0),
        ))
    }
    async fn mro(&self, class: ClassType<'db>) -> Result<Self::Mro, Infallible> {
        Ok(class.iter_mro(self.0))
    }
    async fn next_base(&self, mro: &mut Self::Mro) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(mro.next_back())
    }
    async fn literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Infallible> {
        Ok(class.class_literal(self.0))
    }
    async fn scope(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> Result<AbstractScope<'db>, Infallible> {
        let scope = literal.body_scope(self.0);
        Ok(AbstractScope {
            literal,
            places: place_table(self.0, scope),
            uses: use_def_map(self.0, scope),
            implicit: !literal.file(self.0).is_stub(self.0)
                && ClassType::NonGeneric(literal.into()).is_protocol(self.0),
        })
    }
    async fn retain(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        env: &ProgramEnvironment<'db>,
        retention: Retention<'_, 'db>,
    ) -> Result<(), Infallible> {
        methods.retain(|name, _| legacy_inline(retain_method_with(name, env, retention, self)));
        Ok(())
    }
    async fn remove(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        name: &str,
    ) -> Result<(), Infallible> {
        methods.shift_remove(name);
        Ok(())
    }
    async fn dynamic_member_is_undefined(
        &self,
        class: ClassType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(class
            .own_class_member(self.0, env, None, name)
            .is_undefined())
    }
    async fn synthesized(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(class
            .own_synthesized_member(self.0, env, None, None, name)
            .is_some())
    }
    async fn class_var(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(scope.places.symbol_id(name).is_some_and(|symbol| {
            place_from_declarations(
                self.0,
                env,
                scope.uses.end_of_scope_symbol_declarations(symbol),
            )
            .ignore_conflicting_declarations()
            .qualifiers
            .contains(TypeQualifiers::CLASS_VAR)
        }))
    }
    async fn next_symbol(
        &self,
        scope: &AbstractScope<'db>,
        cursor: &mut usize,
    ) -> Result<Option<ScopedSymbolId>, Infallible> {
        let symbol = scope.uses.end_of_scope_symbol_at(*cursor);
        *cursor += usize::from(symbol.is_some());
        Ok(symbol)
    }
    async fn name(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<&'db Name, Infallible> {
        Ok(scope.places.symbol(symbol).name())
    }
    async fn contains(
        &self,
        methods: &AbstractMethodMap<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(methods.contains_key(name))
    }
    async fn reachable(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<BindingWithConstraintsIterator<'db, 'db>, Infallible> {
        Ok(scope.uses.reachable_symbol_bindings(symbol))
    }
    async fn next_definition(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'db, 'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(bindings.find_map(|binding| binding.binding.definition()))
    }
    async fn candidate(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(might_be_explicitly_abstract(self.0, definition))
    }
    async fn binding(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceWithDefinition<'db>, Infallible> {
        Ok(place_from_bindings(
            self.0,
            env,
            scope.uses.end_of_scope_symbol_bindings(symbol),
        ))
    }
    async fn abstract_kind(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
    ) -> Result<Option<AbstractMethodKind>, Infallible> {
        Ok(legacy_inline(type_as_abstract_method_with(ty, class, self)))
    }
    async fn function_kind(
        &self,
        function: FunctionType<'db>,
        class: ClassType<'db>,
    ) -> Result<Option<AbstractMethodKind>, Infallible> {
        Ok(function.as_abstract_method(self.0, class))
    }
    async fn bound_function(
        &self,
        method: crate::types::BoundMethodType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(method.func(self.0))
    }
    async fn accessor(
        &self,
        property: PropertyInstanceType<'db>,
        accessor: Accessor,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(match accessor {
            Accessor::Getter => property.getter(self.0),
            Accessor::Setter => property.setter(self.0),
            Accessor::Deleter => property.deleter(self.0),
        })
    }
    async fn insert(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        name: &Name,
        method: AbstractMethod<'db>,
    ) -> Result<(), Infallible> {
        methods.insert(name.clone(), method);
        Ok(())
    }
    async fn slot(&self, class: StaticClassLiteral<'db>, name: &str) -> Result<bool, Infallible> {
        Ok(class.has_own_slot_descriptor(self.0, name))
    }
    async fn finish(&self, methods: &mut AbstractMethodMap<'db>) -> Result<(), Infallible> {
        methods.shrink_to_fit();
        Ok(())
    }
}
