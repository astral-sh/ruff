use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{place_table, use_def_map};

use super::{
    VariableKind, effective_superclass_variable_kind, is_function_definition, symbol_definition,
};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers, TypeOrigin};
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_sync};
use crate::types::mro::root::InlineMroRootEffects;
use crate::types::{
    ClassBase, ClassType, Specialization, StaticClassLiteral, Type, TypeQualifiers,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) trait VariableKindEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn has_get_descriptor(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
}

pub(in crate::types) trait EffectiveVariableKindEffects<'db>:
    VariableKindEffects<'db>
{
    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;

    async fn scope_symbol(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>), Self::Error>;

    async fn synthesized_member(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> Result<bool, Self::Error>;

    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error>;

    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn symbol_is_assignment(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error>;

    async fn mro_cursor(&self, class: ClassType<'db>) -> Result<MroCursor<'db>, Self::Error>;
    async fn next_mro(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn effective_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Option<VariableKind>, Self::Error>;
}

pub(in crate::types) trait FunctionDefinitionEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<BindingWithConstraintsIterator<'db, 'db>, Self::Error>;
    async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
}

/// Returns whether any retained binding for the symbol is a function definition.
/// This is the shared body of [`super::is_function_definition`], whose tracked boundary isolates definition-kind reads.
pub(in crate::types) async fn is_function_definition_with<
    'db,
    E: FunctionDefinitionEffects<'db>,
>(
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
    effects: &E,
) -> Result<bool, E::Error> {
    let mut bindings = effects.bindings(scope, symbol).await?;
    while let Some(definition) = effects
        .step(|| bindings.next().map(|binding| binding.binding.definition()))
        .await?
    {
        if let Some(definition) = definition
            && effects.is_function(definition).await?
        {
            return effects.step(|| true).await;
        }
    }
    effects.step(|| false).await
}

/// Returns the variable kind for an attribute if it should participate in `ClassVar` override checks.
pub(in crate::types) async fn variable_kind_with<'db, E: VariableKindEffects<'db>>(
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
    effects: &E,
) -> Result<Option<VariableKind>, E::Error> {
    if effects
        .step(|| class_member.is_class_var() || instance_member.is_class_var())
        .await?
    {
        return effects.step(|| Some(VariableKind::Class)).await;
    }

    // A `Final` attribute behaves like a class variable, but final overrides are diagnosed by
    // `override-of-final-variable` instead of this rule.
    if effects
        .step(|| class_member.qualifiers.contains(TypeQualifiers::FINAL))
        .await?
    {
        return effects.step(|| None).await;
    }

    // A method definition is a descriptor in the class body, not an instance variable declaration,
    // even though instance lookup binds it as a method. It should therefore not participate in the
    // class-variable vs. instance-variable declaration check. For example, `Sub.f` here is a
    // descriptor stored on the class, not an instance attribute:
    //
    // ```python
    // class Base:
    //     f: ClassVar[int]
    //
    // class Sub(Base):
    //     def f(self) -> int: ...
    // ```
    if effects
        .step(|| {
            matches!(
                class_member.place,
                Place::Defined(DefinedPlace {
                    ty: Type::FunctionLiteral(_),
                    ..
                })
            )
        })
        .await?
    {
        return effects.step(|| None).await;
    }

    // Descriptor values are not normal instance variables: lookup calls `__get__`, so the value
    // exposed through an instance can differ from the value stored on the class. For example,
    // `attr = property(lambda self: 1)` installs a descriptor value, so `C().attr` exposes the
    // getter return type instead of the `property` object. By contrast, `attr: Descriptor` only
    // annotates an instance attribute; the annotated type having `__get__` does not make `C.attr`
    // a descriptor value.
    let inferred_type = effects
        .step(|| match class_member.place {
            Place::Defined(DefinedPlace {
                ty,
                origin: TypeOrigin::Inferred,
                ..
            }) => Some(ty),
            _ => None,
        })
        .await?;
    if let Some(ty) = inferred_type
        && effects.has_get_descriptor(ty).await?
    {
        return effects.step(|| None).await;
    }
    effects.step(|| Some(VariableKind::Instance)).await
}

async fn inherited_kind_with<'db, E: EffectiveVariableKindEffects<'db>>(
    class: ClassType<'db>,
    name: &Name,
    effects: &E,
) -> Result<Option<VariableKind>, E::Error> {
    let mut cursor = effects.mro_cursor(class).await?;
    effects.next_mro(&mut cursor).await?;
    while let Some(base) = effects.next_mro(&mut cursor).await? {
        let Some(base) = effects.step(|| base.into_class()).await? else {
            continue;
        };
        if let Some(kind) = effects.effective_kind(base, name).await? {
            return effects.step(|| Some(kind)).await;
        }
    }
    effects.step(|| None).await
}

/// Returns the effective declaration kind, retaining an inherited `ClassVar` through unannotated class-body assignments.
/// An unclassified own member falls back to the first classified superclass.
pub(in crate::types) async fn effective_variable_kind_with<
    'db,
    E: EffectiveVariableKindEffects<'db>,
>(
    superclass: ClassType<'db>,
    name: &Name,
    effects: &E,
) -> Result<Option<VariableKind>, E::Error> {
    let Some((literal, specialization)) = effects.static_identity(superclass).await? else {
        return effects.step(|| None).await;
    };
    let (scope, symbol) = effects.scope_symbol(literal, name).await?;
    let has_own_member = match symbol {
        Some((_, present)) => effects.step(|| present).await?,
        None => {
            effects
                .synthesized_member(literal, specialization, name)
                .await?
        }
    };
    if has_own_member {
        // Method definitions and properties are not instance-variable declarations. Check the symbol
        // definition before class/instance member lookup can erase that distinction. For example,
        // resolving an abstract `@property def f(self) -> int` through instance-member lookup would
        // make it look like an instance variable of type `int`, causing this rule to report
        // `f: ClassVar[int]` as an invalid attribute override even though the superclass member is not
        // an instance-attribute declaration.
        if let Some((symbol, _)) = symbol
            && effects.is_function_definition(scope, symbol).await?
        {
            return effects
                .step(|| inherited_kind_with(superclass, name, effects))
                .await?
                .await;
        }
        let class_member = effects.own_class_member(superclass, name).await?;
        // Final attributes have their own override rule and diagnostic. Treating them as class
        // variables here would report both diagnostics for the same override.
        if effects
            .step(|| class_member.qualifiers.contains(TypeQualifiers::FINAL))
            .await?
        {
            return effects
                .step(|| inherited_kind_with(superclass, name, effects))
                .await?
                .await;
        }
        let instance_member = effects.own_instance_member(superclass, name).await?;
        let kind = effects
            .step(|| variable_kind_with(class_member, instance_member, effects))
            .await?
            .await?;
        if effects
            .step(|| kind == Some(VariableKind::Instance))
            .await?
            && let Some((symbol, _)) = symbol
            && effects.symbol_is_assignment(scope, symbol).await?
        {
            let inherited = effects
                .step(|| inherited_kind_with(superclass, name, effects))
                .await?
                .await?;
            if effects
                .step(|| inherited == Some(VariableKind::Class))
                .await?
            {
                return effects.step(|| Some(VariableKind::Class)).await;
            }
        }
        if effects.step(|| kind.is_some()).await? {
            return effects.step(|| kind).await;
        }
    }
    effects
        .step(|| inherited_kind_with(superclass, name, effects))
        .await?
        .await
}

pub(super) struct OrdinaryVariableKindEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'db> VariableKindEffects<'db> for OrdinaryVariableKindEffects<'_, 'db> {
    type Error = Infallible;
    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }
    async fn has_get_descriptor(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty
            .class_member(self.db, self.env, "__get__")
            .place
            .ignore_possibly_undefined()
            .is_some())
    }
}

impl<'db> EffectiveVariableKindEffects<'db> for OrdinaryVariableKindEffects<'_, 'db> {
    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Infallible> {
        Ok(class.static_class_literal(self.db))
    }
    async fn scope_symbol(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>), Infallible> {
        let scope = class.body_scope(self.db);
        let table = place_table(self.db, scope);
        Ok((
            scope,
            table.symbol_id(name).map(|id| {
                let symbol = table.symbol(id);
                (id, symbol.is_bound() || symbol.is_declared())
            }),
        ))
    }
    async fn synthesized_member(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> Result<bool, Infallible> {
        Ok(class
            .own_synthesized_member(self.db, self.env, specialization, None, name)
            .is_some())
    }
    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Infallible> {
        Ok(is_function_definition(self.db, scope, symbol))
    }
    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class.own_class_member(self.db, self.env, None, name).inner)
    }
    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class.own_instance_member(self.db, self.env, name).inner)
    }
    async fn symbol_is_assignment(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Infallible> {
        Ok(
            symbol_definition(self.db, scope, symbol).is_some_and(|definition| {
                matches!(
                    definition.kind(self.db),
                    DefinitionKind::Assignment(_) | DefinitionKind::AugmentedAssignment(_)
                )
            }),
        )
    }
    async fn mro_cursor(&self, class: ClassType<'db>) -> Result<MroCursor<'db>, Infallible> {
        let (literal, specialization) = class.class_literal_and_specialization(self.db);
        Ok(MroCursor::new(literal, specialization))
    }
    async fn next_mro(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        mro_next_sync(
            self.db,
            cursor,
            MroDirection::Forward,
            &InlineMroRootEffects::new(self.db),
        )
    }
    async fn effective_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Option<VariableKind>, Infallible> {
        Ok(effective_superclass_variable_kind(
            self.db,
            class,
            name.clone(),
        ))
    }
}

pub(super) struct OrdinaryFunctionDefinitionEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> FunctionDefinitionEffects<'db> for OrdinaryFunctionDefinitionEffects<'db> {
    type Error = Infallible;
    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }
    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<BindingWithConstraintsIterator<'db, 'db>, Infallible> {
        Ok(use_def_map(self.db, scope).end_of_scope_symbol_bindings(symbol))
    }
    async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(definition.kind(self.db).is_function_def())
    }
}
