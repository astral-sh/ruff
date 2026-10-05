//! Convert subclass types through their metaclasses while preserving class-object constraints.
//!
//! `OrdinarySubclassMetaclass` passes the enclosing operation's `TypeRecursionContext` to
//! transposition and nested metatype conversion, preserving its alias and TypeVar recursion guards.
//! Instance conversion retains the stored class specialization, and a dynamic metaclass instance
//! remains a subclass type rather than becoming an unconstrained value.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::class::ClassMetaclass;
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    BoundTypeVarInstance, ClassType, DynamicType, KnownClass, SubclassOfInner, SubclassOfType,
    Type, TypeRecursionContext, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct SubclassMetaclassFacts;

shared_semantic_family! {
    #[synchronous(SynchronousSubclassMetaclassEffects)]
    pub(in crate::types) trait SubclassMetaclassEffects<'db> {
        type Error;
        #[operation(child)]
        async fn transpose(&self, inner: SubclassOfInner<'db>) -> Result<SubclassOfInner<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass(&self, inner: SubclassOfInner<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn inferred_metaclass(&self, class: ClassType<'db>) -> Result<ClassMetaclass<'db>, Self::Error>;
        #[operation(child)]
        async fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn known_type_subclass(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn typevar_bounds(&self, typevar: BoundTypeVarInstance<'db>) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error>;
        #[operation(child)]
        async fn constraints_type(&self, constraints: TypeVarConstraints<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(SynchronousMetaclassInstanceEffects)]
    pub(in crate::types) trait MetaclassInstanceEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn dynamic_subclass(&self, dynamic: DynamicType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl SubclassMetaclassFacts {
        fn unknown_subclass<'db>(&self) -> Type<'db> {
            SubclassOfType::subclass_of_unknown()
        }
    }

    #[synchronous(subclass_meta_type_sync)]
    #[capabilities(effects = SubclassMetaclassEffects, facts = SubclassMetaclassFacts)]
    #[passive_values(SubclassOfInner::Dynamic)]
    pub(in crate::types) async fn subclass_meta_type_with<'db, E: SubclassMetaclassEffects<'db>>(
        inner: SubclassOfInner<'db>, facts: SubclassMetaclassFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match effects.transpose(inner).await? {
            SubclassOfInner::Dynamic(dynamic) => effects.subclass(SubclassOfInner::Dynamic(dynamic)).await,
            // A metaclass selected at runtime can already have a type such as `type[M]`,
            // rather than being a class literal. Projecting to instances preserves this
            // constraint when computing its possible subclasses.
            SubclassOfInner::Class(class) => {
                let metaclass = effects.inferred_metaclass(class).await?;
                let metaclass = effects.for_inheritance(metaclass).await?;
                match effects.instance_approximation(metaclass).await? {
                    Some(instance) => effects.meta_type(instance).await,
                    None => Ok(facts.unknown_subclass()),
                }
            }
            // Structural implementations of a protocol can have arbitrary metaclasses. The only
            // guaranteed upper bound is therefore `type`, not the protocol origin's metaclass.
            SubclassOfInner::Protocol(_) => effects.known_type_subclass().await,
            // For `type[T]` where `T` is a TypeVar, `with_transposed_type_var` transforms
            // the bounds from instance types to `type[]` types. For example, `type[T]` where
            // `T: A | B` becomes a TypeVar with bound `type[A] | type[B]`. The metatype is
            // then the metatype of that bound.
            SubclassOfInner::TypeVar(bound_typevar) => {
                match effects.typevar_bounds(bound_typevar).await? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => effects.meta_type(bound).await,
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        let bound = effects.constraints_type(constraints).await?;
                        effects.meta_type(bound).await
                    }
                }
            }
        }
    }

    #[synchronous(subclass_to_instance_sync)]
    #[capabilities(effects = MetaclassInstanceEffects)]
    #[passive_values(Type::Dynamic, Type::ProtocolInstance, Type::TypeVar)]
    pub(in crate::types) async fn subclass_to_instance_with<'db, E: MetaclassInstanceEffects<'db>>(
        inner: SubclassOfInner<'db>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match inner {
            SubclassOfInner::Class(class) => effects.instance(class).await,
            SubclassOfInner::Dynamic(dynamic) => Ok(Type::Dynamic(dynamic)),
            SubclassOfInner::Protocol(protocol) => Ok(Type::ProtocolInstance(protocol)),
            SubclassOfInner::TypeVar(typevar) => Ok(Type::TypeVar(typevar)),
        }
    }

    #[synchronous(metaclass_instance_sync)]
    #[capabilities(effects = MetaclassInstanceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn metaclass_instance_with<'db, E: MetaclassInstanceEffects<'db>>(
        metaclass: Type<'db>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let Some(instance) = effects.instance_approximation(metaclass).await? else {
            return Ok(None);
        };
        // TODO: Intersect `instance` with `type` once equivalent representations are unified:
        // https://github.com/astral-sh/ty/issues/222
        match instance {
            Type::Dynamic(dynamic) => Ok(Some(effects.dynamic_subclass(dynamic).await?)),
            _ => Ok(Some(instance)),
        }
    }
}

pub(super) struct OrdinarySubclassMetaclass<'env, 'context, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) context: &'context TypeRecursionContext<'db>,
}

impl<'db> SynchronousSubclassMetaclassEffects<'db> for OrdinarySubclassMetaclass<'_, '_, 'db> {
    type Error = Infallible;

    fn transpose(&self, inner: SubclassOfInner<'db>) -> Result<SubclassOfInner<'db>, Infallible> {
        Ok(inner.with_transposed_type_var_with_recursion(self.db, self.env, self.context))
    }
    fn subclass(&self, inner: SubclassOfInner<'db>) -> Result<Type<'db>, Infallible> {
        Ok(SubclassOfType::from(self.db, self.env, inner))
    }
    fn inferred_metaclass(&self, class: ClassType<'db>) -> Result<ClassMetaclass<'db>, Infallible> {
        Ok(class.inferred_metaclass(self.db))
    }
    fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> Result<Type<'db>, Infallible> {
        Ok(metaclass.for_inheritance(self.db, self.env))
    }
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }
    fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.to_meta_type_with_recursion(self.db, self.env, self.context))
    }
    fn known_type_subclass(&self) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Type.to_subclass_of(self.db, self.env))
    }
    fn typevar_bounds(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarBoundOrConstraints<'db>, Infallible> {
        Ok(
            match typevar
                .typevar(self.db)
                .bound_or_constraints(self.db, self.env)
            {
                // `with_transposed_type_var` always adds a bound for unbounded TypeVars
                None => unreachable!(),
                Some(bounds) => bounds,
            },
        )
    }
    fn constraints_type(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(constraints.as_type(self.db, self.env))
    }
}

pub(in crate::types) struct OrdinaryMetaclassInstance<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousMetaclassInstanceEffects<'db> for OrdinaryMetaclassInstance<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(Type::instance(self.db, self.env, class))
    }
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }
    fn dynamic_subclass(&self, dynamic: DynamicType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(SubclassOfType::from(self.db, self.env, dynamic))
    }
}
