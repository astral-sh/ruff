//! Descriptor `__get__` evaluation with owned continuations for its semantic dependencies.

use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::call::{Bindings, CallArguments, CallError};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::{
    AttributeKind, DescriptorGetCallContext, DescriptorGetError, DescriptorGetResult,
    DescriptorOrigin, IntersectionBuilder, MemberLookupPolicy, Type, UnionBuilder, UnionType,
    descriptor_get_result,
};
use crate::{Db, Program, ProgramEnvironment};

type DescriptorResult<'db> = Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>>;

pub(super) fn evaluate<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    ty: Type<'db>,
    instance: Option<Type<'db>>,
    owner: Type<'db>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> DescriptorResult<'db> {
    let env = &ProgramEnvironment::from_program(program);
    let mut step = DescriptorStep::start(
        db,
        env,
        DescriptorRequest {
            ty,
            instance,
            owner,
        },
    );
    loop {
        step = match step {
            DescriptorStep::Descriptor(pending) => {
                let request = pending.request;
                let result = request.ty.try_call_dunder_get_with_recursion_guard(
                    db,
                    env,
                    request.instance,
                    request.owner,
                    recursion_guard,
                );
                pending.resume(db, result)
            }
            DescriptorStep::Lookup(pending) => {
                let place = pending
                    .request
                    .ty
                    .class_member_with_policy(db, env, "__get__", pending.policy())
                    .place;
                pending.resume(db, env, place)
            }
            DescriptorStep::DataDescriptor(pending) => {
                let is_data_descriptor = pending.request.ty.is_data_descriptor(db, env);
                pending.resume(db, is_data_descriptor)
            }
            DescriptorStep::Invoke(pending) => {
                let result = pending.callable.try_call_with_recursion_guard(
                    db,
                    env,
                    &CallArguments::positional(pending.arguments),
                    recursion_guard,
                );
                pending.resume(db, env, result)
            }
            DescriptorStep::Complete(result) => return result,
        };
    }
}

#[derive(Clone, Copy)]
struct DescriptorRequest<'db> {
    ty: Type<'db>,
    instance: Option<Type<'db>>,
    owner: Type<'db>,
}

