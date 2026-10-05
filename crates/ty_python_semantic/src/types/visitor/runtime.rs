//! Type walks borrow the caller's execution endpoint for traversal and temporary storage.

mod facts;
mod receiver;
pub(in crate::types) mod storage;

use std::collections::btree_map;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    BorrowOrCopy, ExecutionWork, FieldReadProfile, FieldRequest, FieldRequestContext, FieldReturnMode, NativeValueQuote,
    RunError, RunResult, TaskEndpoint,
};

use crate::types::class::{NamedTupleField, NamedTupleSpec};
use crate::types::constraints::control::TddError;
use crate::types::constraints::control::attempt::{EndpointAdmission, ExecutionControl};
use crate::types::constraints::{OwnedConstraintTypeCursor, TypeVarSolution};
use crate::types::function::FunctionType;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::infer::local_with_fixed_transfers_at;
use crate::types::local_transfer::{
    boxed_future_with_fixed_transfers_at, generated_field_quote,
    local_quoted_with_fixed_transfers_at,
};
use crate::types::local_transfer::collections::CALL_1;
use crate::types::local_transfer::context_variables::context_variable_at_quote;
use crate::types::instance::{
    MaterializedProtocolType, NominalInstanceClass, NominalVisitorChildren, NominalVisitorKind,
    ProtocolVisitorChildren,
};
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedConstraintSetSolution, InternedType,
    MethodWrapper, UnionTypeInstance,
};
use crate::types::method::BoundMethodReceiver;
use crate::types::newtype::{NewType, NewTypeBase};
use crate::types::protocol_class::{
    ProtocolClass, ProtocolInterface, ProtocolInterfaceView, ProtocolMember, ProtocolMemberData,
};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::tuple::TupleSpec;
use crate::types::typed_dict::{SynthesizedTypedDictType, TypedDictOpenness, TypedDictSchema};
use crate::types::typevar::{
    TypeVarBoundOrConstraints, TypeVarConstraints, TypeVarInstance, decode_eager_bounds,
    decode_eager_default,
};
use crate::types::visitor::{
    NonAtomicType, SearchOperation, SearchWork, StoredTypeSequence, TypeCollector,
    TypeSearchEffects, TypeWalkCursor, TypeWalkEffects, TypeWalkEvent, TypeWalkFacts,
    TypeWalkFieldOperation, TypeWalkPolicy, TypeWalkWork, WalkAction,
    expand_known_instance_children_with, expand_method_wrapper_children_with,
    expand_type_children_with, next_type_walk_event_with, push_type_walk_action_with,
    push_type_walk_tuple_with, push_type_walk_visit_with, reserve_walk_pending_with,
};
use crate::types::visitor::search::{
    TypeSearchDecision, TypeSearchDescent, decide_type_search_visit_with,
    schedule_type_search_descent_with,
};
#[cfg(feature = "experimental-analysis")]
use crate::types::visitor::{TypeSearchMode, search_type_with};
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableSignature, CallableType,
    EnumComplementType, GenericAlias, IntersectionType, KnownBoundMethodType, KnownInstanceType,
    NominalInstanceType, PropertyInstanceClass, PropertyInstanceType, ProtocolInstanceType,
    RecursiveType, SlotDescriptorType, Type, TypeAliasType, TypeFormType, TypeGuardType,
    TypeIsType, TypedDictType, UnionType,
};
use crate::{Db, FxOrderSet};

pub(in crate::types) struct TypeSliceDeref;

impl<'types> FieldReadProfile<Box<[Type<'types>]>> for TypeSliceDeref {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call Box<[Type<'types>]>,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        local_with_fixed_transfers_at(endpoint, 2, 0, || {
                if mode != FieldReturnMode::Deref {
                    return Err(RunError::Contract(
                        "type slice field conversion is not a dereference",
                    ));
                }
                // Box's slice dereference borrows the stored pointer and length without traversing
                // the elements or creating ownership that needs cleanup.
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<&[Type<'types>]>(),
                    cleanup_work: 0,
                })
            })
            .await?
    }
}

