//! Metaclass wrappers resolve default classes through canonical known-class lookup.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::class::metaclass_selection::{
    MetaclassSelectionResult, StaticMetaclassEffects, static_inferred_metaclass_with,
    static_metaclass_with, static_try_metaclass_with,
};
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::class::{
    ClassMetaclass, KnownClassInstanceEffects, interpret_class_literal_lookup,
};
use crate::types::member_lookup::class_object::ClassObjectEffects;
use crate::types::subclass_of::metaclass::{MetaclassInstanceEffects, metaclass_instance_with};
use crate::types::subclass_of::{SubclassConstructionFacts, subclass_from_with};
use crate::types::{
    ClassLiteral, ClassType, DynamicType, GenericAlias, KnownClass, MemberEntryEffects,
    StaticClassLiteral, SubclassOfInner, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Returns the instance type of a class's lookup metaclass, retaining stored specialization.
    ///
    /// The shared conversion keeps an unknown metaclass's instances constrained to class objects.
    pub(super) async fn class_metaclass_instance_value(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let ty = self.class_object_local(3, 0, || Type::from(class)).await?;
        let metaclass = self
            .class_object_child(|| MemberEntryEffects::meta_type(self, ty))
            .await?;
        self.metaclass_instance_value(metaclass).await
    }

    /// Converts a resolved metaclass to its instances without discarding class-object constraints.
    pub(in crate::types::infer) async fn metaclass_instance_value(
        &self,
        metaclass: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.class_object_local(
            8,
            size_of::<(Type<'db>, Option<Type<'db>>, RunResult<Option<Type<'db>>>)>(),
            || (),
        )
        .await?;
        let instance = self
            .class_object_child(|| metaclass_instance_with(metaclass, self))
            .await?;
        self.class_object_local(3, 0, || {
            instance.ok_or(RunError::Contract(
                "the type of a metaclass should always be instantiable",
            ))
        })
        .await?
    }

    pub(in crate::types::infer::builder) async fn infer_static_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        static_metaclass_with(class, self).await
    }

    /// Resolves a stored alias's metaclass without specializing its protocol lookup fallback.
    pub(in crate::types::infer) async fn generic_alias_metaclass(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Type<'db>> {
        let origin = self.field(alias.field_requests(self.db()).origin()).await?;
        let file = self.static_class_file(origin).await?;
        self.check_file_program(file).await?;
        let bytes = Self::checked(
            size_of::<ClassType<'db>>()
                .checked_mul(2)
                .and_then(|bytes| bytes.checked_add(size_of::<GenericAlias<'db>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<&Self>())),
        )?;
        let class = self
            .class_object_local(6, bytes, || ClassType::from(alias))
            .await?;
        let metaclass = self
            .class_object_child(|| NamespaceLookupEffects::inferred_metaclass(self, class))
            .await?;
        self.metaclass_lookup_value(metaclass).await
    }

    /// Resolves `ClassMetaclass` to the type used for attribute lookup, looking up `ABCMeta`
    /// canonically for protocol fallback.
    pub(in crate::types::infer) async fn metaclass_lookup_value(
        &self,
        metaclass: ClassMetaclass<'db>,
    ) -> RunResult<Type<'db>> {
        let bytes = Self::checked(
            size_of::<Result<Type<'db>, KnownClass>>()
                .checked_mul(2)
                .and_then(|bytes| bytes.checked_add(size_of::<ClassMetaclass<'db>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Type<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<&Self>())),
        )?;
        let target = self
            .class_object_local(12, bytes, || metaclass.lookup_target())
            .await?;
        match target {
            Ok(metaclass) => Ok(metaclass),
            Err(known) => {
                self.class_object_child(|| KnownClassInstanceEffects::class_literal(self, known))
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MetaclassInstanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.class_object_local(4, size_of::<Type<'db>>(), || ())
            .await
    }

    async fn instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.class_object_child(|| KnownClassInstanceEffects::instance(self, class))
            .await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        ClassObjectEffects::instance_approximation(self, ty).await
    }

    async fn dynamic_subclass(&self, dynamic: DynamicType<'db>) -> RunResult<Type<'db>> {
        self.class_object_child(|| {
            subclass_from_with(
                SubclassOfInner::Dynamic(dynamic),
                SubclassConstructionFacts,
                self,
            )
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticMetaclassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(class.field_requests(self.db()).has_explicit_bases())
            .await
    }

    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(class.field_requests(self.db()).has_explicit_metaclass())
            .await
    }

    async fn known_class(
        &self,
        _class: StaticClassLiteral<'db>,
        known: KnownClass,
    ) -> RunResult<Type<'db>> {
        let result = self.access.known_class_lookup(self.program, known).await?;
        self.local(2, 0, || match interpret_class_literal_lookup(result) {
            Some(class) => Type::ClassLiteral(ClassLiteral::Static(class)),
            None => Type::unknown(),
        })
        .await
    }

    async fn try_metaclass_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        self.access.inner_metaclass(class).await
    }

    async fn try_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        static_try_metaclass_with(class, self).await
    }

    async fn inferred_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassMetaclass<'db>> {
        static_inferred_metaclass_with(class, self).await
    }
}