enum DescriptorStep<'db> {
    Descriptor(PendingDescriptor<'db>),
    Lookup(PendingLookup<'db>),
    DataDescriptor(PendingDataDescriptor<'db>),
    Invoke(PendingInvocation<'db>),
    Complete(DescriptorResult<'db>),
}

impl<'db> DescriptorStep<'db> {
    fn start(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Self {
        if let Some(fallback) = request.ty.materialized_divergent_fallback() {
            return Self::Descriptor(PendingDescriptor {
                request: DescriptorRequest {
                    ty: fallback,
                    ..request
                },
                continuation: DescriptorContinuation::Identity,
            });
        }

        if let Some(dynamic) = request.ty.dynamic_descriptor_type() {
            return Self::Complete(Ok(Some(DescriptorGetResult {
                return_type: dynamic,
                origin: DescriptorOrigin::default(),
                kind: AttributeKind::DataDescriptor,
            })));
        }

        if let Some(union) = request.ty.as_union_like(db) {
            let return_types =
                UnionBuilder::new(db, env).or_recursively_defined(union.recursively_defined(db));
            return UnionDescriptors {
                request,
                remaining: union.elements(db),
                return_types,
                error: None,
                any_descriptor: false,
                all_data_descriptors: true,
                origin: DescriptorOrigin::default(),
            }
            .advance();
        }

        if let Type::Intersection(intersection) = request.ty {
            let return_types = IntersectionBuilder::new(db, env);
            return IntersectionDescriptors {
                request,
                remaining: intersection.positive(db).iter(),
                return_types,
                origin: DescriptorOrigin::default(),
                error: None,
                any_descriptor: false,
            }
            .advance();
        }

        Self::Lookup(PendingLookup {
            request,
            stage: LookupStage::Concrete,
        })
    }
}

struct PendingDescriptor<'db> {
    request: DescriptorRequest<'db>,
    continuation: DescriptorContinuation<'db>,
}

impl<'db> PendingDescriptor<'db> {
    fn resume(self, db: &'db dyn Db, result: DescriptorResult<'db>) -> DescriptorStep<'db> {
        match self.continuation {
            DescriptorContinuation::Identity => DescriptorStep::Complete(result),
            DescriptorContinuation::Union(state) => state.resume(db, self.request.ty, result),
            DescriptorContinuation::Intersection(state) => {
                state.resume(db, self.request.ty, result)
            }
        }
    }
}

/// Builders stay live across nested requests so each result is normalized before the next
/// descriptor is evaluated. Their eager simplifications can themselves perform type relations.
enum DescriptorContinuation<'db> {
    Identity,
    Union(UnionDescriptors<'db>),
    Intersection(IntersectionDescriptors<'db>),
}

struct UnionDescriptors<'db> {
    request: DescriptorRequest<'db>,
    remaining: &'db [Type<'db>],
    return_types: UnionBuilder<'db>,
    error: Option<DescriptorGetCallContext<'db>>,
    any_descriptor: bool,
    all_data_descriptors: bool,
    origin: DescriptorOrigin<'db>,
}

impl<'db> UnionDescriptors<'db> {
    fn advance(mut self) -> DescriptorStep<'db> {
        if let Some((&alternative, remaining)) = self.remaining.split_first() {
            self.remaining = remaining;
            return DescriptorStep::Descriptor(PendingDescriptor {
                request: DescriptorRequest {
                    ty: alternative,
                    ..self.request
                },
                continuation: DescriptorContinuation::Union(self),
            });
        }

        DescriptorStep::Complete(if self.any_descriptor {
            descriptor_get_result(
                self.return_types.build(),
                self.origin,
                if self.all_data_descriptors {
                    AttributeKind::DataDescriptor
                } else {
                    AttributeKind::NormalOrNonDataDescriptor
                },
                self.error,
            )
        } else {
            Ok(None)
        })
    }

    fn resume(
        mut self,
        db: &'db dyn Db,
        alternative: Type<'db>,
        result: DescriptorResult<'db>,
    ) -> DescriptorStep<'db> {
        let result = result.unwrap_or_else(|failure| {
            self.error = self.error.or(Some(failure.context));
            Some(failure.fallback())
        });
        if let Some(DescriptorGetResult {
            return_type,
            kind,
            origin,
        }) = result
        {
            self.origin = self.origin.merge(db, origin);
            self.any_descriptor = true;
            self.all_data_descriptors &= kind.is_data();
            self.return_types = self.return_types.add(return_type);
        } else {
            self.all_data_descriptors = false;
            self.return_types = self.return_types.add(alternative);
        }
        self.advance()
    }
}

struct IntersectionDescriptors<'db> {
    request: DescriptorRequest<'db>,
    remaining: ordermap::set::Iter<'db, Type<'db>>,
    return_types: IntersectionBuilder<'db>,
    origin: DescriptorOrigin<'db>,
    error: Option<DescriptorGetCallContext<'db>>,
    any_descriptor: bool,
}

impl<'db> IntersectionDescriptors<'db> {
    fn advance(mut self) -> DescriptorStep<'db> {
        if let Some(&element) = self.remaining.next() {
            return DescriptorStep::Descriptor(PendingDescriptor {
                request: DescriptorRequest {
                    ty: element,
                    ..self.request
                },
                continuation: DescriptorContinuation::Intersection(self),
            });
        }

