//! Convert a subclass type to its instances or metaclass instances through `SourceAccess::endpoint`.
//!
//! Children retain the stored `ClassType` specialization and use the environment reconstructed
//! from `SourceEffects::program`. `subclass_metaclass_instance_value` additionally validates its
//! caller-supplied environment against that program. TypeVar transposition refuses before recursive
//! work. Nested metatype conversion delegates to
//! `MemberEntryEffects::meta_type`, which supports admitted nominal, literal and class-object
//! conversions and refuses unavailable children. Supporting recursive aliases and TypeVars requires
//! the enclosing operation's `TypeRecursionContext`, as retained by `OrdinarySubclassMetaclass` in
//! `types/subclass_of/metaclass.rs`; this adapter does not create a separate recursion context.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::class::{
    ClassMetaclass, KnownClassInstanceEffects, known_class_to_subclass_of_with,
};
use crate::types::relation::source::RelationSourceOperation;
use crate::types::subclass_of::metaclass::{
    MetaclassInstanceEffects, SubclassMetaclassEffects, SubclassMetaclassFacts,
    metaclass_instance_with, subclass_meta_type_with, subclass_to_instance_with,
};
use crate::types::subclass_of::{SubclassConstructionFacts, subclass_from_with};
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    BoundTypeVarInstance, ClassType, DynamicType, KnownClass, MemberEntryEffects, SubclassOfInner,
    SubclassOfType, Type, TypeVarBoundOrConstraints,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Resolves a subclass constraint's metaclass without converting that metaclass to instances.
    pub(in crate::types::infer) async fn subclass_metaclass_value(
        &self,
        subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        let bytes = Self::checked(
            size_of::<(
                SubclassOfInner<'db>,
                SourceSubclassMetaclass<'_, '_, 'run, 'db, A>,
                SubclassOfType<'db>,
                RunResult<Type<'db>>,
                &Self,
            )>()
            .checked_mul(2),
        )?;
        let (inner, effects) = self
            .class_object_local(12, bytes, || {
                (
                    subclass.subclass_of(),
                    SourceSubclassMetaclass { source: self },
                )
            })
            .await?;
        self.class_object_child(|| subclass_meta_type_with(inner, SubclassMetaclassFacts, &effects))
            .await
    }

    /// Converts the stored subclass constraint directly, without computing its metaclass.
    pub(super) async fn subclass_instance_value(
        &self,
        subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        let inner = self.initialize_value(|| subclass.subclass_of()).await?;
        self.local(
            1,
            size_of::<SourceSubclassMetaclass<'_, '_, 'run, 'db, A>>(),
            || (),
        )
        .await?;
        let effects = SourceSubclassMetaclass { source: self };
        self.allocate_future(|| subclass_to_instance_with(inner, &effects))
            .await?
            .await
    }

    pub(super) async fn subclass_metaclass_instance_value(
        &self,
        env: &ProgramEnvironment<'db>,
        subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.allocate_future(|| async {
            let effects = SourceSubclassMetaclass { source: self };
            let inner = self.local(1, 0, || subclass.subclass_of()).await?;
            let metaclass =
                subclass_meta_type_with(inner, SubclassMetaclassFacts, &effects).await?;
            metaclass_instance_with(metaclass, &effects)
                .await?
                .ok_or(RunError::Contract(
                    "the type of a metaclass should always be instantiable",
                ))
        })
        .await?
        .await
    }
}

struct SourceSubclassMetaclass<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceSubclassMetaclass<'_, '_, 'run, 'db, A> {
    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.source.work(1).await?;
        if let Type::SubclassOf(subclass) = ty {
            let inner = self.source.local(1, 0, || subclass.subclass_of()).await?;
            return self
                .source
                .allocate_future(|| subclass_to_instance_with(inner, self))
                .await?
                .await
                .map(Some);
        }
        self.source
            .allocate_future(|| NamespaceLookupEffects::instance_approximation(self.source, ty))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassMetaclassEffects<'db>
    for SourceSubclassMetaclass<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn transpose(&self, inner: SubclassOfInner<'db>) -> RunResult<SubclassOfInner<'db>> {
        self.source.work(1).await?;
        match inner {
            SubclassOfInner::TypeVar(_) => {
                self.source
                    .unavailable(SourceOperation::Relation(
                        RelationSourceOperation::TypeVarSubclassTranspose,
                    ))
                    .await
            }
            _ => Ok(inner),
        }
    }

    async fn subclass(&self, inner: SubclassOfInner<'db>) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| subclass_from_with(inner, SubclassConstructionFacts, self.source))
            .await?
            .await
    }

    async fn inferred_metaclass(&self, class: ClassType<'db>) -> RunResult<ClassMetaclass<'db>> {
        self.source
            .allocate_future(|| NamespaceLookupEffects::inferred_metaclass(self.source, class))
            .await?
            .await
    }

    async fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| NamespaceLookupEffects::for_inheritance(self.source, metaclass))
            .await?
            .await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        SourceSubclassMetaclass::instance_approximation(self, ty).await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| MemberEntryEffects::meta_type(self.source, ty))
            .await?
            .await
    }

    async fn known_type_subclass(&self) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| known_class_to_subclass_of_with(KnownClass::Type, self.source))
            .await?
            .await
    }

    async fn typevar_bounds(
        &self,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarBoundOrConstraints<'db>> {
        self.source
            .unavailable(SourceOperation::Relation(
                RelationSourceOperation::TypevarBoundOrConstraints,
            ))
            .await
    }

    async fn constraints_type(
        &self,
        _constraints: TypeVarConstraints<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::Relation(
                RelationSourceOperation::TypevarConstraints,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MetaclassInstanceEffects<'db>
    for SourceSubclassMetaclass<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.local(1, size_of::<Type<'db>>(), || ()).await
    }

    async fn instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| KnownClassInstanceEffects::instance(self.source, class))
            .await?
            .await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        SourceSubclassMetaclass::instance_approximation(self, ty).await
    }

    async fn dynamic_subclass(&self, dynamic: DynamicType<'db>) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| {
                subclass_from_with(
                    SubclassOfInner::Dynamic(dynamic),
                    SubclassConstructionFacts,
                    self.source,
                )
            })
            .await?
            .await
    }
}
