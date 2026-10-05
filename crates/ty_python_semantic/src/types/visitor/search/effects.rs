//! Typed children and stored-field access for the shared type walk.

use std::collections::btree_map;
use std::convert::Infallible;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use super::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableSignature, CallableType,
    EnumComplementType, FieldInstance, FunctionType, FunctoolsPartialInstance, FxOrderSet,
    GenericAlias, GenericContext, InternedConstraintSetSolution, InternedType, IntersectionType,
    MethodWrapper, NamedTupleField, NamedTupleSpec, NegativeIntersectionElements,
    NominalVisitorChildren, NonAtomicType, PropertyInstanceClass, PropertyInstanceType,
    ProtocolVisitorChildren, SearchControl, SearchWork, SlotDescriptorType, Specialization,
    StoredTypeSequence, SyncTypeDepthEffects, SyncTypeSearchEffects, SyncTypeWalkEffects,
    SynthesizedTypedDictType, TypeCollector, TypeFormType, TypeGuardType, TypeIsType,
    TypeSearchDecision, TypeSearchDescent, TypeVarConstraints, TypeVarSolution, TypeWalkCursor, TypeWalkEvent, TypeWalkFacts,
    TypeWalkPolicy, TypedDictOpenness, UnionType, UnionTypeInstance, Unrestricted, WalkAction,
    enter_depth_active_with, leave_depth_active_with, reserve_walk_pending_with,
};
use crate::types::constraints::OwnedConstraintTypeCursor;
use crate::types::constraints::control::{Unrestricted as UnrestrictedCollections, unrestricted};
use crate::types::newtype::{NewType, NewTypeBase};
use crate::types::protocol_class::{ProtocolInterfaceView, ProtocolMember, ProtocolMemberData};
use crate::types::tuple::TupleSpec;
use crate::types::typed_dict::TypedDictSchema;
use crate::types::typevar::{TypeVarBoundOrConstraints, TypeVarInstance};
use crate::types::{
    ClassType, KnownBoundMethodType, KnownInstanceType, NominalInstanceType, ProtocolInstanceType,
    RecursiveType, Type, TypeAliasType, TypedDictType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) enum TypeWalkWork {
    Search(SearchWork),
    DepthVisit,
    DepthEnter,
    DepthExit,
}

