//! Resolve constructor members before expanding the callables they refer to.
//!
//! Direct calls and conversion to `Callable` share this lookup. Dependency discovery inspects the
//! raw declarations without evaluating descriptors. Resolution preserves the actual receiver and invokes descriptors, but leaves signatures to the consumer:
//! direct calls choose constructor stages after checking arguments, while callable conversion
//! assembles their signatures before any arguments are available.

use crate::place::{DefinedPlace, Place, Provenance};
use crate::{Db, ProgramEnvironment};

use super::cyclic::CallableRecursionGuard;
use super::{BoundMethodType, ClassType, DescriptorOrigin, DynamicType, MemberLookupPolicy, Type};

pub(super) use super::callable::evaluation::constructor_callables;

pub(in crate::types) mod callable;

/// Constructor member lookup uses the class for inheritance and the actual receiver for
/// descriptor binding. Keeping both also preserves materialized protocol receivers.
#[derive(Clone, Copy, Debug)]
pub(super) struct ConstructorMembers<'db> {
    class: ClassType<'db>,
    receiver: Type<'db>,
    pub(super) instance: Type<'db>,
}

impl<'db> ConstructorMembers<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        receiver: Type<'db>,
    ) -> Self {
        Self {
            class,
            receiver,
            instance: receiver
                .to_instance_approximation(db, env)
                .unwrap_or(Type::unknown()),
        }
    }

    pub(super) fn metaclass_call(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorMember<'db> {
        let lookup_type = Type::from(self.class);
        let member = lookup_type
            .member_lookup_with_recursion_guard(
                db,
                env,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                if self.receiver == lookup_type {
                    None
                } else {
                    Some(self.receiver)
                },
                Some(recursion_guard),
            )
            .unwrap_or_else(|error| error.fallback_member(db));
        ConstructorMember {
            place: member.member(db).place,
            origin: member.descriptor_origin(db),
        }
    }

    pub(super) fn new_method(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorMember<'db> {
        let Some(member) = Type::from(self.class).lookup_dunder_new(db, env) else {
            return ConstructorMember::undefined();
        };
        self.receiver
            .resolve_dunder_new_callable(db, env, member.place, Some(recursion_guard))
    }

    pub(super) fn raw_initializer(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include_object: bool,
    ) -> Place<'db> {
        let policy = if include_object {
            MemberLookupPolicy::NO_INSTANCE_FALLBACK
        } else {
            MemberLookupPolicy::NO_INSTANCE_FALLBACK | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
        };
        Type::from(self.class)
            .class_namespace_member(db, env, self.class, "__init__", policy)
            .place
    }

    pub(super) fn initializer(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include_object: bool,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorMember<'db> {
        match self.raw_initializer(db, env, include_object) {
            Place::Defined(place) => {
                let initializer = self.bind_initializer(db, env, place.ty, recursion_guard);
                ConstructorMember {
                    place: Place::Defined(DefinedPlace {
                        ty: initializer
                            .bound_method
                            .map_or(initializer.callable, Type::BoundMethod),
                        ..place
                    }),
                    origin: initializer.origin,
                }
            }
            Place::Undefined => ConstructorMember::undefined(),
        }
    }

    pub(super) fn bind_initializer(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> InitializerBinding<'db> {
        let mut binding = match initializer.function_like_dunder_get(
            db,
            env,
            Some(self.instance),
            Some(self.receiver),
        ) {
            Some(Type::BoundMethod(method)) => InitializerBinding {
                callable: method.func(db),
                bound_method: Some(method),
                origin: DescriptorOrigin::default(),
            },
            Some(callable) => InitializerBinding {
                callable,
                bound_method: None,
                origin: DescriptorOrigin::default(),
            },
            None => {
                let descriptor = initializer
                    .try_call_dunder_get_with_recursion_guard(
                        db,
                        env,
                        Some(self.instance),
                        self.receiver,
                        Some(recursion_guard),
                    )
                    .unwrap_or_else(|error| Some(error.fallback()));
                InitializerBinding {
                    callable: descriptor.map_or(initializer, |descriptor| descriptor.return_type),
                    bound_method: None,
                    origin: descriptor
                        .map_or(DescriptorOrigin::default(), |descriptor| descriptor.origin),
                }
            }
        };
        binding.callable = binding.callable.bind_self_typevars(db, env, self.instance);
        binding
    }
}

/// A resolved constructor dependency and the descriptor calls that produced it.
pub(super) struct ConstructorMember<'db> {
    pub(super) place: Place<'db>,
    pub(super) origin: DescriptorOrigin<'db>,
}

impl ConstructorMember<'_> {
    pub(super) fn undefined() -> Self {
        Self {
            place: Place::Undefined,
            origin: DescriptorOrigin::default(),
        }
    }
}

impl<'db> Type<'db> {
    /// Resolves a `__new__` descriptor before the constructor supplies its implicit `cls`.
    pub(super) fn resolve_dunder_new_callable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        place: Place<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> ConstructorMember<'db> {
        let Place::Defined(defined) = place else {
            return ConstructorMember::undefined();
        };
        // If `__new__` itself resolved to `Any`, treat it as absent rather than as a real
        // constructor override. This preserves the known nominal constructor result for
        // subclasses of `Any` while still allowing explicitly typed `__new__` callables
        // returning `Any` to keep their annotated behavior.
        if matches!(defined.ty, Type::Dynamic(DynamicType::Any)) {
            return ConstructorMember::undefined();
        }
        let descriptor = defined
            .ty
            .try_call_dunder_get_with_recursion_guard(db, env, None, self, recursion_guard)
            .unwrap_or_else(|error| Some(error.fallback()));
        match descriptor {
            Some(descriptor) => ConstructorMember {
                place: Place::Defined(DefinedPlace {
                    ty: descriptor.return_type,
                    provenance: Provenance::Unknown,
                    ..defined
                }),
                origin: descriptor.origin,
            },
            None => ConstructorMember {
                place,
                origin: DescriptorOrigin::default(),
            },
        }
    }
}

/// Retain a newly bound receiver so constructor synthesis can inspect an explicit `self`
/// annotation before removing it. A descriptor's returned callable is already bound.
#[derive(Clone, Copy, Debug)]
pub(super) struct InitializerBinding<'db> {
    pub(super) callable: Type<'db>,
    pub(super) bound_method: Option<BoundMethodType<'db>>,
    pub(super) origin: DescriptorOrigin<'db>,
}
