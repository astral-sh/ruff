//! Identify class access to instance storage whose type depends on class parameters.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::Db;
use crate::place::{DefinedPlace, Place, TypeOrigin};
use crate::types::variance::VarianceInferable;
use crate::types::{
    ClassLiteral, GenericAlias, MemberLookupPolicy, ProgramEnvironment, Type, TypeVarVariance,
    UnionType,
};

pub(in crate::types) struct OrdinaryGenericAttributeEffects<'db, 'env> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'db, 'env> OrdinaryGenericAttributeEffects<'db, 'env> {
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousGenericAttributeEffects)]
    pub(in crate::types) trait GenericAttributeEffects<'db> {
        type Error;
        #[operation(source)]
        async fn union(&self, union: UnionType<'db>, name: &str) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn class(&self, class: ClassLiteral<'db>, name: &str) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias_origin(&self, alias: GenericAlias<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
    }

    #[synchronous(has_generic_instance_attribute_sync)]
    #[capabilities(effects = GenericAttributeEffects)]
    #[passive_values()]
    pub(in crate::types) async fn has_generic_instance_attribute_with<'db, E: GenericAttributeEffects<'db>>(
        ty: Type<'db>, name: &str, effects: &E,
    ) -> Result<bool, E::Error> {
        match ty {
            Type::Union(union) => effects.union(union, name).await,
            Type::ClassLiteral(class) => effects.class(class, name).await,
            Type::GenericAlias(alias) => {
                let class = effects.alias_origin(alias).await?;
                effects.class(class, name).await
            }
            _ => Ok(false),
        }
    }
}

impl<'db> SynchronousGenericAttributeEffects<'db> for OrdinaryGenericAttributeEffects<'db, '_> {
    type Error = Infallible;

    fn union(&self, union: UnionType<'db>, name: &str) -> Result<bool, Self::Error> {
        Ok(union
            .elements(self.db)
            .iter()
            .any(|element| element.has_generic_instance_attribute(self.db, self.env, name)))
    }

    fn class(&self, class: ClassLiteral<'db>, name: &str) -> Result<bool, Self::Error> {
        Ok(class_has_generic_instance_attribute(
            self.db, self.env, class, name,
        ))
    }

    fn alias_origin(&self, alias: GenericAlias<'db>) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(self.db).into())
    }
}

fn class_has_generic_instance_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassLiteral<'db>,
    name: &str,
) -> bool {
    let Some(generic_context) = class
        .as_static()
        .and_then(|class| class.generic_context(db))
    else {
        return false;
    };
    // A metaclass data descriptor takes precedence over the instance declaration.
    if class
        .metaclass(db)
        .find_name_in_mro_with_policy(db, env, name, MemberLookupPolicy::default())
        .and_then(|member| member.place.ignore_possibly_undefined())
        .is_some_and(|ty| ty.is_data_descriptor(db, env))
    {
        return false;
    }
    let member = Type::from(class.identity_specialization(db)).class_object_member(
        db,
        env,
        name,
        MemberLookupPolicy::default(),
    );
    let Place::Defined(DefinedPlace {
        ty,
        origin: TypeOrigin::Declared,
        ..
    }) = member.place
    else {
        return false;
    };
    if member.is_class_var() {
        return false;
    }
    let ty = match ty.resolve_type_alias(db) {
        Type::Union(union) if union.has_aliases(db) => union.expand_aliases(db, env),
        ty => ty,
    };
    let alternatives = match &ty {
        Type::Union(union) => union.elements(db),
        _ => std::slice::from_ref(&ty),
    };
    alternatives.iter().any(|ty| {
        // Synthesized methods bind through `try_call_dunder_get` without necessarily
        // exposing a `__get__` member on their meta-type.
        if let Type::Callable(callable) = ty
            && callable.is_method_like(db)
        {
            return false;
        }

        // Descriptors define their own class-access behavior, but do not exempt other
        // alternatives in a union from the restriction on generic instance storage.
        ty.class_member(db, env, "__get__").is_undefined()
            // Variance accounts for aliases without expanding recursive specializations,
            // and ignores alias arguments that do not affect the resulting type.
            && generic_context.variables(db).any(|typevar| {
                ty.variance_of(db, env, typevar.identity(db)).evaluate(db)
                    != TypeVarVariance::Bivariant
            })
    })
}
