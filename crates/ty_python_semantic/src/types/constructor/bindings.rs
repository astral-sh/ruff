//! Build constructor call bindings through shared member, callable, and storage effects.

use super::member_resolution::ObjectInitializer;
use super::{ConstructorMember, ConstructorMembers};
use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::call::Bindings;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::constructor_preparation::ConstructorBindingStorageEffects;
use crate::types::call::bindings::InlineBindingsEffects;
use crate::types::class::CodeGeneratorKind;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::{ClassLiteral, ClassType, GenericContext, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

/// Metadata and member effects for the constructor preparation algorithm.
///
/// The same recipe selects constructor fallbacks and orders metaclass, `__new__`, and
/// `__init__` bindings in ordinary execution and in a controlled source invocation.
pub(in crate::types) trait ConstructorBindingsEffects<'db>:
    ConstructorBindingStorageEffects<'db>
{
    async fn decision<T: Copy>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error>;

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error>;

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;

    async fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn generated_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn known(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;

    async fn enum_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error>;

    async fn is_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn identity_class(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    async fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn to_class_type(
        &self,
        db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error>;

    async fn metaclass_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;

    async fn new_method(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;

    async fn initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        include_object: ObjectInitializer,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;
}

impl<'db> ConstructorBindingsEffects<'db> for InlineBindingsEffects<'_, 'db> {
    async fn decision<T: Copy>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(class.class_literal(db))
    }

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.generic_context(db))
    }

    async fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.is_typed_dict(db))
    }

    async fn generated_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(CodeGeneratorKind::TypedDict.matches(db, class))
    }

    async fn known(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(db))
    }

    async fn enum_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(KnownClass::Enum.to_class_literal(db, env).to_class_type(db))
    }

    async fn is_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.is_subclass_of(db, env, target))
    }

    async fn identity_class(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.identity_specialization(db))
    }

    async fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(receiver.to_instance_approximation(db, env))
    }

    async fn to_class_type(
        &self,
        db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(receiver.to_class_type(db))
    }

    async fn metaclass_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error> {
        members.metaclass_call(db, env, guard)
    }

    async fn new_method(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error> {
        members.new_method(db, env, guard)
    }

    async fn initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        include_object: ObjectInitializer,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error> {
        members.initializer(
            db,
            env,
            match include_object {
                ObjectInitializer::Include => true,
                ObjectInitializer::Exclude => false,
            },
            guard,
        )
    }
}

/// Produces the gradual bindings used by constructor families with bespoke call validation.
async fn fallback_bindings<'db, E: ConstructorBindingsEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Type<'db>,
    context: Option<GenericContext<'db>>,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    let instance = effects.instance_approximation(db, env, receiver).await?;
    let return_type = effects
        .decision(|| instance.unwrap_or(Type::unknown()))
        .await?;
    let bindings = effects.fallback(receiver, context, return_type).await?;
    effects.transfer(&bindings).await?;
    Ok(bindings)
}

