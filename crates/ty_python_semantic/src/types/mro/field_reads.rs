use ty_python_core::scope::ScopeId;

use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::{
    ClassLiteral, ClassType, DataclassFlags, DataclassParams, GenericAlias, KnownClass,
    SpecialFormType, StaticClassLiteral, Type, TypingModule,
};

pub(in crate::types) enum MroIdentity<'db> {
    Type(Type<'db>),
    GenericAlias(GenericAlias<'db>),
}

impl<'db> MroIdentity<'db> {
    pub(in crate::types) fn of(base: ClassBase<'db>) -> Self {
        match base {
            ClassBase::Any => Self::Type(Type::SpecialForm(SpecialFormType::Any)),
            ClassBase::Class(ClassType::NonGeneric(class)) => Self::Type(Type::ClassLiteral(class)),
            ClassBase::Class(ClassType::Generic(alias)) => Self::GenericAlias(alias),
            ClassBase::TypedDict(_) => Self::Type(Type::SpecialForm(SpecialFormType::TypedDict(
                TypingModule::Typing,
            ))),
            _ => Self::Type(base.into()),
        }
    }
}

/// Reads the class identities used by MRO construction without query or constructor access.
#[derive(Clone, Copy)]
pub(in crate::types) struct MroFieldReads<'db> {
    fields: salsa::FieldReads<'db>,
}

impl<'db> MroFieldReads<'db> {
    pub(in crate::types) fn new(db: &'db dyn salsa::Database) -> Self {
        Self {
            fields: salsa::FieldReads::new(db),
        }
    }

    pub(in crate::types) fn alias_origin(
        self,
        alias: GenericAlias<'db>,
    ) -> StaticClassLiteral<'db> {
        *alias.read_fields(self.fields).origin()
    }

    pub(in crate::types) fn alias_specialization(
        self,
        alias: GenericAlias<'db>,
    ) -> Specialization<'db> {
        *alias.read_fields(self.fields).specialization()
    }

    pub(in crate::types) fn class_literal(self, class: ClassType<'db>) -> ClassLiteral<'db> {
        match class {
            ClassType::NonGeneric(literal) => literal,
            ClassType::Generic(alias) => ClassLiteral::Static(self.alias_origin(alias)),
        }
    }

    pub(in crate::types) fn static_class_literal(
        self,
        class: ClassType<'db>,
    ) -> Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)> {
        match class {
            ClassType::NonGeneric(ClassLiteral::Static(class)) => Some((class, None)),
            ClassType::NonGeneric(_) => None,
            ClassType::Generic(alias) => Some((
                self.alias_origin(alias),
                Some(self.alias_specialization(alias)),
            )),
        }
    }

    pub(in crate::types) fn body_scope(self, class: StaticClassLiteral<'db>) -> ScopeId<'db> {
        *class.read_fields(self.fields).body_scope()
    }

    pub(in crate::types) fn is_object(self, class: ClassType<'db>) -> bool {
        self.class_literal(class).as_static().is_some_and(|class| {
            *class.read_fields(self.fields).known() == Some(KnownClass::Object)
        })
    }

    pub(in crate::types) fn mro_identity(self, base: ClassBase<'db>) -> Type<'db> {
        match MroIdentity::of(base) {
            MroIdentity::Type(ty) => ty,
            MroIdentity::GenericAlias(alias) => Type::ClassLiteral(self.alias_origin(alias).into()),
        }
    }
    pub(in crate::types) fn static_known(
        self,
        class: StaticClassLiteral<'db>,
    ) -> Option<KnownClass> {
        *class.read_fields(self.fields).known()
    }
    pub(in crate::types) fn dataclass_params(
        self,
        class: StaticClassLiteral<'db>,
    ) -> Option<crate::types::DataclassParams<'db>> {
        *class.read_fields(self.fields).dataclass_params()
    }
    pub(in crate::types) fn has_explicit_bases(self, class: StaticClassLiteral<'db>) -> bool {
        *class.read_fields(self.fields).has_explicit_bases()
    }
    pub(in crate::types) fn has_type_params(self, class: StaticClassLiteral<'db>) -> bool {
        *class.read_fields(self.fields).has_type_params()
    }
    pub(in crate::types) fn has_explicit_metaclass(self, class: StaticClassLiteral<'db>) -> bool {
        *class.read_fields(self.fields).has_explicit_metaclass()
    }
    pub(in crate::types) fn typed_dict_without_inference(
        self,
        class: StaticClassLiteral<'db>,
    ) -> Option<bool> {
        self.static_known(class)
            .map(KnownClass::is_typed_dict_subclass)
            .or_else(|| (!self.has_explicit_bases(class)).then_some(false))
    }
    pub(in crate::types) fn dataclass_flags(self, params: DataclassParams<'db>) -> DataclassFlags {
        *params.read_fields(self.fields).flags()
    }
    pub(in crate::types) fn has_decorators(self, class: StaticClassLiteral<'db>) -> bool {
        *class.read_fields(self.fields).has_decorators()
    }
}
