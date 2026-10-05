//! Resolve constructor members before expanding the callables they refer to.
//!
//! Direct calls and conversion to `Callable` share this lookup. Dependency discovery inspects the
//! raw declarations without evaluating descriptors. Resolution preserves the actual receiver and invokes descriptors, but leaves signatures to the consumer:
//! direct calls choose constructor stages after checking arguments, while callable conversion
//! assembles their signatures before any arguments are available.

use crate::place::Place;
use crate::{Db, ProgramEnvironment};
#[cfg(test)]
use salsa::plumbing::AsId;

use super::cyclic::CallableRecursionGuard;
use super::signatures::effects::legacy_inline;
#[cfg(test)]
use super::visitor::SearchWork;
use super::visitor::{SearchControl, Unrestricted};
use super::{BoundMethodType, ClassType, DescriptorOrigin, Type};
use effects::{ConstructorError, inline_result};
use member_resolution::{ObjectInitializer, OrdinaryConstructorMembers, OrdinaryNewDescriptor};

pub(super) use super::callable::evaluation::constructor_callables;

pub(in crate::types) mod bindings;
pub(in crate::types) mod callable;
pub(in crate::types) mod effects;
pub(in crate::types) mod member_resolution;
pub(in crate::types) mod new_lookup;

#[cfg(test)]
pub(in crate::types) mod expansion_probe;
#[cfg(test)]
pub(in crate::types) mod scheduled_effects;

/// Constructor member lookup uses the class for inheritance and the actual receiver for
/// descriptor binding. Keeping both also preserves materialized protocol receivers.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct ConstructorMembers<'db> {
    pub(in crate::types) class: ClassType<'db>,
    pub(in crate::types) receiver: Type<'db>,
    pub(in crate::types) instance: Type<'db>,
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
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        self.metaclass_call_with_guard(db, env, Some(recursion_guard))
    }

    fn metaclass_call_with_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        inline_result(member_resolution::metaclass_call_with(
            self,
            &OrdinaryConstructorMembers {
                db,
                env,
                guard: recursion_guard,
            },
        ))
    }

    pub(super) fn new_method(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        self.new_method_with_guard(db, env, Some(recursion_guard))
    }

    fn new_method_with_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        inline_result(member_resolution::new_method_with(
            self,
            &OrdinaryConstructorMembers {
                db,
                env,
                guard: recursion_guard,
            },
        ))
    }

    pub(super) fn raw_initializer(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include_object: bool,
    ) -> Place<'db> {
        let policy = ObjectInitializer::from_include_object(include_object).policy();
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
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        inline_result(member_resolution::initializer_with(
            self,
            ObjectInitializer::from_include_object(include_object),
            &OrdinaryConstructorMembers {
                db,
                env,
                guard: Some(recursion_guard),
            },
        ))
    }

    pub(super) fn bind_initializer(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> Result<InitializerBinding<'db>, ConstructorError> {
        self.bind_initializer_with_guard(db, env, initializer, Some(recursion_guard))
    }

    fn bind_initializer_with_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<InitializerBinding<'db>, ConstructorError> {
        #[cfg(test)]
        if expansion_probe::active() {
            let mut control = expansion_probe::AttemptSearchControl::new(db);
            control
                .admit(SearchWork::Advance)
                .map_err(ConstructorError::Incomplete)?;
            let binding =
                self.resolve_initializer_descriptor(db, env, initializer, recursion_guard)?;
            return self
                .bind_initializer_with_control(db, env, binding, &mut control, |ty, receiver| {
                    expansion_probe::bind_initializer_self(db, env, ty, receiver)
                })
                .map_err(ConstructorError::Incomplete);
        }
        let binding = self.resolve_initializer_descriptor(db, env, initializer, recursion_guard)?;
        match self.bind_initializer_with_control(
            db,
            env,
            binding,
            &mut Unrestricted,
            |ty, receiver| Ok(ty.bind_self_typevars_after_search(db, env, receiver)),
        ) {
            Ok(binding) => Ok(binding),
            Err(error) => match error {},
        }
    }

    fn resolve_initializer_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<InitializerBinding<'db>, ConstructorError> {
        inline_result(member_resolution::resolve_initializer_descriptor_with(
            self,
            initializer,
            &OrdinaryConstructorMembers {
                db,
                env,
                guard: recursion_guard,
            },
        ))
    }

    fn bind_initializer_with_control<C: SearchControl>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut binding: InitializerBinding<'db>,
        control: &mut C,
        map: impl FnOnce(Type<'db>, Type<'db>) -> Result<Type<'db>, C::Error>,
    ) -> Result<InitializerBinding<'db>, C::Error> {
        #[cfg(test)]
        let search = expansion_probe::search_observation::initializer(
            self.class
                .static_class_literal(db)
                .map(|(class, _)| class.as_id()),
        );
        binding.callable = binding.callable.try_bind_self_typevars(
            db,
            env,
            self.instance,
            control,
            |ty, receiver| {
                #[cfg(test)]
                drop(search);
                map(ty, receiver)
            },
        )?;
        Ok(binding)
    }
}

/// A resolved constructor dependency and the descriptor calls that produced it.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct ConstructorMember<'db> {
    pub(in crate::types) place: Place<'db>,
    pub(in crate::types) origin: DescriptorOrigin<'db>,
}

impl ConstructorMember<'_> {
    pub(in crate::types) fn undefined() -> Self {
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
        legacy_inline(member_resolution::resolve_new_with(
            self,
            place,
            &OrdinaryNewDescriptor {
                db,
                env,
                guard: recursion_guard,
            },
        ))
    }
}

/// Retain a newly bound receiver so constructor synthesis can inspect an explicit `self`
/// annotation before removing it. A descriptor's returned callable is already bound.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InitializerBinding<'db> {
    pub(in crate::types) callable: Type<'db>,
    pub(in crate::types) bound_method: Option<BoundMethodType<'db>>,
    pub(in crate::types) origin: DescriptorOrigin<'db>,
}