/// Builds constructor bindings before argument matching by combining metaclass `__call__`,
/// `__new__`, and `__init__` signatures.
/// Returns fallback bindings for cases that intentionally keep bespoke call behavior.
pub(in crate::types) async fn constructor_bindings_with<'db, E: ConstructorBindingsEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Type<'db>,
    class: ClassType<'db>,
    recursion_guard: &CallableRecursionGuard<'db>,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    let class_literal = effects.class_literal(db, class).await?;
    let class_generic_context = effects.generic_context(db, class_literal).await?;

    // Specialized and non-generic TypedDict constructors use their dedicated validation.
    // An unspecialized generic constructor also needs its real `__init__` signature so
    // ordinary call inference can solve the class type variables.
    let typed_dict = effects.is_typed_dict(db, class_literal).await?
        || effects.generated_typed_dict(db, class_literal).await?;
    if effects
        .decision(|| {
            typed_dict
                && (!matches!(receiver, Type::ClassLiteral(_)) || class_generic_context.is_none())
        })
        .await?
    {
        return fallback_bindings(db, env, receiver, class_generic_context, effects).await;
    }

    // These cases are checked in `Type::known_class_literal_bindings`, but currently we only
    // call that for `ClassLiteral` types, so we need a permissive fallback here. TODO Ideally
    // that would be called from `constructor_bindings` for better consistency, but that causes
    // some test failures deserving separate investigation.
    let known = effects.known(db, class).await?;
    if effects
        .decision(|| {
            matches!(
                known,
                Some(
                    KnownClass::Bool
                        | KnownClass::Type
                        | KnownClass::Object
                        | KnownClass::FunctoolsPartial
                        | KnownClass::Property
                        | KnownClass::Super
                        | KnownClass::TypeAliasType
                        | KnownClass::ExtensionsTypeAliasType
                        | KnownClass::Deprecated
                )
            )
        })
        .await?
    {
        return fallback_bindings(db, env, receiver, class_generic_context, effects).await;
    }

    // Temporary special-casing for all subclasses of `enum.Enum` until we support the
    // functional syntax for creating enum classes. TODO we should ideally check e.g.
    // `MyEnum(1)` to make sure `1` is a valid value for `MyEnum`.
    if let Some(enum_class) = effects.enum_class(db, env).await?
        && effects.is_subclass(db, env, class, enum_class).await?
    {
        return fallback_bindings(db, env, receiver, class_generic_context, effects).await;
    }

    // If we are trying to construct a non-specialized generic class, we should use the
    // constructor parameters to try to infer the class specialization. To do this, we need to
    // tweak our member lookup logic a bit. Normally, when looking up a class or instance
    // member, we first apply the class's default specialization, and apply that specialization
    // to the type of the member. To infer a specialization from the argument types, we need to
    // have the class's typevars still in the method signature when we attempt to call it. To
    // do this, we instead use the _identity_ specialization, which maps each of the class's
    // generic typevars to itself.
    let self_type = if let Type::ClassLiteral(class) = receiver
        && effects.generic_context(db, class).await?.is_some()
    {
        let identity = effects.identity_class(db, class).await?;
        effects.decision(|| Type::from(identity)).await?
    } else {
        effects.decision(|| receiver).await?
    };

    let Some(constructor_instance_ty) = effects.instance_approximation(db, env, self_type).await?
    else {
        return fallback_bindings(db, env, receiver, class_generic_context, effects).await;
    };
    let receiver_class = effects.to_class_type(db, self_type).await?;
    let members = effects
        .decision(|| ConstructorMembers {
            class: receiver_class.unwrap_or(class),
            receiver: self_type,
            instance: constructor_instance_ty,
        })
        .await?;

    // Check for a custom `__call__` on the metaclass (excluding `type.__call__`).
    // We preserve its full overload set here and defer constructor branching decisions
    // until call-time overload resolution.
    let metaclass_dunder_call = effects
        .metaclass_call(db, env, members, recursion_guard)
        .await?;

    // TypedDict classes inherit `dict.__new__`, whose gradual `**kwargs` signature cannot
    // constrain their type variables. Their synthesized `__init__` contains the actual field
    // types, including generic extra items, so constructor inference should start there.
    let new_method = if effects.is_typed_dict(db, class_literal).await? {
        effects.decision(ConstructorMember::undefined).await?
    } else {
        effects
            .new_method(db, env, members, recursion_guard)
            .await?
    };
    let init_method_no_object = effects
        .initializer(
            db,
            env,
            members,
            ObjectInitializer::Exclude,
            recursion_guard,
        )
        .await?;

    let new_bindings = if let Place::Defined(DefinedPlace {
        ty: new_callable,
        definedness,
        ..
    }) = new_method.place
    {
        let mut bindings = effects
            .bindings_from_descriptor(db, env, new_callable, new_method.origin)
            .await?;
        effects
            .bind_new(db, env, &mut bindings, self_type, constructor_instance_ty)
            .await?;
        effects
            .wrap_constructor(
                db,
                &mut bindings,
                constructor_instance_ty,
                ConstructorCallableKind::New,
            )
            .await?;
        if definedness == Definedness::PossiblyUndefined {
            effects
                .mark_unbound(&mut bindings, ConstructorCallableKind::New)
                .await?;
        }
        Some(bindings)
    } else {
        None
    };

    // Only fall back to `object.__init__` when `__new__` is absent.
    let init_method = if effects
        .decision(|| init_method_no_object.place.is_undefined() && new_bindings.is_none())
        .await?
    {
        effects
            .initializer(
                db,
                env,
                members,
                ObjectInitializer::Include,
                recursion_guard,
            )
            .await?
    } else {
        init_method_no_object
    };
    let mut init_bindings = match init_method.place {
        Place::Defined(DefinedPlace {
            ty: init_callable,
            definedness,
            ..
        }) => {
            let mut bindings = effects
                .bindings_from_descriptor(db, env, init_callable, init_method.origin)
                .await?;
            effects
                .wrap_constructor(
                    db,
                    &mut bindings,
                    constructor_instance_ty,
                    ConstructorCallableKind::Init,
                )
                .await?;
            if definedness == Definedness::PossiblyUndefined {
                effects
                    .mark_unbound(&mut bindings, ConstructorCallableKind::Init)
                    .await?;
            }
            Some(bindings)
        }
        Place::Undefined if new_bindings.is_none() => {
            // If we are using vendored typeshed, it should be impossible to have missing
            // or unbound `__init__` method on a class, as all classes have `object` in MRO.
            // Thus the following may only trigger if a custom typeshed is used.
            // Custom/broken typeshed: no `__init__` available even after falling back
            // to `object`. Keep analysis going and surface the missing-implicit-call
            // lint via the builder.
            let mut bindings = effects
                .fallback(self_type, None, constructor_instance_ty)
                .await?;
            effects
                .wrap_constructor(
                    db,
                    &mut bindings,
                    constructor_instance_ty,
                    ConstructorCallableKind::Init,
                )
                .await?;
            effects
                .mark_unbound(&mut bindings, ConstructorCallableKind::Init)
                .await?;
            Some(bindings)
        }
        Place::Undefined => None,
    };
    if let Some(bindings) = &mut init_bindings {
        effects.bind_initializer_self(db, env, bindings).await?;
    }

    let constructor_bindings = if let Some(mut new_bindings) = new_bindings {
        // Preserve the full `__new__` signature and defer `__init__` validation until we know
        // which `__new__` overload matched at call time.
        if let Some(init_bindings) = &init_bindings {
            effects
                .attach_downstream(&mut new_bindings, init_bindings)
                .await?;
        }
        Some(new_bindings)
    } else {
        init_bindings
    };

    let mut bindings = if let Place::Defined(DefinedPlace {
        ty: metaclass_call_method,
        ..
    }) = metaclass_dunder_call.place
    {
        let mut metaclass_bindings = effects
            .bindings_from_descriptor(db, env, metaclass_call_method, metaclass_dunder_call.origin)
            .await?;
        effects
            .wrap_constructor(
                db,
                &mut metaclass_bindings,
                constructor_instance_ty,
                ConstructorCallableKind::MetaclassCall,
            )
            .await?;
        if let Some(downstream_bindings) = &constructor_bindings {
            // Preserve the full metaclass `__call__` signature and defer whether constructor
            // downstream checks apply until the matched overload is known.
            effects
                .attach_downstream(&mut metaclass_bindings, downstream_bindings)
                .await?;
        }
        metaclass_bindings
    } else if let Some(constructor_bindings) = constructor_bindings {
        constructor_bindings
    } else {
        return fallback_bindings(db, env, receiver, class_generic_context, effects).await;
    };

    effects
        .apply_class_context(db, &mut bindings, class_generic_context)
        .await?;
    effects.transfer(&bindings).await?;
    Ok(bindings)
}
