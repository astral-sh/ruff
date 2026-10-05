//! Class-base conversion decisions and the dependencies needed to resolve them.

use std::convert::Infallible;

use super::ClassBase;
use crate::types::class::CodeGeneratorKind;
use crate::types::known_instance::InternedType;
use crate::types::newtype::NewType;
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::tuple::TupleType;
use crate::types::{
    ClassLiteral, ClassType, DynamicType, IntersectionType, KnownClass, KnownInstanceType,
    NominalInstanceType, RecursiveType, SpecialFormType, Type, TypeAliasType, TypingModule,
    UnionType, todo_type,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ClassBaseConversion<'db> {
    /// `None` is a completed decision that the type cannot be a class base.
    Ready(Option<ClassBase<'db>>),
    Dependency(ClassBaseDependency<'db>),
}

impl<'db> ClassBaseConversion<'db> {
    /// Classify a conversion using only the type's stored representation.
    pub(in crate::types) fn from_type(ty: Type<'db>) -> Self {
        match ty {
            Type::RecursiveVar(_) => {
                Self::Dependency(ClassBaseDependency::UnboundRecursiveVariable)
            }
            Type::Dynamic(dynamic) => Self::Ready(Some(ClassBase::Dynamic(dynamic))),
            Type::Divergent(divergent) => Self::Ready(Some(ClassBase::Divergent(divergent))),
            Type::Recursive(recursive) => {
                Self::Dependency(ClassBaseDependency::Recursive(recursive))
            }
            Type::ClassLiteral(literal) => {
                Self::Dependency(ClassBaseDependency::DefaultSpecialization(literal))
            }
            Type::GenericAlias(generic) => {
                Self::Ready(Some(ClassBase::Class(ClassType::Generic(generic))))
            }
            Type::NominalInstance(instance) => {
                Self::Dependency(ClassBaseDependency::NominalInstance(instance))
            }
            Type::SubclassOf(subclass_of) => Self::Ready(
                subclass_of
                    .subclass_of()
                    .into_dynamic()
                    .map(ClassBase::Dynamic),
            ),
            Type::Intersection(intersection) => {
                Self::Dependency(ClassBaseDependency::Intersection(intersection))
            }
            Type::Union(union) => Self::Dependency(ClassBaseDependency::Union(union)),

            // This likely means that we're in unreachable code,
            // in which case we want to treat `Never` in a forgiving way and silence diagnostics
            Type::Never => Self::Ready(Some(ClassBase::unknown())),

            Type::TypeAlias(alias) => Self::Dependency(ClassBaseDependency::TypeAlias(alias)),
            Type::NewTypeInstance(newtype) => {
                Self::Dependency(ClassBaseDependency::NewType(newtype))
            }

            Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::EnumComplement(_)
            | Type::LiteralValue(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => Self::Ready(None),

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::SubscriptedGeneric(_) => Self::Ready(Some(ClassBase::Generic)),
                KnownInstanceType::SubscriptedProtocol(_) => Self::Ready(Some(ClassBase::Protocol)),
                // A class inheriting from a newtype would make intuitive sense, but newtype
                // wrappers are just identity callables at runtime, so this sort of inheritance
                // doesn't work and isn't allowed.
                KnownInstanceType::NewType(_) => Self::Ready(None),
                KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::ConstraintSetSolution(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::UnionType(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::LiteralStringAlias(_)
                | KnownInstanceType::NamedTupleSpec(_)
                | KnownInstanceType::Sentinel(_)
                | KnownInstanceType::Range { .. }
                | KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::MethodWrapper(_)
                | KnownInstanceType::FunctoolsPartialCall(_) => Self::Ready(None),
                KnownInstanceType::TypeGenericAlias(_) => {
                    Self::Dependency(ClassBaseDependency::KnownClass(KnownClass::Type))
                }
                KnownInstanceType::Annotated(ty) => {
                    Self::Dependency(ClassBaseDependency::Annotated(ty))
                }
            },

            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::TypeQualifier(_) => Self::Ready(None),

                SpecialFormType::Annotated
                | SpecialFormType::Literal
                | SpecialFormType::LiteralString
                | SpecialFormType::Union
                | SpecialFormType::NoReturn
                | SpecialFormType::Never
                | SpecialFormType::TypeGuard
                | SpecialFormType::TypeIs
                | SpecialFormType::TypingSelf
                | SpecialFormType::Unpack
                | SpecialFormType::Concatenate
                | SpecialFormType::TypeAlias
                | SpecialFormType::Optional
                | SpecialFormType::Not
                | SpecialFormType::Top
                | SpecialFormType::Bottom
                | SpecialFormType::Intersection
                | SpecialFormType::TypeOf
                | SpecialFormType::CallableTypeOf
                | SpecialFormType::RegularCallableTypeOf
                | SpecialFormType::Divergent
                | SpecialFormType::Todo
                | SpecialFormType::AlwaysTruthy
                | SpecialFormType::AlwaysFalsy
                | SpecialFormType::TypeForm => Self::Ready(None),

                SpecialFormType::Any => Self::Ready(Some(ClassBase::Dynamic(DynamicType::Any))),
                SpecialFormType::Unknown => Self::Ready(Some(ClassBase::unknown())),
                SpecialFormType::Protocol => Self::Ready(Some(ClassBase::Protocol)),
                SpecialFormType::Generic => Self::Ready(Some(ClassBase::Generic)),
                SpecialFormType::TypedDict(module) => {
                    Self::Ready(Some(ClassBase::TypedDict(module)))
                }
                SpecialFormType::NamedTuple => Self::Dependency(ClassBaseDependency::NamedTuple),

                // TODO: Classes inheriting from `typing.Type` also have `Generic` in their MRO
                SpecialFormType::Type => {
                    Self::Dependency(ClassBaseDependency::KnownClass(KnownClass::Type))
                }
                SpecialFormType::Tuple => {
                    Self::Dependency(ClassBaseDependency::KnownClass(KnownClass::Tuple))
                }
                SpecialFormType::LegacyStdlibAlias(alias) => {
                    Self::Dependency(ClassBaseDependency::KnownClass(alias.aliased_class()))
                }
                SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                    Self::from_type(todo_type!("Support for Callable as a base class"))
                }
            },
        }
    }

    /// Preserve explicit `Any` only for the original base expression.
    pub(in crate::types) fn from_explicit_type(ty: Type<'db>) -> Self {
        if matches!(ty, Type::SpecialForm(SpecialFormType::Any)) {
            Self::Ready(Some(ClassBase::Any))
        } else {
            Self::from_type(ty)
        }
    }

    pub(in crate::types) fn resolve(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<ClassBase<'db>> {
        match self.resolve_with(db, env, subclass, &InlineConversionEffects) {
            Ok(base) => base,
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn resolve_with<E: ConversionEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
        effects: &E,
    ) -> Result<Option<ClassBase<'db>>, E::Error> {
        resolve_class_base_sync(self, env, subclass, &InlineResolution { db, effects })
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassBaseResolutionEffects)]
    pub(in crate::types) trait ClassBaseResolutionEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn dependency(&self, env: &ProgramEnvironment<'db>, subclass: Option<ClassLiteral<'db>>, dependency: ClassBaseDependency<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
    }

    #[synchronous(resolve_class_base_sync)]
    #[capabilities(effects = ClassBaseResolutionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn resolve_class_base_with<'db, E: ClassBaseResolutionEffects<'db>>(
        conversion: ClassBaseConversion<'db>,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
        effects: &E,
    ) -> Result<Option<ClassBase<'db>>, E::Error> {
        effects.checkpoint().await?;
        let base = match conversion {
            ClassBaseConversion::Ready(base) => base,
            ClassBaseConversion::Dependency(dependency) => effects.dependency(env, subclass, dependency).await?,
        };
        effects.checkpoint().await?;
        Ok(base)
    }
}

