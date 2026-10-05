use std::fmt::Display;

use crate::ProgramEnvironment;
use crate::types::class::ClassMetaclass;
use crate::types::generics::Specialization;
use crate::types::mapping::effects::{
    InlineMappingEffects, MappingEffects, SynchronousMappingEffects, inline_mapping_result,
};
use crate::types::mro::base::{InlineBaseMroEffects, base_mro_start_sync};
use crate::types::mro::construction::{InlineStaticMroEffects, base_has_cyclic_mro_sync};
use crate::types::{
    ClassLiteral, ClassType, DivergentType, DynamicType, SpecialFormType, Type,
    TypingModule,
};
use crate::{Db, DisplaySettings};

pub(super) mod conversion;
pub(in crate::types) mod metaclass;
pub(in crate::types) mod specialization;

pub(super) use conversion::ClassBaseConversion;
#[cfg(test)]
pub(super) use conversion::ClassBaseDependency;

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum: all types that would be invalid to have as a
/// class base are transformed into [`ClassBase::unknown()`]
///
/// Note that a non-specialized generic class _cannot_ be a class base. When we see a
/// non-specialized generic class in any type expression (including the list of base classes), we
/// automatically construct the default specialization for that class.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum ClassBase<'db> {
    /// The `Any` special form used directly as a base class.
    ///
    /// This is distinct from [`ClassBase::Dynamic`] because a base expression whose inferred type
    /// is `Any` does not give the class the same gradual assignability as an explicit `Any` base.
    Any,
    Dynamic(DynamicType<'db>),
    Divergent(DivergentType),
    Class(ClassType<'db>),
    /// Although `Protocol` is not a class in typeshed's stubs, it is at runtime,
    /// and can appear in the MRO of a class.
    Protocol,
    /// Bare `Generic` cannot be subclassed directly in user code,
    /// but nonetheless appears in the MRO of classes that inherit from `Generic[T]`,
    /// `Protocol[T]`, or bare `Protocol`.
    Generic,
    TypedDict(TypingModule),
}

impl<'db> ClassBase<'db> {
    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Dynamic(dynamic) => Some(Self::Dynamic(dynamic.recursive_type_normalized())),
            Self::Divergent(_) => Some(self),
            Self::Class(class) => Some(Self::Class(
                class.recursive_type_normalized_impl(db, env, div, nested)?,
            )),
            Self::Any | Self::Protocol | Self::Generic | Self::TypedDict(_) => Some(self),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            ClassBase::Any => "Any",
            ClassBase::Class(class) => class.name(db),
            ClassBase::Dynamic(DynamicType::Any) => "Any",
            ClassBase::Dynamic(
                DynamicType::Unknown
                | DynamicType::UnknownGeneric(_)
                | DynamicType::UnknownLambdaParameter
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::AmbiguousOverload,
            ) => "Unknown",
            ClassBase::Dynamic(DynamicType::UnspecializedTypeVar) => "UnspecializedTypeVar",
            ClassBase::Dynamic(DynamicType::Todo(_)) => "@Todo",
            ClassBase::Divergent(_) => "Divergent",
            ClassBase::Protocol => "Protocol",
            ClassBase::Generic => "Generic",
            ClassBase::TypedDict(_) => "TypedDict",
        }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self::Class(ClassType::object(db, env))
    }

    pub(super) const fn is_typed_dict(self) -> bool {
        self.typed_dict_module().is_some()
    }

    pub(super) const fn typed_dict_module(self) -> Option<TypingModule> {
        match self {
            ClassBase::TypedDict(module) => Some(module),
            _ => None,
        }
    }

    /// Return the identity of this base for method-resolution-order construction.
    ///
    /// Specializations of a generic class share one runtime class and must occupy the same MRO
    /// entry. Keep the specialization on the original `ClassBase` for member lookup.
    ///
    /// The `TypedDict` module affects member lookup, but both special forms represent the same
    /// pseudo-base when detecting duplicate or conflicting bases. An explicit `Any` base remains
    /// distinct from a base expression whose type is `Any`.
    pub(super) fn mro_identity(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Any => Type::SpecialForm(SpecialFormType::Any),
            Self::Class(class) => Type::ClassLiteral(class.class_literal(db)),
            Self::TypedDict(_) => {
                Type::SpecialForm(SpecialFormType::TypedDict(TypingModule::Typing))
            }
            _ => self.into(),
        }
    }

    /// Return whether this is an explicit `Any` base.
    pub(super) const fn is_explicit_any_base(self) -> bool {
        matches!(self, ClassBase::Any)
    }

    /// Convert an explicit base while preserving a direct use of the `Any` special form.
    pub(super) fn try_from_explicit_base(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<Self> {
        ClassBaseConversion::from_explicit_type(ty).resolve(db, env, subclass)
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_type(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<Self> {
        ClassBaseConversion::from_type(ty).resolve(db, env, subclass)
    }

    pub(super) fn into_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Any
            | Self::Dynamic(_)
            | Self::Divergent(_)
            | Self::Generic
            | Self::Protocol
            | Self::TypedDict(_) => None,
        }
    }

    /// Return this base's selected metaclass or inferred protocol fallback.
    ///
    /// `subclass` is the class whose declaration names this base. Only a direct `Protocol` base
    /// depends on whether its declaration is a stub resolved through a standard-library search
    /// path; named bases retain their own metaclass constraints or fallback wherever they are inherited.
    pub(super) fn inferred_metaclass(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass: ClassLiteral<'db>,
    ) -> ClassMetaclass<'db> {
        match metaclass::class_base_metaclass_sync(
            self,
            env,
            subclass,
            &metaclass::InlineClassBaseMetaclassEffects(db),
        ) {
            Ok(metaclass) => metaclass,
            Err(never) => match never {},
        }
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        inline_mapping_result(self.apply_optional_specialization_sync(
            db,
            specialization,
            &InlineMappingEffects,
        ))
    }

    pub(crate) async fn apply_optional_specialization_with<E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        specialization::apply_optional_base_specialization_with(
            db,
            self,
            specialization,
            &specialization::MappingClassBaseEffects(effects),
        )
        .await
    }

    pub(crate) fn apply_optional_specialization_sync<E: SynchronousMappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        specialization::apply_optional_base_specialization_sync(
            db,
            self,
            specialization,
            &specialization::MappingClassBaseEffects(effects),
        )
    }

    pub(super) fn has_cyclic_mro(self, db: &'db dyn Db) -> bool {
        match base_has_cyclic_mro_sync(db, self, &InlineStaticMroEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        additional_specialization: Option<Specialization<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> + Clone {
        let start = match base_mro_start_sync(
            db,
            env,
            self,
            additional_specialization,
            &InlineBaseMroEffects::new(db),
        ) {
            Ok(start) => start,
            Err(never) => match never {},
        };
        start.into_iter(db)
    }

    pub(super) fn display(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl std::fmt::Display {
        self.display_with(db, env, DisplaySettings::default())
    }

    pub(super) fn display_with<'env>(
        self,
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
        display_settings: DisplaySettings<'db>,
    ) -> impl Display + 'env {
        std::fmt::from_fn(move |f| match self {
            ClassBase::Any => f.write_str("Any"),
            ClassBase::Dynamic(dynamic) => dynamic.fmt(f),
            ClassBase::Divergent(_) => f.write_str("Divergent"),
            ClassBase::Class(class) => Type::from(class)
                .display_with(db, env, display_settings.clone())
                .fmt(f),
            ClassBase::Protocol => f.write_str("typing.Protocol"),
            ClassBase::Generic => f.write_str("typing.Generic"),
            ClassBase::TypedDict(_) => f.write_str("typing.TypedDict"),
        })
    }
}

impl<'db> From<ClassType<'db>> for ClassBase<'db> {
    fn from(value: ClassType<'db>) -> Self {
        ClassBase::Class(value)
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Any => Type::Dynamic(DynamicType::Any),
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Divergent(divergent) => Type::Divergent(divergent),
            ClassBase::Class(class) => class.into(),
            ClassBase::Protocol => Type::SpecialForm(SpecialFormType::Protocol),
            ClassBase::Generic => Type::SpecialForm(SpecialFormType::Generic),
            ClassBase::TypedDict(module) => Type::SpecialForm(SpecialFormType::TypedDict(module)),
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}
