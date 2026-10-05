//! Semantic dependencies of constructor-callable assembly.
//!
//! Queued providers must admit each dependency explicitly. The legacy provider remains
//! immediately ready and preserves the existing synchronous lookup and recovery behavior.

use std::future::{Future, ready};

use super::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::place::Place;
#[cfg(test)]
use crate::types::callable::CallableConversionRequest;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::enums::enum_metadata;
use crate::types::generics::GenericContext;
use crate::types::signatures::effects::{legacy_inline, sealed};
use crate::types::signatures::{Parameter, Signature};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{
    BoundMethodType, CallableType, ClassType, FunctionType, MemberLookupPolicy, Type, UnionType,
};
#[cfg(test)]
use crate::types::{CallableTypes, DescriptorOrigin};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ConstructorError {
    #[cfg(test)]
    Incomplete(super::expansion_probe::Incomplete),
}

pub(in crate::types) fn inline_result<T, E>(
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    legacy_inline(async { Ok(future.await) })
}

// Source APIs can return internal recovery after a nested operation stops the attempt.
// Check before exposing such a value to a constructor continuation.
pub(in crate::types) fn checked_source<T>(
    db: &dyn Db,
    operation: impl FnOnce() -> Result<T, ConstructorError>,
) -> Result<T, ConstructorError> {
    read_source(&ConstructorSourceControl(db), operation)?
}

struct ConstructorSourceControl<'db>(&'db dyn Db);

impl SourceReadControl for ConstructorSourceControl<'_> {
    type Error = ConstructorError;

    fn check(&self) -> Result<(), ConstructorError> {
        #[cfg(not(test))]
        let _ = self.0;
        #[cfg(test)]
        if super::expansion_probe::active() {
            super::expansion_probe::continue_work(self.0).map_err(ConstructorError::Incomplete)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::types) struct ConstructorCallableRequest<'db> {
    pub(in crate::types) class: ClassType<'db>,
    pub(in crate::types) receiver: Type<'db>,
}

pub(in crate::types) trait ConstructorEffects<'db>: sealed::Sealed {
    type Error;

    async fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn metaclass_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;

    async fn new_method(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;

    async fn raw_initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> Result<Place<'db>, Self::Error>;

    async fn bind_initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        initializer: Type<'db>,
    ) -> Result<InitializerBinding<'db>, Self::Error>;

    async fn object_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> Result<Place<'db>, Self::Error>;

    #[cfg(test)]
    async fn convert(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: CallableConversionRequest<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<Option<CallableTypes<'db>>, Self::Error>;

    async fn is_actual_enum(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn class_generic_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;

    async fn expand_initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
    ) -> Result<Option<UnionType<'db>>, Self::Error>;

    async fn bind_new_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        receiver: Type<'db>,
        instance: Type<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;

    async fn initializer_self_annotation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        method: Option<BoundMethodType<'db>>,
        signature: &Signature<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn merge_generic_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Option<GenericContext<'db>>,
        right: Option<GenericContext<'db>>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;

    async fn bind_initializer_signature(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: Signature<'db>,
        method: BoundMethodType<'db>,
    ) -> Result<Signature<'db>, Self::Error>;

    async fn remove_unused_typevars(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error>;

    async fn specialize_object_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error>;

    async fn object_new_callable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
        instance: Type<'db>,
    ) -> Result<Option<CallableType<'db>>, Self::Error>;

    async fn new_return_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        return_type: Type<'db>,
        instance: Type<'db>,
    ) -> Result<bool, Self::Error>;
}

pub(super) struct LegacyInlineEffects<'a, 'db> {
    pub(super) recursion_guard: Option<&'a CallableRecursionGuard<'db>>,
}

impl sealed::Sealed for LegacyInlineEffects<'_, '_> {}

impl<'db> ConstructorEffects<'db> for LegacyInlineEffects<'_, 'db> {
    type Error = ConstructorError;

    fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(receiver.to_instance_approximation(db, env))
        }))
    }

    fn metaclass_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<ConstructorMember<'db>, Self::Error>> {
        ready(checked_source(db, || {
            members.metaclass_call_with_guard(db, env, self.recursion_guard)
        }))
    }

    fn new_method(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<ConstructorMember<'db>, Self::Error>> {
        ready(checked_source(db, || {
            members.new_method_with_guard(db, env, self.recursion_guard)
        }))
    }

    fn raw_initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(members.raw_initializer(db, env, false))
        }))
    }

    fn bind_initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        initializer: Type<'db>,
    ) -> impl Future<Output = Result<InitializerBinding<'db>, Self::Error>> {
        ready(checked_source(db, || {
            members.bind_initializer_with_guard(db, env, initializer, self.recursion_guard)
        }))
    }

    fn object_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(Type::from(members.class)
                .member_lookup_with_policy(
                    db,
                    env,
                    "__new__",
                    MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                )
                .place)
        }))
    }

    #[cfg(test)]
    fn convert(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: CallableConversionRequest<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> impl Future<Output = Result<Option<CallableTypes<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(match self.recursion_guard {
                Some(guard) => {
                    guard.with_dependency(db, origin, || request.evaluate(db, env, Some(guard)))
                }
                None => request.evaluate(db, env, None),
            })
        }))
    }

    fn is_actual_enum(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(checked_source(db, || {
            Ok(enum_metadata(db, class.class_literal(db)).is_some())
        }))
    }

    fn class_generic_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<Option<GenericContext<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(class.constructor_generic_context(db, env))
        }))
    }

    fn expand_initializer(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        initializer: Type<'db>,
    ) -> impl Future<Output = Result<Option<UnionType<'db>>, Self::Error>> {
        ready(checked_source(db, || Ok(initializer.as_union_like(db))))
    }

    fn bind_new_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        receiver: Type<'db>,
        instance: Type<'db>,
    ) -> impl Future<Output = Result<CallableType<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(callable.bind_self(db, env, receiver, instance))
        }))
    }

    fn initializer_self_annotation(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        method: Option<BoundMethodType<'db>>,
        signature: &Signature<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(method
                .filter(|method| !method.class_method(db))
                .and_then(|_| signature.parameters().get_positional(0))
                .filter(|parameter| !parameter.inferred_annotation)
                .map(Parameter::annotated_type)
                .filter(|ty| {
                    ty.as_typevar()
                        .is_none_or(|bound_typevar| !bound_typevar.typevar(db).is_self(db))
                }))
        }))
    }

    fn merge_generic_context(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        left: Option<GenericContext<'db>>,
        right: Option<GenericContext<'db>>,
    ) -> impl Future<Output = Result<Option<GenericContext<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(GenericContext::merge_optional(db, left, right))
        }))
    }

    fn bind_initializer_signature(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: Signature<'db>,
        method: BoundMethodType<'db>,
    ) -> impl Future<Output = Result<Signature<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(signature.bind_self_with_receiver(
                db,
                env,
                Some(method.signature_receiver(db)),
                Some(method.typing_self_type(db)),
            ))
        }))
    }

    fn remove_unused_typevars(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: Signature<'db>,
    ) -> impl Future<Output = Result<Signature<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(signature.remove_unused_typevars(db, env))
        }))
    }

    fn specialize_object_new(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> impl Future<Output = Result<FunctionType<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(function.with_inherited_generic_context(db, context))
        }))
    }

    fn object_new_callable(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
        instance: Type<'db>,
    ) -> impl Future<Output = Result<Option<CallableType<'db>>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(function
                .into_bound_method_type(db, instance)
                .into_callable_type(db))
        }))
    }

    fn new_return_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        return_type: Type<'db>,
        instance: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(checked_source(db, || {
            Ok(return_type.is_assignable_to(db, env, instance))
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::constructor::callable::constructor_callables_with;
    use crate::types::constructor::constructor_callables;
    use crate::types::signatures::effects::try_poll_immediate;

    #[test]
    fn constructor_inline_driver_stays_ready() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/constructors.py",
            r#"
            from enum import Enum

            class Empty: ...

            class Initializer:
                def __init__(self, value: int) -> None: ...

            class Generic[T]:
                def __init__(self, value: T) -> None: ...

            class ExplicitSelf[T]:
                def __init__(self: ExplicitSelf[int], value: T) -> None: ...

            class ForeignNew:
                def __new__(cls, value: str) -> int: ...
                def __init__(self, value: bytes) -> None: ...

            class Meta(type):
                def __call__(cls, value: bytes) -> str: ...

            class Metaclass(metaclass=Meta): ...

            class Color(Enum):
                RED = "red"
            "#,
        )?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/constructors.py")?,
            env.program(&db),
        );
        for name in [
            "Empty",
            "Initializer",
            "Generic",
            "ExplicitSelf",
            "ForeignNew",
            "Metaclass",
            "Color",
        ] {
            let receiver = global_symbol(&db, file, name).place.expect_type();
            let Type::ClassLiteral(class) = receiver else {
                anyhow::bail!("expected a class for {name}");
            };
            let class = class.identity_specialization(&db);
            let receiver = Type::from(class);
            let request = ConstructorCallableRequest { class, receiver };
            let guard = CallableRecursionGuard::new();
            let expected = constructor_callables(&db, &env, request.class, receiver, &guard);
            let effects = LegacyInlineEffects {
                recursion_guard: Some(&guard),
            };
            let actual =
                try_poll_immediate(constructor_callables_with(&db, &env, &effects, request));
            assert!(
                matches!(actual, Poll::Ready(Ok(callables)) if callables == expected),
                "{name}"
            );
        }
        Ok(())
    }
}
