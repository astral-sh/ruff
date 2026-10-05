use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::{Future, pending, ready};
use std::task::Poll;

use super::{DescriptorEffects, sealed};
use crate::db::tests::setup_db;
use crate::place::Place;
use crate::types::call::{Bindings, CallError};
use crate::types::descriptor::{
    DescriptorInvocationRequest, DescriptorMemberRequest, DescriptorRequest, DescriptorResult,
    evaluate_entry_with_effects, evaluate_with_effects,
};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{
    AttributeKind, BoundMethodType, DescriptorGetCallContext, DescriptorGetError,
    DescriptorGetResult, DescriptorOrigin, IntersectionBuilder, IntersectionType,
    MemberLookupPolicy, SlotDescriptorType, Type, UnionBuilder, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectError {
    Unsupported(&'static str),
    Unexpected(&'static str),
    Incomplete(&'static str),
}

enum Step<'db> {
    FunctionLike(
        DescriptorRequest<'db>,
        Result<Option<Type<'db>>, EffectError>,
    ),
    Protocol(
        DescriptorRequest<'db>,
        Result<DescriptorResult<'db>, EffectError>,
    ),
    UnionLike(Type<'db>, Result<Option<UnionType<'db>>, EffectError>),
    Declare(Vec<DescriptorRequest<'db>>),
    Descriptor(DescriptorRequest<'db>, Option<DescriptorResult<'db>>),
    Member(DescriptorMemberRequest<'db>, Place<'db>),
    NoneType(Type<'db>),
    DataDescriptor(Type<'db>, bool),
    Invoke(DescriptorInvocationRequest<'db>, EffectError),
    MergeOrigins(
        DescriptorOrigin<'db>,
        DescriptorOrigin<'db>,
        DescriptorOrigin<'db>,
    ),
    UnionAdd(Type<'db>),
    UnionBuild(Type<'db>),
}

struct ScriptedEffects<'db> {
    steps: RefCell<VecDeque<Step<'db>>>,
}

impl<'db> ScriptedEffects<'db> {
    fn new(steps: impl IntoIterator<Item = Step<'db>>) -> Self {
        Self {
            steps: RefCell::new(steps.into_iter().collect()),
        }
    }

    fn next(&self, operation: &'static str) -> Result<Step<'db>, EffectError> {
        self.steps
            .borrow_mut()
            .pop_front()
            .ok_or(EffectError::Unsupported(operation))
    }

    fn assert_finished(&self) {
        assert!(self.steps.borrow().is_empty());
    }
}

impl sealed::Sealed for ScriptedEffects<'_> {}

impl<'db> DescriptorEffects<'db> for ScriptedEffects<'db> {
    type Error = EffectError;

    async fn checkpoint(&self) -> Result<(), EffectError> {
        Ok(())
    }

    async fn slot_value(
        &self,
        db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, EffectError> {
        Ok(descriptor.value_type(db))
    }

    async fn union_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> Result<(UnionBuilder<'db>, &'db [Type<'db>]), EffectError> {
        Ok((
            UnionBuilder::new(db, env).or_recursively_defined(union.recursively_defined(db)),
            union.elements(db),
        ))
    }

    async fn intersection_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>), EffectError> {
        Ok((IntersectionBuilder::new(db, env), intersection.positive(db)))
    }

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> Result<Option<DescriptorRequest<'db>>, EffectError> {
        Ok(requests.next())
    }

    async fn call_context(
        &self,
        db: &'db dyn Db,
        request: DescriptorRequest<'db>,
        callable: Type<'db>,
    ) -> Result<DescriptorGetCallContext<'db>, EffectError> {
        Ok(DescriptorGetCallContext::new(
            db,
            request.ty,
            callable,
            request.instance,
            request.owner,
        ))
    }

    fn function_like(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready((|| {
            let Step::FunctionLike(expected, result) = self.next("function_like")? else {
                return Err(EffectError::Unexpected("function_like"));
            };
            assert_eq!(request, expected);
            result
        })())
    }

    fn protocol(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<DescriptorResult<'db>, Self::Error>> {
        ready((|| {
            let Step::Protocol(expected, result) = self.next("protocol")? else {
                return Err(EffectError::Unexpected("protocol"));
            };
            assert_eq!(request, expected);
            result
        })())
    }

    fn declare_descriptors(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready((|| {
            let Step::Declare(expected) = self.next("declare_descriptors")? else {
                return Err(EffectError::Unexpected("declare_descriptors"));
            };
            assert!(requests.eq(expected));
            Ok(())
        })())
    }

    async fn descriptor(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error> {
        let Step::Descriptor(expected, result) = self.next("descriptor")? else {
            return Err(EffectError::Unexpected("descriptor"));
        };
        assert_eq!(request, expected);
        match result {
            Some(result) => Ok(result),
            None => pending().await,
        }
    }

    fn union_like(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<UnionType<'db>>, Self::Error>> {
        ready((|| {
            let Step::UnionLike(expected, result) = self.next("union_like")? else {
                return Err(EffectError::Unexpected("union_like"));
            };
            assert_eq!(ty, expected);
            result
        })())
    }

    fn class_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorMemberRequest<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready((|| {
            let Step::Member(expected, result) = self.next("class_member")? else {
                return Err(EffectError::Unexpected("class_member"));
            };
            assert_eq!(request, expected);
            Ok(result)
        })())
    }

    fn data_descriptor(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready((|| {
            let Step::DataDescriptor(expected, result) = self.next("data_descriptor")? else {
                return Err(EffectError::Unexpected("data_descriptor"));
            };
            assert_eq!(ty, expected);
            Ok(result)
        })())
    }

    fn none_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready((|| {
            let Step::NoneType(result) = self.next("none_type")? else {
                return Err(EffectError::Unexpected("none_type"));
            };
            Ok(result)
        })())
    }

    fn invoke(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> impl Future<Output = Result<Result<Bindings<'db>, CallError<'db>>, Self::Error>> {
        ready((|| {
            let Step::Invoke(expected, error) = self.next("invoke")? else {
                return Err(EffectError::Unexpected("invoke"));
            };
            assert_eq!(request, expected);
            Err(error)
        })())
    }

    fn bindings_origin(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
        _arguments: &[Type<'db>; 3],
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Self::Error>> {
        ready(Err(EffectError::Unsupported("bindings_origin")))
    }

    fn bindings_return_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(EffectError::Unsupported("bindings_return_type")))
    }

    fn merge_origins(
        &self,
        _db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Self::Error>> {
        ready((|| {
            let Step::MergeOrigins(expected_left, expected_right, result) =
                self.next("merge_origins")?
            else {
                return Err(EffectError::Unexpected("merge_origins"));
            };
            assert_eq!(left, expected_left);
            assert_eq!(right, expected_right);
            Ok(result)
        })())
    }

    fn union_add(
        &self,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Self::Error>> {
        ready((|| {
            let Step::UnionAdd(expected) = self.next("union_add")? else {
                return Err(EffectError::Unexpected("union_add"));
            };
            assert_eq!(ty, expected);
            Ok(builder)
        })())
    }

    fn union_build(
        &self,
        _builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready((|| {
            let Step::UnionBuild(result) = self.next("union_build")? else {
                return Err(EffectError::Unexpected("union_build"));
            };
            Ok(result)
        })())
    }

    fn union_pair(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(EffectError::Unsupported("union_pair")))
    }

    fn intersection_add(
        &self,
        _builder: IntersectionBuilder<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<IntersectionBuilder<'db>, Self::Error>> {
        ready(Err(EffectError::Unsupported("intersection_add")))
    }

    fn intersection_build(
        &self,
        _builder: IntersectionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(EffectError::Unsupported("intersection_build")))
    }
}

fn request(ty: Type<'_>) -> DescriptorRequest<'_> {
    DescriptorRequest {
        ty,
        instance: Some(Type::int_literal(91)),
        owner: Type::int_literal(92),
    }
}

fn member(ty: Type<'_>, policy: MemberLookupPolicy) -> DescriptorMemberRequest<'_> {
    DescriptorMemberRequest { ty, policy }
}

#[test]
fn bound_method_entry_needs_no_descriptor_dependencies() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = Type::BoundMethod(BoundMethodType::from_callable(
        &db,
        Type::unknown(),
        env.program(&db),
        Type::int_literal(1),
    ));
    let effects = ScriptedEffects::new([]);
    assert_eq!(
        try_poll_immediate(evaluate_entry_with_effects(
            &db,
            &env,
            request(ty),
            &effects
        )),
        Poll::Ready(Ok(Ok(None)))
    );
    effects.assert_finished();
}

#[test]
fn native_descriptor_entry_returns_binding_without_protocol_lookup() {
    let db = setup_db();
    let env = db.program_environment();
    let request = request(Type::int_literal(1));
    let return_type = Type::int_literal(2);
    let effects = ScriptedEffects::new([Step::FunctionLike(request, Ok(Some(return_type)))]);
    assert_eq!(
        try_poll_immediate(evaluate_entry_with_effects(&db, &env, request, &effects)),
        Poll::Ready(Ok(Ok(Some(DescriptorGetResult {
            return_type,
            origin: DescriptorOrigin::default(),
            kind: AttributeKind::NormalOrNonDataDescriptor,
        }))))
    );
    effects.assert_finished();
}

#[test]
fn incomplete_native_binding_does_not_fall_back_to_protocol_lookup() {
    let db = setup_db();
    let env = db.program_environment();
    let request = request(Type::int_literal(1));
    let effects = ScriptedEffects::new([Step::FunctionLike(
        request,
        Err(EffectError::Incomplete("native binding")),
    )]);
    assert_eq!(
        try_poll_immediate(evaluate_entry_with_effects(&db, &env, request, &effects)),
        Poll::Ready(Err(EffectError::Incomplete("native binding")))
    );
    effects.assert_finished();
}

#[test]
fn slot_entry_preserves_class_and_instance_access_after_native_binding() {
    let db = setup_db();
    let env = db.program_environment();
    let value_type = Type::int_literal(1);
    let ty = Type::SlotDescriptor(SlotDescriptorType::new(&db, value_type));
    for (instance, return_type) in [(Some(Type::int_literal(91)), value_type), (None, ty)] {
        let request = DescriptorRequest {
            instance,
            ..request(ty)
        };
        let effects = ScriptedEffects::new([Step::FunctionLike(request, Ok(None))]);
        assert_eq!(
            try_poll_immediate(evaluate_entry_with_effects(&db, &env, request, &effects)),
            Poll::Ready(Ok(Ok(Some(DescriptorGetResult {
                return_type,
                origin: DescriptorOrigin::default(),
                kind: AttributeKind::DataDescriptor,
            }))))
        );
        effects.assert_finished();
    }
}

#[test]
fn entry_preserves_protocol_request_after_negative_native_binding() {
    let db = setup_db();
    let env = db.program_environment();
    let result = Ok(Some(DescriptorGetResult {
        return_type: Type::int_literal(2),
        origin: DescriptorOrigin {
            incomplete: true,
            ..DescriptorOrigin::default()
        },
        kind: AttributeKind::DataDescriptor,
    }));
    for instance in [Some(Type::int_literal(91)), None] {
        let request = DescriptorRequest {
            instance,
            ..request(Type::int_literal(1))
        };
        let effects = ScriptedEffects::new([
            Step::FunctionLike(request, Ok(None)),
            Step::Protocol(request, Ok(result)),
        ]);
        assert_eq!(
            try_poll_immediate(evaluate_entry_with_effects(&db, &env, request, &effects)),
            Poll::Ready(Ok(result))
        );
        effects.assert_finished();
    }
}

#[test]
fn dynamic_descriptor_needs_no_descriptor_dependencies() {
    let db = setup_db();
    let env = db.program_environment();
    for ty in [Type::any(), Type::unknown()] {
        let effects = ScriptedEffects::new([]);
        assert_eq!(
            try_poll_immediate(evaluate_with_effects(&db, &env, request(ty), &effects)),
            Poll::Ready(Ok(Ok(Some(DescriptorGetResult {
                return_type: ty,
                origin: DescriptorOrigin::default(),
                kind: AttributeKind::DataDescriptor,
            }))))
        );
        effects.assert_finished();
    }
}

#[test]
fn unsupported_union_lookup_is_an_effect_error() {
    let db = setup_db();
    let env = db.program_environment();
    let effects = ScriptedEffects::new([]);
    assert_eq!(
        try_poll_immediate(evaluate_with_effects(
            &db,
            &env,
            request(Type::int_literal(1)),
            &effects,
        )),
        Poll::Ready(Err(EffectError::Unsupported("union_like")))
    );
}

#[test]
fn missing_concrete_get_is_descriptor_absence() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = Type::int_literal(1);
    let effects = ScriptedEffects::new([
        Step::UnionLike(ty, Ok(None)),
        Step::Member(
            member(ty, MemberLookupPolicy::REQUIRE_CONCRETE),
            Place::Undefined,
        ),
    ]);
    assert_eq!(
        try_poll_immediate(evaluate_with_effects(&db, &env, request(ty), &effects)),
        Poll::Ready(Ok(Ok(None)))
    );
    effects.assert_finished();
}

#[test]
fn get_lookup_checks_concreteness_before_disabling_instance_fallback() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = Type::int_literal(1);
    let effects = ScriptedEffects::new([
        Step::UnionLike(ty, Ok(None)),
        Step::Member(
            member(ty, MemberLookupPolicy::REQUIRE_CONCRETE),
            Place::bound(Type::int_literal(2)),
        ),
        Step::Member(
            member(ty, MemberLookupPolicy::NO_INSTANCE_FALLBACK),
            Place::Undefined,
        ),
    ]);
    assert_eq!(
        try_poll_immediate(evaluate_with_effects(&db, &env, request(ty), &effects)),
        Poll::Ready(Ok(Ok(None)))
    );
    effects.assert_finished();
}

#[test]
fn invocation_preserves_receiver_instance_and_owner_and_incomplete_error() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = Type::int_literal(1);
    let callable = Type::int_literal(3);
    let none = Type::none(&db, &env);
    for instance in [Some(Type::int_literal(91)), None] {
        let request = DescriptorRequest {
            instance,
            ..request(ty)
        };
        let mut steps = vec![
            Step::UnionLike(ty, Ok(None)),
            Step::Member(
                member(ty, MemberLookupPolicy::REQUIRE_CONCRETE),
                Place::bound(Type::int_literal(2)),
            ),
            Step::Member(
                member(ty, MemberLookupPolicy::NO_INSTANCE_FALLBACK),
                Place::bound(callable),
            ),
        ];
        if instance.is_none() {
            steps.push(Step::NoneType(none));
        }
        steps.extend([
            Step::DataDescriptor(ty, true),
            Step::Invoke(
                DescriptorInvocationRequest {
                    callable,
                    arguments: [ty, instance.unwrap_or(none), request.owner],
                },
                EffectError::Incomplete("argument checking"),
            ),
        ]);
        let effects = ScriptedEffects::new(steps);
        assert_eq!(
            try_poll_immediate(evaluate_with_effects(&db, &env, request, &effects)),
            Poll::Ready(Err(EffectError::Incomplete("argument checking")))
        );
        effects.assert_finished();
    }
}

#[test]
fn union_declares_every_child_before_waiting_for_the_first() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = UnionType::from_elements(&db, &env, [Type::int_literal(1), Type::int_literal(2)]);
    let Type::Union(union) = ty else {
        panic!("distinct integer literals form a union");
    };
    let children: Vec<_> = union.elements(&db).iter().copied().map(request).collect();
    let effects = ScriptedEffects::new([
        Step::UnionLike(ty, Ok(Some(union))),
        Step::Declare(children.clone()),
        Step::Descriptor(children[0], None),
        Step::Descriptor(children[1], Some(Ok(None))),
    ]);
    assert_eq!(
        try_poll_immediate(evaluate_with_effects(&db, &env, request(ty), &effects)),
        Poll::Pending
    );
    assert_eq!(effects.steps.borrow().len(), 1);
}

#[test]
fn union_retains_first_error_and_later_absent_alternative() {
    let db = setup_db();
    let env = db.program_environment();
    let ty = UnionType::from_elements(&db, &env, (1..=3).map(Type::int_literal));
    let Type::Union(union) = ty else {
        panic!("distinct integer literals form a union");
    };
    let children: Vec<_> = union.elements(&db).iter().copied().map(request).collect();
    let first_origin = DescriptorOrigin {
        incomplete: true,
        ..DescriptorOrigin::default()
    };
    let first = DescriptorGetError {
        fallback: DescriptorGetResult {
            return_type: Type::int_literal(10),
            origin: first_origin,
            kind: AttributeKind::DataDescriptor,
        },
        context: DescriptorGetCallContext::new(
            &db,
            children[0].ty,
            Type::int_literal(11),
            children[0].instance,
            children[0].owner,
        ),
    };
    let last = DescriptorGetError {
        fallback: DescriptorGetResult {
            return_type: Type::int_literal(20),
            origin: DescriptorOrigin::default(),
            kind: AttributeKind::DataDescriptor,
        },
        context: DescriptorGetCallContext::new(
            &db,
            children[2].ty,
            Type::int_literal(21),
            children[2].instance,
            children[2].owner,
        ),
    };
    let combined_return_type = UnionType::from_elements(
        &db,
        &env,
        [
            first.fallback.return_type,
            children[1].ty,
            last.fallback.return_type,
        ],
    );
    let effects = ScriptedEffects::new([
        Step::UnionLike(ty, Ok(Some(union))),
        Step::Declare(children.clone()),
        Step::Descriptor(children[0], Some(Err(first))),
        Step::MergeOrigins(DescriptorOrigin::default(), first_origin, first_origin),
        Step::UnionAdd(first.fallback.return_type),
        Step::Descriptor(children[1], Some(Ok(None))),
        Step::UnionAdd(children[1].ty),
        Step::Descriptor(children[2], Some(Err(last))),
        Step::MergeOrigins(first_origin, DescriptorOrigin::default(), first_origin),
        Step::UnionAdd(last.fallback.return_type),
        Step::UnionBuild(combined_return_type),
    ]);
    let expected = DescriptorGetError {
        fallback: DescriptorGetResult {
            return_type: combined_return_type,
            origin: first_origin,
            kind: AttributeKind::NormalOrNonDataDescriptor,
        },
        context: first.context,
    };
    assert_eq!(
        try_poll_immediate(evaluate_with_effects(&db, &env, request(ty), &effects)),
        Poll::Ready(Ok(Err(expected)))
    );
    effects.assert_finished();
}