pub(in crate::types) trait TypeSearchUnavailable {
    async fn unavailable<T>(
        &self,
        db: &dyn Db,
        endpoint: &TaskEndpoint<'_, '_>,
        operation: SearchOperation,
    ) -> RunResult<T>;

    fn error(&self, db: &dyn Db, error: TddError<RunError>) -> RunError;
}

pub(in crate::types) trait RuntimeTypeSearch<'db> {
    fn predicate(&self, ty: Type<'db>) -> bool;
}

/// Supplies an admitted predicate for the shared type search.
/// The search retains the first result that differs from `R::default()`.
pub(in crate::types) trait RuntimeTypeSearchWith<'run, 'db: 'run, R = bool> {
    async fn predicate(&self, endpoint: &TaskEndpoint<'run, 'db>, ty: Type<'db>)
    -> RunResult<R>;
}

impl<'run, 'db: 'run, Q: RuntimeTypeSearch<'db>> RuntimeTypeSearchWith<'run, 'db> for Q {
    async fn predicate(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        local_with_fixed_transfers_at(endpoint, CALL_1 + 4, 0, || {
            RuntimeTypeSearch::predicate(self, ty)
        }).await
    }
}

pub(in crate::types) struct RuntimeTypeWalk<'call, 'run, 'db, Q, U> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) endpoint: &'call TaskEndpoint<'run, 'db>,
    pub(in crate::types) query: Q,
    pub(in crate::types) unavailable: U,
}