struct InlineResolution<'effects, 'db, E> {
    db: &'db dyn Db,
    effects: &'effects E,
}

impl<'db, E: ConversionEffects<'db>> SynchronousClassBaseResolutionEffects<'db>
    for InlineResolution<'_, 'db, E>
{
    type Error = E::Error;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        self.effects.check()
    }

    fn dependency(
        &self,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
        dependency: ClassBaseDependency<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        dependency.resolve_with(self.db, env, subclass, self.effects)
    }
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// Conversion keeps its dependency provider when following aliases and compound bases.
pub(in crate::types) trait ConversionEffects<'db>:
    SourceReadControl + sealed::Sealed + Sized
{
    fn default_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassBase<'db>, Self::Error>;

    fn convert_child(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        ClassBaseConversion::from_type(ty).resolve_with(db, env, subclass, self)
    }
}

struct InlineConversionEffects;

impl sealed::Sealed for InlineConversionEffects {}

impl SourceReadControl for InlineConversionEffects {
    type Error = Infallible;

    fn check(&self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl<'db> ConversionEffects<'db> for InlineConversionEffects {
    fn default_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassBase<'db>, Infallible> {
        Ok(ClassBase::Class(class.default_specialization(db)))
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ClassBaseDependency<'db> {
    UnboundRecursiveVariable,
    Recursive(RecursiveType<'db>),
    DefaultSpecialization(ClassLiteral<'db>),
    NominalInstance(NominalInstanceType<'db>),
    Intersection(IntersectionType<'db>),
    Union(UnionType<'db>),
    TypeAlias(TypeAliasType<'db>),
    NewType(NewType<'db>),
    Annotated(InternedType<'db>),
    KnownClass(KnownClass),
    NamedTuple,
}

impl<'db> ClassBaseDependency<'db> {
    #[cfg(test)]
    pub(in crate::types) fn resolve(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<ClassBase<'db>> {
        ClassBaseConversion::Dependency(self).resolve(db, env, subclass)
    }

    fn resolve_with<E: ConversionEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass: Option<ClassLiteral<'db>>,
        effects: &E,
    ) -> Result<Option<ClassBase<'db>>, E::Error> {
        Ok(match self {
            Self::UnboundRecursiveVariable => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Self::Recursive(recursive) => {
                let unfolded = read_source(effects, || recursive.unfold(db, env))?;
                let Some(unfolded) = unfolded.into_unfolded() else {
                    return Ok(None);
                };
                effects.convert_child(db, env, unfolded, subclass)?
            }
            Self::DefaultSpecialization(literal) => {
                Some(effects.default_specialization(db, literal)?)
            }
            Self::NominalInstance(instance) => {
                if read_source(effects, || {
                    instance.has_known_class(db, KnownClass::GenericAlias)
                })? {
                    effects.convert_child(db, env, todo_type!("GenericAlias instance"), subclass)?
                } else {
                    None // TODO -- handle `__mro_entries__`?
                }
            }
            Self::Intersection(intersection) => {
                let mut valid_element = None;
                for element in intersection.positive(db) {
                    valid_element = effects.convert_child(db, env, *element, subclass)?;
                    if valid_element.is_some() {
                        break;
                    }
                }
                let Some(valid_element) = valid_element else {
                    return Ok(None);
                };

                let type_instance = read_source(effects, || KnownClass::Type.to_instance(db, env))?;
                if read_source(effects, || {
                    Type::Intersection(intersection).is_disjoint_from(db, env, type_instance)
                })? {
                    None
                } else {
                    Some(valid_element)
                }
            }
            Self::Union(union) => {
                if let Some(module) = read_source(effects, || {
                    TypingModule::from_typed_dict_type(db, Type::Union(union))
                })? {
                    return Ok(Some(ClassBase::TypedDict(module)));
                }

                // We do not support full unions of MROs (yet). Until we do,
                // support the cases where one of the types in the union is
                // a dynamic type such as `Any` or `Unknown`, and all other
                // types *would be* valid class bases. In this case, we can
                // "fold" the other potential bases into the dynamic type,
                // and return `Any`/`Unknown` as the class base to prevent
                // invalid-base diagnostics and further downstream errors.
                let Some(Type::Dynamic(dynamic)) = union
                    .elements(db)
                    .iter()
                    .find(|elem| matches!(elem, Type::Dynamic(_)))
                else {
                    return Ok(None);
                };

                for element in union.elements(db) {
                    if effects
                        .convert_child(db, env, *element, subclass)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                }
                Some(ClassBase::Dynamic(*dynamic))
            }
            Self::TypeAlias(alias) => {
                let value = read_source(effects, || alias.value_type(db))?;
                effects.convert_child(db, env, value, subclass)?
            }
            Self::NewType(newtype) => {
                let base = read_source(effects, || newtype.concrete_base_type(db))?;
                effects.convert_child(db, env, base, subclass)?
            }
            Self::Annotated(ty) => match ty.inner(db) {
                Type::Dynamic(dynamic) => Some(ClassBase::Dynamic(dynamic)),
                Type::NominalInstance(instance) => {
                    Some(ClassBase::Class(read_source(effects, || {
                        instance.class(db, env)
                    })?))
                }
                _ => None,
            },
            Self::KnownClass(known_class) => {
                let class = read_source(effects, || known_class.to_class_literal(db, env))?;
                effects.convert_child(db, env, class, subclass)?
            }
            Self::NamedTuple => {
                let Some(class) = subclass.and_then(ClassLiteral::as_static) else {
                    return Ok(None);
                };
                let fields = read_source(effects, || {
                    class.own_fields(db, None, CodeGeneratorKind::NamedTuple)
                })?;
                let tuple = read_source(effects, || {
                    TupleType::heterogeneous(
                        db,
                        env,
                        fields.values().map(|field| field.declared_ty),
                    )
                })?;
                let class_type = read_source(effects, || tuple.to_class_type(db))?;
                effects.convert_child(db, env, class_type.into(), subclass)?
            }
        })
    }
}