pub(in crate::types) struct OrdinaryTypeWalk<'control, 'env, 'db, C, Q> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
    pub(in crate::types) control: &'control mut C,
    pub(in crate::types) query: Q,
}
impl<'db, C: SearchControl, Q> SyncTypeWalkEffects<'db> for OrdinaryTypeWalk<'_, '_, 'db, C, Q> {
    type Error = C::Error;
    fn union_elements(&mut self, ty: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).elements())
    }
    fn intersection_positive(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).positive())
    }
    fn intersection_negative(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).negative())
    }
    fn enum_rest(
        &mut self,
        ty: EnumComplementType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).rest())
    }
    fn function_signature(
        &mut self,
        ty: FunctionType<'db>,
    ) -> Result<Option<&'db CallableSignature<'db>>, Self::Error> {
        Ok(ty.updated_signature_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn function_implementations(
        &mut self,
        ty: FunctionType<'db>,
    ) -> Result<Option<&'db [CallableType<'db>]>, Self::Error> {
        Ok(ty.updated_implementation_callables_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn callable_signatures(
        &mut self,
        ty: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).signatures())
    }
    fn method_func(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).func())
    }
    fn method_self(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.self_instance_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn method_receiver(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.signature_receiver_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn bound_super_children(
        &mut self,
        ty: BoundSuperType<'db>,
    ) -> Result<[Option<Type<'db>>; 3], Self::Error> {
        Ok(ty.children_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn alias_specialization(
        &mut self,
        ty: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .specialization())
    }
    fn specialization_context(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .generic_context())
    }
    fn specialization_types(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).types())
    }
    fn specialization_tuple(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(ty.tuple_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn context_variable(
        &mut self,
        ty: GenericContext<'db>,
        index: usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(ty.variable_at_with_fields(salsa::FieldReads::new(self.db), index))
    }
    fn bound_typevar(
        &mut self,
        ty: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).typevar())
    }
    fn eager_typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<(Option<TypeVarBoundOrConstraints<'db>>, bool), Self::Error> {
        Ok(ty.eager_bounds_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn eager_typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<(Option<Type<'db>>, bool), Self::Error> {
        Ok(ty.eager_default_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn constraint_elements(
        &mut self,
        ty: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).elements())
    }
    fn nominal_children(
        &mut self,
        ty: NominalInstanceType<'db>,
    ) -> Result<NominalVisitorChildren<'db>, Self::Error> {
        Ok(ty.children_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn protocol_children(
        &mut self,
        ty: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolVisitorChildren<'db>, Self::Error> {
        Ok(ty.children_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn property_class(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<PropertyInstanceClass<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .instance_class())
    }
    fn property_getter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).getter())
    }
    fn property_setter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).setter())
    }
    fn property_deleter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).deleter())
    }
    fn slot_value(&mut self, ty: SlotDescriptorType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).value_type())
    }
    fn type_is_argument(&mut self, ty: TypeIsType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .type_argument())
    }
    fn type_guard_return(&mut self, ty: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .return_type())
    }
    fn type_form_argument(&mut self, ty: TypeFormType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .type_argument())
    }
    fn alias_arguments(
        &mut self,
        ty: TypeAliasType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        Ok(ty.specialization_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn recursive_arguments(
        &mut self,
        ty: RecursiveType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).arguments())
    }
    fn interface_members(
        &mut self,
        ty: ProtocolInterfaceView<'db>,
    ) -> Result<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>, Self::Error> {
        Ok(ty.members_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn synthesized_typed_dict_items(
        &mut self,
        ty: SynthesizedTypedDictType<'db>,
    ) -> Result<&'db TypedDictSchema<'db>, Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).items())
    }
    fn synthesized_typed_dict_openness(
        &mut self,
        ty: SynthesizedTypedDictType<'db>,
    ) -> Result<TypedDictOpenness<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).openness())
    }
    fn eager_newtype_base(
        &mut self,
        ty: NewType<'db>,
    ) -> Result<Option<NewTypeBase<'db>>, Self::Error> {
        Ok(ty.eager_base_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn solution_bindings(
        &mut self,
        ty: InternedConstraintSetSolution<'db>,
    ) -> Result<&'db [TypeVarSolution<'db>], Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).bindings())
    }
    fn field_default(&mut self, ty: FieldInstance<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(*ty
            .read_fields(salsa::FieldReads::new(self.db))
            .default_type())
    }
    fn field_converter(
        &mut self,
        ty: FieldInstance<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).converter())
    }
    fn union_value(
        &mut self,
        ty: UnionTypeInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty
            .read_fields(salsa::FieldReads::new(self.db))
            .union_type()
            .as_ref()
            .ok()
            .copied())
    }
    fn interned_type(&mut self, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).inner())
    }
    fn named_tuple_fields(
        &mut self,
        ty: NamedTupleSpec<'db>,
    ) -> Result<&'db [NamedTupleField<'db>], Self::Error> {
        Ok(ty.read_fields(salsa::FieldReads::new(self.db)).fields())
    }
    fn partial_callable(
        &mut self,
        ty: FunctoolsPartialInstance<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).partial())
    }
    fn method_wrapper_type(&mut self, ty: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(*ty.read_fields(salsa::FieldReads::new(self.db)).wrapped())
    }
    fn remember_type(
        &mut self,
        seen: &mut TypeCollector<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(unrestricted(seen.type_was_already_seen_with(
            ty,
            &mut UnrestrictedCollections,
        )))
    }

    fn push_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_action_sync(cursor, action, TypeWalkFacts, self)
    }
    fn push_visit(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_visit_sync(cursor, ty, TypeWalkFacts, self)
    }
    fn push_tuple(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        tuple: &'db TupleSpec<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_tuple_sync(cursor, tuple, TypeWalkFacts, self)
    }
    fn expand_children(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        kind: NonAtomicType<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<(), Self::Error> {
        super::expand_type_children_sync(cursor, kind, policy, TypeWalkFacts, self)
    }
    fn expand_wrapper(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        wrapper: KnownBoundMethodType<'db>,
    ) -> Result<(), Self::Error> {
        super::expand_method_wrapper_children_sync(cursor, wrapper, TypeWalkFacts, self)
    }
    fn expand_known(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        known: KnownInstanceType<'db>,
    ) -> Result<(), Self::Error> {
        super::expand_known_instance_children_sync(cursor, known, TypeWalkFacts, self)
    }
    fn next_event(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<Option<TypeWalkEvent<'db>>, Self::Error> {
        super::next_type_walk_event_sync(cursor, policy, TypeWalkFacts, self)
    }
    fn checkpoint(&mut self, work: TypeWalkWork) -> Result<(), Self::Error> {
        if let TypeWalkWork::Search(work) = work {
            self.control.admit(work)?;
        }
        Ok(())
    }
    fn take_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
    ) -> Result<Option<WalkAction<'db>>, Self::Error> {
        if cursor.pending.is_empty() {
            return Ok(None);
        }
        self.control.admit(SearchWork::Advance)?;
        Ok(cursor.pending.pop())
    }
    fn enqueue(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Self::Error> {
        self.control.admit(SearchWork::PendingFrame {
            held: cursor.pending.len(),
        })?;
        unrestricted(reserve_walk_pending_with(
            cursor,
            1,
            &mut UnrestrictedCollections,
        ));
        cursor.pending.push(action);
        Ok(())
    }
    fn enqueue_visits<const N: usize>(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        children: [Option<Type<'db>>; N],
    ) -> Result<(), Self::Error> {
        for ty in children.into_iter().rev().flatten() {
            self.enqueue(cursor, WalkAction::Visit(ty))?;
        }
        Ok(())
    }
    fn next_stored(
        &mut self,
        types: &mut StoredTypeSequence<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(types.next_type())
    }
    fn next_member(
        &mut self,
        members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> Result<Option<(&'db Name, &'db ProtocolMemberData<'db>)>, Self::Error> {
        Ok(members.next())
    }
    fn constraint_type_step(
        &mut self,
        cursor: &mut OwnedConstraintTypeCursor<'db, 'db>,
    ) -> Result<Option<Option<[Type<'db>; 2]>>, Self::Error> {
        Ok(unrestricted(cursor.next_with(&mut UnrestrictedCollections)))
    }
    fn protocol_interface(
        &mut self,
        ty: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error> {
        Ok(ty.interface(self.db))
    }
    fn protocol_member_types(
        &mut self,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<[Option<Type<'db>>; 6], Self::Error> {
        Ok(member.types_for_visitor_array(self.db, self.env))
    }
    fn typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(ty.bounds_for_visitor(self.db, self.env, true).0)
    }
    fn typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty.default_for_visitor(self.db, self.env, true).0)
    }
    fn alias_value(&mut self, ty: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.value_type(self.db))
    }
    fn recursive_unfold(&mut self, ty: RecursiveType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.unfold(self.db, self.env).into_type())
    }
    fn typed_dict_items(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<&'db TypedDictSchema<'db>, Self::Error> {
        Ok(ty.items(self.db))
    }
    fn typed_dict_extra(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty
            .explicit_extra_items(self.db)
            .map(|extra| extra.declared_ty))
    }
    fn newtype_base(&mut self, ty: NewType<'db>) -> Result<NewTypeBase<'db>, Self::Error> {
        Ok(ty.base(self.db))
    }
    fn newtype_instance(&mut self, base: NewTypeBase<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(base.instance_type(self.db, self.env))
    }
}
impl<'db, T, C: SearchControl, Q: Fn(Type<'db>) -> T> SyncTypeSearchEffects<'db, T>
    for OrdinaryTypeWalk<'_, '_, 'db, C, Q>
{
    fn new_state(&mut self) -> Result<(TypeWalkCursor<'db>, TypeCollector<'db>), Self::Error> {
        Ok((TypeWalkFacts.empty_cursor(), TypeWalkFacts.empty_seen()))
    }

    fn predicate(&mut self, ty: Type<'db>) -> Result<T, Self::Error> {
        Ok((self.query)(ty))
    }

    fn decide_visit(
        &mut self,
        ty: Type<'db>,
        policy: TypeWalkPolicy,
        found: T,
    ) -> Result<TypeSearchDecision<'db, T>, Self::Error>
    where
        T: Copy + Default + PartialEq,
    {
        super::decide_type_search_visit_sync(ty, policy, found, TypeWalkFacts, self)
    }

    fn schedule_descent(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        seen: &mut TypeCollector<'db>,
        descent: TypeSearchDescent<'db>,
    ) -> Result<(), Self::Error> {
        super::schedule_type_search_descent_sync::<T, _>(cursor, seen, descent, TypeWalkFacts, self)
    }
}
impl<'db> SyncTypeDepthEffects<'db> for OrdinaryTypeWalk<'_, '_, 'db, Unrestricted, ()> {
    fn nominal_class(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(instance.class(self.db, self.env))
    }
    fn enter_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(unrestricted(enter_depth_active_with(
            active,
            ty,
            &mut UnrestrictedCollections,
        )))
    }
    fn leave_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        unrestricted(leave_depth_active_with(
            active,
            ty,
            &mut UnrestrictedCollections,
        ));
        Ok(())
    }
}
