use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::class::{CodeGeneratorKind, DynamicNamedTupleLiteral};
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_sync};
use crate::types::mro::root::InlineMroRootEffects;
use crate::types::{ClassBase, ClassLiteral, ClassType, Specialization, StaticClassLiteral};

pub(in crate::types) trait NamedTupleFieldEffects<'db> {
    type Error;

    /// Runs local work using quoted work units and requested storage bytes.
    /// Controlled execution admits both costs before calling `action`; `None` means an
    /// overflowed quotation and refuses the action. Ordinary execution calls it directly.
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Runs a scalar operation with one work unit and `size_of::<T>()` result bytes.
    /// Variable work and transitive storage require separate admission; this fixed charge
    /// does not cover traversal, allocation, or cleanup of an owned collection.
    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn next_mro(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;

    async fn class_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Self::Error>;

    async fn is_named_tuple(&self, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;

    /// Looks up a static NamedTuple field's first declaration. `None` means no matching
    /// field; `Some(None)` means a matching field without a declaration to return.
    async fn static_field(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        field_name: &Name,
    ) -> Result<Option<Option<Definition<'db>>>, Self::Error>;

    async fn dynamic_has_field(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        field_name: &Name,
    ) -> Result<bool, Self::Error>;

    async fn dynamic_definition(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error>;
}

/// Returns the first inherited `NamedTuple` field in the MRO for `field_name`.
/// The tuple identifies the superclass that owns the field and an optional diagnostic location:
/// the field's first declaration for a static NamedTuple, or the NamedTuple's own definition for
/// a dynamic NamedTuple. A matching field can have no such definition.
pub(in crate::types) async fn conflicting_named_tuple_field_with<
    'db,
    E: NamedTupleFieldEffects<'db>,
>(
    class: StaticClassLiteral<'db>,
    field_name: &Name,
    effects: &E,
) -> Result<Option<(ClassType<'db>, Option<Definition<'db>>)>, E::Error> {
    let mut cursor = effects.step(|| MroCursor::new(class.into(), None)).await?;
    effects.next_mro(&mut cursor).await?;
    while let Some(class_base) = effects.next_mro(&mut cursor).await? {
        let Some(superclass) = effects.step(|| class_base.into_class()).await? else {
            continue;
        };
        let (superclass_literal, superclass_specialization) =
            effects.class_identity(superclass).await?;

        if effects.is_named_tuple(superclass_literal).await? {
            match superclass_literal {
                ClassLiteral::Static(superclass_literal) => {
                    if let Some(declaration) = effects
                        .static_field(superclass_literal, superclass_specialization, field_name)
                        .await?
                    {
                        return effects.step(|| Some((superclass, declaration))).await;
                    }
                }
                ClassLiteral::DynamicNamedTuple(namedtuple) => {
                    if effects.dynamic_has_field(namedtuple, field_name).await? {
                        let definition = effects.dynamic_definition(namedtuple).await?;
                        return effects.step(|| Some((superclass, definition))).await;
                    }
                }
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => {}
            }
        }
    }

    effects.step(|| None).await
}

pub(super) struct OrdinaryNamedTupleFieldEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> NamedTupleFieldEffects<'db> for OrdinaryNamedTupleFieldEffects<'db> {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
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

    async fn class_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Infallible> {
        Ok(class.class_literal_and_specialization(self.db))
    }

    async fn is_named_tuple(&self, class: ClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(CodeGeneratorKind::NamedTuple.matches(self.db, class))
    }

    async fn static_field(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        field_name: &Name,
    ) -> Result<Option<Option<Definition<'db>>>, Infallible> {
        Ok(class
            .own_fields(self.db, specialization, CodeGeneratorKind::NamedTuple)
            .get(field_name)
            .map(|field| field.first_declaration))
    }

    async fn dynamic_has_field(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        field_name: &Name,
    ) -> Result<bool, Infallible> {
        Ok(class.field(self.db, field_name).is_some())
    }

    async fn dynamic_definition(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(class.definition(self.db))
    }
}
