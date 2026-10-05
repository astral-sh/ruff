use std::collections::btree_map;

use ruff_python_ast::name::Name;

use crate::Program;
use crate::types::generics::Specialization;
#[cfg(test)]
use crate::types::instance::ProtocolInterfaceSource;
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
};
use crate::types::protocol_class::{ProtocolClass, ProtocolInterfaceView, ProtocolMemberData};
use crate::types::{
    CallableType, ClassLiteral, ClassType, GenericAlias, KnownClass, MaterializationKind,
    NominalInstanceType, ProtocolInstanceType, StaticClassLiteral, Type, TypeFormType,
    TypeGuardType, TypeIsType,
};

/// Reads existing handles without exposing query or constructor access to the caller.
#[derive(Clone, Copy)]
pub(in crate::types) struct RelationFieldReads<'db> {
    fields: salsa::FieldReads<'db>,
}

impl<'db> RelationFieldReads<'db> {
    pub(in crate::types) fn new(db: &'db dyn salsa::Database) -> Self {
        Self {
            fields: salsa::FieldReads::new(db),
        }
    }

    pub(super) fn type_form_argument(self, value: TypeFormType<'db>) -> Type<'db> {
        *value.read_fields(self.fields).type_argument()
    }
    pub(super) fn field_default(self, value: FieldInstance<'db>) -> Option<Type<'db>> {
        *value.read_fields(self.fields).default_type()
    }
    pub(super) fn field_converter(
        self,
        value: FieldInstance<'db>,
    ) -> Option<(Type<'db>, Type<'db>)> {
        *value.read_fields(self.fields).converter()
    }
    pub(super) fn method_wrapper_kind(self, value: MethodWrapper<'db>) -> MethodWrapperKind {
        *value.read_fields(self.fields).kind()
    }
    pub(super) fn method_wrapper_type(self, value: MethodWrapper<'db>) -> Type<'db> {
        *value.read_fields(self.fields).wrapped()
    }
    pub(super) fn partial_wrapped(self, value: FunctoolsPartialInstance<'db>) -> InternedType<'db> {
        *value.read_fields(self.fields).wrapped()
    }
    pub(super) fn partial_callable(
        self,
        value: FunctoolsPartialInstance<'db>,
    ) -> CallableType<'db> {
        *value.read_fields(self.fields).partial()
    }
    pub(super) fn interned_type(self, value: InternedType<'db>) -> Type<'db> {
        *value.read_fields(self.fields).inner()
    }
    pub(super) fn type_is_argument(self, value: TypeIsType<'db>) -> Type<'db> {
        *value.read_fields(self.fields).type_argument()
    }
    pub(super) fn type_guard_return(self, value: TypeGuardType<'db>) -> Type<'db> {
        *value.read_fields(self.fields).return_type()
    }

    pub(in crate::types) fn protocol_interface_includes_member(
        self,
        interface: ProtocolInterfaceView<'db>,
        name: &str,
    ) -> bool {
        interface.includes_member_with_fields(self.fields, name)
    }

    pub(in crate::types) fn protocol_interface_program(
        self,
        interface: ProtocolInterfaceView<'db>,
    ) -> Program<'db> {
        *interface.base().read_fields(self.fields).program()
    }

    pub(in crate::types) fn protocol_class_origin(
        self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Option<ProtocolClass<'db>> {
        protocol.class_origin_with_fields(self.fields)
    }

    #[cfg(test)]
    pub(in crate::types) fn protocol_interface_source(
        self,
        protocol: ProtocolInstanceType<'db>,
    ) -> ProtocolInterfaceSource<'db> {
        protocol.interface_source_with_fields(self.fields)
    }

    pub(in crate::types) fn protocol_materialization_kind(
        self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Option<MaterializationKind> {
        protocol.materialization_kind_with_fields(self.fields)
    }

    pub(in crate::types) fn protocol_nominal_origin_instance(
        self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Option<NominalInstanceType<'db>> {
        protocol.nominal_origin_instance_with_fields(self.fields)
    }

    pub(in crate::types) fn nominal_has_known_class(
        self,
        nominal: NominalInstanceType<'db>,
        known: KnownClass,
    ) -> bool {
        nominal.known_class_with_fields(self.fields) == Some(known)
    }

    pub(in crate::types) fn class_literal(self, class: ClassType<'db>) -> ClassLiteral<'db> {
        match class {
            ClassType::NonGeneric(literal) => literal,
            ClassType::Generic(alias) => self.alias_origin(alias).into(),
        }
    }

    pub(in crate::types) fn protocol_interface_member_count(
        self,
        interface: ProtocolInterfaceView<'db>,
    ) -> usize {
        interface.member_count_with_fields(self.fields)
    }

    pub(in crate::types) fn protocol_interface_members(
        self,
        interface: ProtocolInterfaceView<'db>,
    ) -> btree_map::Iter<'db, Name, ProtocolMemberData<'db>> {
        interface.members_with_fields(self.fields)
    }

    pub(in crate::types) fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> StaticClassLiteral<'db> {
        *alias.read_fields(self.fields).origin()
    }

    pub(in crate::types) fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Specialization<'db> {
        *alias.read_fields(self.fields).specialization()
    }

    pub(in crate::types) fn specialization_types(
        &self,
        value: Specialization<'db>,
    ) -> &'db [Type<'db>] {
        value.read_fields(self.fields).types().as_ref()
    }
}