impl<'db, Q, U: TypeSearchUnavailable> RuntimeTypeWalk<'_, '_, 'db, Q, U> {
    /// Reads the stored class origin without applying the protocol's pending materialization.
    pub(in crate::types) async fn materialized_protocol_origin(
        &self,
        materialized: MaterializedProtocolType<'db>,
    ) -> RunResult<ProtocolClass<'db>> {
        self.field(materialized, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).origin())
        .await
    }

    /// Borrows stored protocol members in name order and admits construction of their iterator.
    pub(in crate::types) async fn stored_protocol_members(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> RunResult<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>> {
        let members = self
            .field(interface, |handle, context| handle.field_requests(context), |handle, context| handle.members_request(context))
            .await?;
        let action = || members.iter();
        let requested_bytes =
            size_of::<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>>()
                .checked_add(size_of_val(&action))
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<
                        Option<RunResult<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>>>,
                    >())
                })
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<
                        RunResult<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>>,
                    >())
                })
                .ok_or(RunError::Contract(
                    "stored protocol iterator quotation overflow",
                ))?;
        Ok(self
            .endpoint
            .local_call(|| {
                // Iterator construction and action/result transfers have fixed scalar work.
                self.endpoint.admit_work(4)?;
                self.endpoint
                    .admit(ExecutionWork::Resource { requested_bytes })?;
                self.endpoint.check_completion()?;
                Ok(action())
            })
            .await)
    }

    /// Advances a borrowed receiver constraint cursor without extending its backing-set lifetime.
    /// Returns the completion, duplicate-step, or stored-pair result of
    /// [`OwnedConstraintTypeCursor::next_with`], preserving the pair's stored order.
    pub(in crate::types) async fn receiver_constraint_step<'walk>(
        &mut self,
        cursor: &mut OwnedConstraintTypeCursor<'walk, 'db>,
    ) -> RunResult<Option<Option<[Type<'db>; 2]>>> {
        local_quoted_with_fixed_transfers_at(
            self.endpoint, const { receiver::receiver_index_quote() }, || {
                cursor
                    .next_with(&mut receiver::ReceiverControl { endpoint: self.endpoint })
                    .map_err(|error| self.unavailable.error(self.db, error))
            }).await?
    }

    async fn refuse<T>(&self, operation: SearchOperation) -> RunResult<T> {
        self.unavailable
            .unavailable(self.db, self.endpoint, operation)
            .await
    }

    /// Constructs and reads a generated field request after admitting its fixed transfers.
    /// The accessor identifies the generated accessor type for quotation and is not invoked.
    async fn field<H: Copy, A, R: FieldRequest<'db>>(
        &self,
        handle: H,
        accessor: impl FnOnce(H, FieldRequestContext<'db>) -> A,
        request: impl Fn(H, FieldRequestContext<'db>) -> R,
    ) -> RunResult<R::Output> {
        self.field_with_profile(handle, accessor, request, &BorrowOrCopy).await
    }

    /// Reads a generated field with the supplied conversion profile after admitting construction.
    async fn field_with_profile<H: Copy, A, R: FieldRequest<'db>, P: FieldReadProfile<R::Stored>>(
        &self,
        handle: H,
        accessor: impl FnOnce(H, FieldRequestContext<'db>) -> A,
        request: impl Fn(H, FieldRequestContext<'db>) -> R,
        profile: &P,
    ) -> RunResult<R::Output> {
        let quote = generated_field_quote(accessor, &request);
        let read = boxed_future_with_fixed_transfers_at(self.endpoint, quote, || {
            self.endpoint.read_field(request(handle, self.endpoint.field_request_context()), profile)
        }).await?;
        Ok(read.await)
    }

    async fn local<T>(&self, work: usize, operation: impl FnOnce() -> T) -> RunResult<T> {
        local_with_fixed_transfers_at(self.endpoint, work, 0, operation).await
    }

    async fn refuse_field<T>(&self, operation: TypeWalkFieldOperation) -> RunResult<T> {
        self.refuse(SearchOperation::StoredField(operation)).await
    }
}

impl<'db, Q, U: TypeSearchUnavailable> TypeWalkEffects<'db> for RuntimeTypeWalk<'_, '_, 'db, Q, U> {
    type Error = RunError;

    async fn union_elements(&mut self, ty: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.field_with_profile(
            ty,
            |handle, context| handle.field_requests(context),
            |handle, context| handle.field_requests(context).elements(),
            &TypeSliceDeref,
        ).await
    }

    async fn intersection_positive(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).positive())
        .await
    }

    async fn intersection_negative(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> RunResult<&'db NegativeIntersectionElements<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).negative())
        .await
    }

    async fn enum_rest(
        &mut self,
        _ty: EnumComplementType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::EnumRest).await
    }

    async fn function_signature(
        &mut self,
        _ty: FunctionType<'db>,
    ) -> RunResult<Option<&'db CallableSignature<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::FunctionSignature)
            .await
    }

    async fn function_implementations(
        &mut self,
        _ty: FunctionType<'db>,
    ) -> RunResult<Option<&'db [CallableType<'db>]>> {
        self.refuse_field(TypeWalkFieldOperation::FunctionImplementations)
            .await
    }

    async fn callable_signatures(
        &mut self,
        ty: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).signatures())
        .await
    }

    async fn method_func(&mut self, ty: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).func())
        .await
    }

    async fn method_self(&mut self, ty: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        let receiver = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.receiver_request(context))
            .await?;
        self.local(2, || match receiver {
            BoundMethodReceiver::Instance(receiver)
            | BoundMethodReceiver::Constrained { receiver, .. } => receiver,
        })
        .await
    }

    async fn method_receiver(&mut self, ty: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        let receiver = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.receiver_request(context))
            .await?;
        self.local(2, || match receiver {
            BoundMethodReceiver::Instance(receiver) => receiver,
            BoundMethodReceiver::Constrained { constraint, .. } => constraint,
        })
        .await
    }

    async fn bound_super_children(
        &mut self,
        _ty: BoundSuperType<'db>,
    ) -> RunResult<[Option<Type<'db>>; 3]> {
        self.refuse_field(TypeWalkFieldOperation::BoundSuperChildren)
            .await
    }

    async fn alias_specialization(
        &mut self,
        ty: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).specialization())
        .await
    }

    async fn specialization_context(
        &mut self,
        ty: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).generic_context())
        .await
    }

    async fn specialization_types(
        &mut self,
        ty: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.field_with_profile(
            ty,
            |handle, context| handle.field_requests(context),
            |handle, context| handle.field_requests(context).types(),
            &TypeSliceDeref,
        ).await
    }

    async fn specialization_tuple(
        &mut self,
        ty: Specialization<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        let tuple = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.tuple_request(context))
            .await?;
        match tuple {
            Some(tuple) => Ok(Some(
                self.field(tuple, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).tuple())
                .await?,
            )),
            None => Ok(None),
        }
    }

    async fn context_variable(
        &mut self,
        ty: GenericContext<'db>,
        index: usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let variables = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.variables_request(context))
            .await?;
        local_quoted_with_fixed_transfers_at(self.endpoint, const { context_variable_at_quote() },
            || GenericContext::variable_at_in(variables, index),
        ).await
    }

    async fn bound_typevar(
        &mut self,
        ty: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).typevar())
        .await
    }

    async fn eager_typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> RunResult<(Option<TypeVarBoundOrConstraints<'db>>, bool)> {
        let bounds = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.bound_or_constraints_request(context))
            .await?;
        self.local(CALL_1 + 8, || decode_eager_bounds(bounds)).await
    }

    async fn eager_typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> RunResult<(Option<Type<'db>>, bool)> {
        let default = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.default_request(context))
            .await?;
        self.local(CALL_1 + 8, || decode_eager_default(default)).await
    }

    async fn constraint_elements(
        &mut self,
        ty: TypeVarConstraints<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let elements = self
            .field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).elements())
            .await?;
        self.local(CALL_1 + 2, || &**elements).await
    }

    async fn nominal_children(
        &mut self,
        ty: NominalInstanceType<'db>,
    ) -> RunResult<NominalVisitorChildren<'db>> {
        let kind = local_with_fixed_transfers_at(self.endpoint, CALL_1 + 4, 0, || ty.visitor_kind())
            .await?;
        match kind {
            NominalVisitorKind::None => Ok(NominalVisitorChildren::None),
            NominalVisitorKind::Tuple(tuple) => {
                let tuple = self
                    .field(tuple, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).tuple())
                    .await?;
                Ok(NominalVisitorChildren::Tuple(tuple))
            }
            NominalVisitorKind::Class(class) => {
                let class = match class {
                    NominalInstanceClass::Plain(class) => class,
                    NominalInstanceClass::InheritsFromExplicitAny(class) => {
                        self.field(class, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).class())
                        .await?
                    }
                };
                self.local(3 * CALL_1 + 5, || NominalVisitorChildren::Class(Type::from(class)))
                    .await
            }
        }
    }

    async fn protocol_children(
        &mut self,
        _ty: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolVisitorChildren<'db>> {
        self.refuse_field(TypeWalkFieldOperation::ProtocolChildren)
            .await
    }

    async fn property_class(
        &mut self,
        _ty: PropertyInstanceType<'db>,
    ) -> RunResult<PropertyInstanceClass<'db>> {
        self.refuse_field(TypeWalkFieldOperation::PropertyClass)
            .await
    }

    async fn property_getter(
        &mut self,
        _ty: PropertyInstanceType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::PropertyGetter)
            .await
    }

    async fn property_setter(
        &mut self,
        _ty: PropertyInstanceType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::PropertySetter)
            .await
    }

    async fn property_deleter(
        &mut self,
        _ty: PropertyInstanceType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::PropertyDeleter)
            .await
    }

    async fn slot_value(&mut self, _ty: SlotDescriptorType<'db>) -> RunResult<Type<'db>> {
        self.refuse_field(TypeWalkFieldOperation::SlotValue).await
    }

    async fn type_is_argument(&mut self, ty: TypeIsType<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).type_argument())
        .await
    }

    async fn type_guard_return(&mut self, ty: TypeGuardType<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).return_type())
        .await
    }

    async fn type_form_argument(&mut self, ty: TypeFormType<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).type_argument())
        .await
    }

    async fn alias_arguments(
        &mut self,
        ty: TypeAliasType<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        match ty {
            TypeAliasType::PEP695(alias) => {
                self.field(alias, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).specialization())
                    .await
            }
            TypeAliasType::ManualPEP695(alias) => {
                self.field(alias, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).specialization())
                    .await
            }
        }
    }

    async fn recursive_arguments(
        &mut self,
        ty: RecursiveType<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).arguments())
        .await
    }

    async fn interface_members(
        &mut self,
        _ty: ProtocolInterfaceView<'db>,
    ) -> RunResult<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::InterfaceMembers)
            .await
    }

    async fn synthesized_typed_dict_items(
        &mut self,
        _ty: SynthesizedTypedDictType<'db>,
    ) -> RunResult<&'db TypedDictSchema<'db>> {
        self.refuse_field(TypeWalkFieldOperation::SynthesizedTypedDictItems)
            .await
    }

    async fn synthesized_typed_dict_openness(
        &mut self,
        _ty: SynthesizedTypedDictType<'db>,
    ) -> RunResult<TypedDictOpenness<'db>> {
        self.refuse_field(TypeWalkFieldOperation::SynthesizedTypedDictOpenness)
            .await
    }

    async fn eager_newtype_base(
        &mut self,
        ty: NewType<'db>,
    ) -> RunResult<Option<NewTypeBase<'db>>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.eager_base_request(context))
            .await
    }

    async fn solution_bindings(
        &mut self,
        _ty: InternedConstraintSetSolution<'db>,
    ) -> RunResult<&'db [TypeVarSolution<'db>]> {
        self.refuse_field(TypeWalkFieldOperation::SolutionBindings)
            .await
    }

    async fn field_default(&mut self, ty: FieldInstance<'db>) -> RunResult<Option<Type<'db>>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).default_type())
        .await
    }

    async fn field_converter(
        &mut self,
        ty: FieldInstance<'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).converter())
        .await
    }

    async fn union_value(&mut self, _ty: UnionTypeInstance<'db>) -> RunResult<Option<Type<'db>>> {
        self.refuse_field(TypeWalkFieldOperation::UnionValue).await
    }

    async fn interned_type(&mut self, ty: InternedType<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).inner())
        .await
    }

    async fn named_tuple_fields(
        &mut self,
        _ty: NamedTupleSpec<'db>,
    ) -> RunResult<&'db [NamedTupleField<'db>]> {
        self.refuse_field(TypeWalkFieldOperation::NamedTupleFields)
            .await
    }

    async fn partial_callable(
        &mut self,
        ty: FunctoolsPartialInstance<'db>,
    ) -> RunResult<CallableType<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).partial())
        .await
    }

    async fn method_wrapper_type(&mut self, ty: MethodWrapper<'db>) -> RunResult<Type<'db>> {
        self.field(ty, |handle, context| handle.field_requests(context), |handle, context| handle.field_requests(context).wrapped())
        .await
    }

    async fn remember_type(
        &mut self,
        seen: &mut TypeCollector<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let ((inline, len, capacity), preparation) = local_quoted_with_fixed_transfers_at(
            self.endpoint, const { storage::seen_metadata_quote() },
            || {
                let metadata = storage::seen_metadata(seen);
                let preparation = match (metadata.0, metadata.1 == metadata.2) {
                    (false, false) => const { storage::seen_payload_preparation_quote(false, false) },
                    (false, true) => const { storage::seen_payload_preparation_quote(false, true) },
                    (true, false) => const { storage::seen_payload_preparation_quote(true, false) },
                    (true, true) => const { storage::seen_payload_preparation_quote(true, true) },
                };
                (metadata, preparation)
            },
        ).await?;
        let quote = local_quoted_with_fixed_transfers_at(
            self.endpoint, preparation,
            || storage::seen_payload_quote(inline, len, capacity, ty.inline_payload_bytes()),
        ).await??;
        local_quoted_with_fixed_transfers_at(self.endpoint, Ok(quote), || {
                seen.type_was_already_seen_with(
                    ty,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.unavailable.error(self.db, error))
            }).await?
    }

    async fn push_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok((0, 0)), || {
            push_type_walk_action_with(cursor, action, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn push_visit(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok((0, 0)), || {
            push_type_walk_visit_with(cursor, ty, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn push_tuple(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        tuple: &'db TupleSpec<'db>,
    ) -> Result<(), Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, const { facts::tuple_quote() }, || {
            push_type_walk_tuple_with(cursor, tuple, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn expand_children(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        kind: NonAtomicType<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<(), Self::Error> {
        let quote = local_quoted_with_fixed_transfers_at(
            self.endpoint, const { facts::expansion_preparation_quote() },
            || facts::children_quote(kind),
        ).await??;
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok(quote), || {
            expand_type_children_with(cursor, kind, policy, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn expand_wrapper(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        wrapper: KnownBoundMethodType<'db>,
    ) -> Result<(), Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, const { facts::expansion_quote() }, || {
            expand_method_wrapper_children_with(cursor, wrapper, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn expand_known(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        known: KnownInstanceType<'db>,
    ) -> Result<(), Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, const { facts::expansion_quote() }, || {
            expand_known_instance_children_with(cursor, known, TypeWalkFacts, self)
        }).await?;
        future.await
    }
    async fn next_event(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<Option<TypeWalkEvent<'db>>, Self::Error> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok((0, 0)), || {
            next_type_walk_event_with(cursor, policy, TypeWalkFacts, self)
        }).await?;
        future.await
    }

    async fn checkpoint(&mut self, work: TypeWalkWork) -> RunResult<()> {
        if let TypeWalkWork::Search(SearchWork::Semantic(operation)) = work {
            return self.refuse(operation).await;
        }
        self.local(1, || ()).await
    }

    async fn take_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
    ) -> RunResult<Option<WalkAction<'db>>> {
        let action = local_quoted_with_fixed_transfers_at(
            self.endpoint, const { storage::pending_pop_quote() }, || cursor.pending.pop(),
        ).await?;
        // Keep the removed action in this future through quote selection. The final callback
        // retains it during failed-admission cleanup, so receiver-cursor cleanup follows the
        // execution endpoint's queued child tasks.
        let quote = local_quoted_with_fixed_transfers_at(
            self.endpoint, const { facts::frame_preparation_quote() },
            || facts::frame_quote(action.as_ref()),
        ).await?;
        local_quoted_with_fixed_transfers_at(self.endpoint, quote, || action).await
    }

    async fn enqueue(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> RunResult<()> {
        // The future keeps the frame until both admission and reservation have succeeded.
        // A rejecting callback therefore cannot destroy it before queued children drain.
        let mut action = Some(action);
        let quote = local_quoted_with_fixed_transfers_at(
            self.endpoint, const { storage::pending_metadata_quote() },
            || {
                let len = cursor.pending.len();
                let capacity = cursor.pending.capacity();
                match (len == capacity, capacity > 8) {
                    (false, false) => const { storage::pending_insert_quote(false, false) },
                    (false, true) => const { storage::pending_insert_quote(false, true) },
                    (true, false) => const { storage::pending_insert_quote(true, false) },
                    (true, true) => const { storage::pending_insert_quote(true, true) },
                }
            },
        ).await?;
        local_quoted_with_fixed_transfers_at(self.endpoint, quote, || {
                reserve_walk_pending_with(
                    cursor,
                    1,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.unavailable.error(self.db, error))?;
                if let Some(action) = action.take() {
                    cursor.pending.push(action);
                }
                Ok(())
            }).await??;
        Ok(())
    }

    async fn enqueue_visits<const N: usize>(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        children: [Option<Type<'db>>; N],
    ) -> RunResult<()> {
        for ty in children.into_iter().rev().flatten() {
            self.enqueue(cursor, WalkAction::Visit(ty)).await?;
        }
        Ok(())
    }

    async fn next_stored(
        &mut self,
        types: &mut StoredTypeSequence<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        match types {
            StoredTypeSequence::Ordered(_) | StoredTypeSequence::Negative(_)
            | StoredTypeSequence::SolutionBindings(_) | StoredTypeSequence::NamedTupleFields(_) => {
                local_quoted_with_fixed_transfers_at(
                    self.endpoint, const { storage::stored_slice_next_quote() }, || types.next_type(),
                ).await
            }
            StoredTypeSequence::TypedDictFields(_) => {
                self.local(1, || types.next_type()).await
            }
        }
    }

    async fn next_member(
        &mut self,
        members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> RunResult<Option<(&'db Name, &'db ProtocolMemberData<'db>)>> {
        Ok(self.endpoint.local_call(|| Ok(members.next())).await)
    }

    async fn constraint_type_step(
        &mut self,
        cursor: &mut OwnedConstraintTypeCursor<'db, 'db>,
    ) -> RunResult<Option<Option<[Type<'db>; 2]>>> {
        self.receiver_constraint_step(cursor).await
    }

    async fn protocol_interface(
        &mut self,
        _: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>> {
        self.refuse(SearchOperation::ProtocolInterface).await
    }

    async fn protocol_member_types(
        &mut self,
        _: ProtocolMember<'db, 'db>,
    ) -> RunResult<[Option<Type<'db>>; 6]> {
        self.refuse(SearchOperation::ProtocolMember).await
    }

    async fn typevar_bounds(
        &mut self,
        _: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.refuse(SearchOperation::TypeVarBounds).await
    }

    async fn typevar_default(&mut self, _: TypeVarInstance<'db>) -> RunResult<Option<Type<'db>>> {
        self.refuse(SearchOperation::TypeVarDefault).await
    }

    async fn alias_value(&mut self, _: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.refuse(SearchOperation::AliasValue).await
    }

    async fn recursive_unfold(&mut self, _: RecursiveType<'db>) -> RunResult<Type<'db>> {
        self.refuse(SearchOperation::RecursiveUnfold).await
    }

    async fn typed_dict_items(
        &mut self,
        _: TypedDictType<'db>,
    ) -> RunResult<&'db TypedDictSchema<'db>> {
        self.refuse(SearchOperation::TypedDictItems).await
    }

    async fn typed_dict_extra(&mut self, _: TypedDictType<'db>) -> RunResult<Option<Type<'db>>> {
        self.refuse(SearchOperation::TypedDictOpenness).await
    }

    async fn newtype_base(&mut self, _: NewType<'db>) -> RunResult<NewTypeBase<'db>> {
        self.refuse(SearchOperation::NewTypeBase).await
    }

    async fn newtype_instance(&mut self, _: NewTypeBase<'db>) -> RunResult<Type<'db>> {
        self.refuse(SearchOperation::NewTypeInstance).await
    }
}

impl<'run, 'db: 'run, R, Q: RuntimeTypeSearchWith<'run, 'db, R>, U: TypeSearchUnavailable>
    TypeSearchEffects<'db, R> for RuntimeTypeWalk<'_, 'run, 'db, Q, U>
{
    async fn new_state(&mut self) -> RunResult<(TypeWalkCursor<'db>, TypeCollector<'db>)> {
        local_quoted_with_fixed_transfers_at(
            self.endpoint,
            const { storage::search_initial_state_quote() },
            || (TypeWalkFacts.empty_cursor(), TypeWalkFacts.empty_seen()),
        )
        .await
    }

    async fn predicate(&mut self, ty: Type<'db>) -> RunResult<R> {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok((0, 0)), || {
            self.query.predicate(self.endpoint, ty)
        }).await?;
        future.await
    }

    async fn decide_visit(
        &mut self,
        ty: Type<'db>,
        policy: TypeWalkPolicy,
        found: R,
    ) -> RunResult<TypeSearchDecision<'db, R>>
    where
        R: Copy + Default + PartialEq,
    {
        let future = local_quoted_with_fixed_transfers_at(
            self.endpoint,
            const { facts::search_decision_quote::<R>() },
            || decide_type_search_visit_with(ty, policy, found, TypeWalkFacts, self),
        ).await?;
        future.await
    }

    async fn schedule_descent(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        seen: &mut TypeCollector<'db>,
        descent: TypeSearchDescent<'db>,
    ) -> RunResult<()> {
        let future = local_quoted_with_fixed_transfers_at(
            self.endpoint,
            const { facts::search_descent_quote() },
            || schedule_type_search_descent_with::<R, _>(cursor, seen, descent, TypeWalkFacts, self),
        ).await?;
        future.await
    }
}

#[cfg(feature = "experimental-analysis")]
struct HasTypeVar;

#[cfg(feature = "experimental-analysis")]
impl<'db> RuntimeTypeSearch<'db> for HasTypeVar {
    fn predicate(&self, ty: Type<'db>) -> bool {
        matches!(ty, Type::TypeVar(_))
    }
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) async fn has_typevar_with<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
    unavailable: impl TypeSearchUnavailable,
) -> RunResult<bool> {
    let mut effects = RuntimeTypeWalk {
        db,
        endpoint,
        query: HasTypeVar,
        unavailable,
    };
    let future = boxed_future_with_fixed_transfers_at(endpoint, Ok((0, 0)), || search_type_with(
        ty,
        TypeSearchMode::SkipLazyAttributes,
        TypeWalkFacts,
        &mut effects,
    )).await?;
    future.await
}

#[cfg(feature = "experimental-analysis")]
struct HasDynamic;

#[cfg(feature = "experimental-analysis")]
impl<'db> RuntimeTypeSearch<'db> for HasDynamic {
    fn predicate(&self, ty: Type<'db>) -> bool {
        ty.is_dynamic()
    }
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) async fn has_dynamic_with<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
    unavailable: impl TypeSearchUnavailable,
) -> RunResult<bool> {
    let mut effects = RuntimeTypeWalk {
        db,
        endpoint,
        query: HasDynamic,
        unavailable,
    };
    let future = boxed_future_with_fixed_transfers_at(endpoint, Ok((0, 0)), || search_type_with(
        ty,
        TypeSearchMode::SkipLazyAttributes,
        TypeWalkFacts,
        &mut effects,
    )).await?;
    future.await
}

#[cfg(feature = "experimental-analysis")]
struct HasAliasLike;

#[cfg(feature = "experimental-analysis")]
impl<'db> RuntimeTypeSearch<'db> for HasAliasLike {
    fn predicate(&self, ty: Type<'db>) -> bool {
        ty.is_alias_like()
    }
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) async fn has_alias_like_with<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
    unavailable: impl TypeSearchUnavailable,
) -> RunResult<bool> {
    let mut effects = RuntimeTypeWalk {
        db,
        endpoint,
        query: HasAliasLike,
        unavailable,
    };
    let future = boxed_future_with_fixed_transfers_at(endpoint, Ok((0, 0)), || search_type_with(
        ty,
        TypeSearchMode::SkipLazyAttributes,
        TypeWalkFacts,
        &mut effects,
    )).await?;
    future.await
}