        DescriptorStep::Complete(if self.any_descriptor {
            descriptor_get_result(
                self.return_types.build(),
                self.origin,
                // TODO: Discover data descriptors in intersections without decomposing
                // the descriptor return type into an unsound intersection.
                AttributeKind::NormalOrNonDataDescriptor,
                self.error,
            )
        } else {
            Ok(None)
        })
    }

    fn resume(
        mut self,
        db: &'db dyn Db,
        element: Type<'db>,
        result: DescriptorResult<'db>,
    ) -> DescriptorStep<'db> {
        let result = result.unwrap_or_else(|failure| {
            self.error = self.error.or(Some(failure.context));
            Some(failure.fallback())
        });
        let (return_type, element_origin) = if let Some(result) = result {
            self.any_descriptor = true;
            (result.return_type, result.origin)
        } else {
            (element, DescriptorOrigin::default())
        };
        self.return_types.add_positive_in_place(return_type);
        self.origin = self.origin.merge(db, element_origin);
        self.advance()
    }
}

enum LookupStage {
    Concrete,
    IncludeDynamic,
}

struct PendingLookup<'db> {
    request: DescriptorRequest<'db>,
    stage: LookupStage,
}

impl<'db> PendingLookup<'db> {
    fn policy(&self) -> MemberLookupPolicy {
        match self.stage {
            LookupStage::Concrete => MemberLookupPolicy::REQUIRE_CONCRETE,
            LookupStage::IncludeDynamic => MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        }
    }

    fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        place: Place<'db>,
    ) -> DescriptorStep<'db> {
        let Place::Defined(DefinedPlace {
            ty: descr_get,
            definedness,
            ..
        }) = place
        else {
            return DescriptorStep::Complete(Ok(None));
        };

        match self.stage {
            LookupStage::Concrete => {
                // A recursive member lookup can yield the internal cycle marker. It does not
                // represent a concrete descriptor method and must not escape through the access.
                if descr_get.is_divergent() {
                    return DescriptorStep::Complete(Ok(None));
                }

                // Descriptor special-method lookup checks the descriptor's type, so instance storage
                // cannot shadow `__get__`. Dynamic MRO entries still participate in the lookup.
                DescriptorStep::Lookup(Self {
                    request: self.request,
                    stage: LookupStage::IncludeDynamic,
                })
            }
            LookupStage::IncludeDynamic => {
                let instance_ty = self.request.instance.unwrap_or_else(|| Type::none(db, env));
                DescriptorStep::DataDescriptor(PendingDataDescriptor {
                    request: self.request,
                    callable: descr_get,
                    definedness,
                    instance_ty,
                })
            }
        }
    }
}

struct PendingDataDescriptor<'db> {
    request: DescriptorRequest<'db>,
    callable: Type<'db>,
    definedness: Definedness,
    instance_ty: Type<'db>,
}

impl<'db> PendingDataDescriptor<'db> {
    fn resume(self, db: &'db dyn Db, is_data_descriptor: bool) -> DescriptorStep<'db> {
        let kind = if is_data_descriptor {
            AttributeKind::DataDescriptor
        } else {
            AttributeKind::NormalOrNonDataDescriptor
        };
        let call = DescriptorGetCallContext::new(
            db,
            self.request.ty,
            self.callable,
            self.request.instance,
            self.request.owner,
        );
        DescriptorStep::Invoke(PendingInvocation {
            callable: self.callable,
            arguments: [self.request.ty, self.instance_ty, self.request.owner],
            call,
            kind,
            definedness: self.definedness,
        })
    }
}

struct PendingInvocation<'db> {
    callable: Type<'db>,
    arguments: [Type<'db>; 3],
    call: DescriptorGetCallContext<'db>,
    kind: AttributeKind,
    definedness: Definedness,
}

impl<'db> PendingInvocation<'db> {
    fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        result: Result<Bindings<'db>, CallError<'db>>,
    ) -> DescriptorStep<'db> {
        let (bindings, error) = match result {
            Ok(bindings) => (bindings, None),
            Err(error) => (*error.1, Some(self.call)),
        };
        let origin = bindings.descriptor_origin(db, env, &self.arguments);
        let return_type = bindings.return_type(db, env);
        let return_type = if self.definedness == Definedness::AlwaysDefined {
            return_type
        } else {
            UnionType::from_two_elements(db, env, return_type, self.arguments[0])
        };

        DescriptorStep::Complete(descriptor_get_result(return_type, origin, self.kind, error))
    }
}
