//! Select call bindings through `BindingPreparationEffects` for ordinary execution and controlled
//! source inference.
//!
//! Ordinary execution retains its `CallableRecursionGuard`. The controlled implementation in
//! `infer::builder::local::source::call` refuses dependencies that do not yet support its execution
//! budget.

pub(in crate::types) mod bound_method;
pub(in crate::types) mod known_class;

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::super::*;

pub(in crate::types) enum BindingPreparationDependency<'db> {
    Recursive(RecursiveType<'db>),
    UnboundRecursive,
    Callable(CallableType<'db>),
    TypeVar(BoundTypeVarInstance<'db>),
    BoundMethod(BoundMethodType<'db>),
    KnownBoundMethod(KnownBoundMethodType<'db>),
    WrapperDescriptor(WrapperDescriptorKind),
    DataclassTransformer,
    SubclassOf(SubclassOfType<'db>),
    InitVar,
    Instance,
    Dynamic,
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    EnumComplement(EnumComplementType<'db>),
    DataclassDecorator,
    SpecialForm,
    LiteralValue(LiteralValueType<'db>),
    KnownInstance(KnownInstanceType<'db>),
    TypeAlias(TypeAliasType<'db>),
    NotCallable,
}

pub(in crate::types) struct BindingPreparationFacts;

shared_semantic_family! {
    #[synchronous(SynchronousBindingPreparationEffects)]
    pub(in crate::types) trait BindingPreparationEffects<'db> {
        type Error;

        #[operation(child)]
        async fn guarded(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, unknown_is_recovery: bool) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn body(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, unknown_is_recovery: bool) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn forward(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, unknown_is_recovery: bool) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn function(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, function: FunctionType<'db>) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn known_class(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, class: ClassLiteral<'db>) -> Result<Option<Bindings<'db>>, Self::Error>;
        #[operation(child)]
        async fn constructor(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, class: ClassType<'db>) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn dependency(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, dependency: BindingPreparationDependency<'db>, unknown_is_recovery: bool) -> Result<Bindings<'db>, Self::Error>;
    }

    #[finite_capability]
    impl BindingPreparationFacts {
        fn direct_signature(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::FunctionLiteral(_) | Type::Callable(_))
        }

        fn divergent_fallback<'db>(&self, ty: Type<'db>) -> Option<Type<'db>> {
            ty.materialized_divergent_fallback()
        }
    }

    #[synchronous(bindings_sync)]
    #[capabilities(effects = BindingPreparationEffects, facts = BindingPreparationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn bindings_with<'db, E: BindingPreparationEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
        facts: BindingPreparationFacts,
        effects: &E,
    ) -> Result<Bindings<'db>, E::Error> {
        // Reading these signatures does not expand another callable. `Type::try_call` passes the
        // invocation's guard to the later builtin-delegation check in `Bindings::evaluate_known_cases`.
        if facts.direct_signature(ty) {
            effects.body(db, env, ty, unknown_is_recovery).await
        } else {
            effects.guarded(db, env, ty, unknown_is_recovery).await
        }
    }

    #[synchronous(bindings_body_sync)]
    #[capabilities(effects = BindingPreparationEffects, facts = BindingPreparationFacts)]
    #[passive_values(ClassType::NonGeneric, ClassType::Generic, BindingPreparationDependency::Recursive, BindingPreparationDependency::UnboundRecursive, BindingPreparationDependency::Callable, BindingPreparationDependency::TypeVar, BindingPreparationDependency::BoundMethod, BindingPreparationDependency::KnownBoundMethod, BindingPreparationDependency::WrapperDescriptor, BindingPreparationDependency::DataclassTransformer, BindingPreparationDependency::SubclassOf, BindingPreparationDependency::InitVar, BindingPreparationDependency::Instance, BindingPreparationDependency::Dynamic, BindingPreparationDependency::Union, BindingPreparationDependency::Intersection, BindingPreparationDependency::EnumComplement, BindingPreparationDependency::DataclassDecorator, BindingPreparationDependency::SpecialForm, BindingPreparationDependency::LiteralValue, BindingPreparationDependency::KnownInstance, BindingPreparationDependency::TypeAlias, BindingPreparationDependency::NotCallable)]
    pub(in crate::types) async fn bindings_body_with<'db, E: BindingPreparationEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
        facts: BindingPreparationFacts,
        effects: &E,
    ) -> Result<Bindings<'db>, E::Error> {
        if let Some(fallback) = facts.divergent_fallback(ty) {
            return effects.forward(db, env, fallback, unknown_is_recovery).await;
        }

        let dependency = match ty {
            Type::Recursive(recursive) => BindingPreparationDependency::Recursive(recursive),
            Type::RecursiveVar(_) => BindingPreparationDependency::UnboundRecursive,
            Type::Callable(callable) => BindingPreparationDependency::Callable(callable),
            Type::TypeVar(typevar) => BindingPreparationDependency::TypeVar(typevar),
            Type::BoundMethod(method) => BindingPreparationDependency::BoundMethod(method),
            Type::KnownBoundMethod(method) => BindingPreparationDependency::KnownBoundMethod(method),
            Type::WrapperDescriptor(descriptor) => BindingPreparationDependency::WrapperDescriptor(descriptor),
            Type::DataclassTransformer(_) => BindingPreparationDependency::DataclassTransformer,
            Type::FunctionLiteral(function) => return effects.function(db, env, function).await,
            Type::ClassLiteral(class) => {
                // TODO this should be called from `constructor_bindings` for better consistency
                if let Some(bindings) = effects.known_class(db, env, ty, class).await? {
                    return Ok(bindings);
                }
                return effects.constructor(db, env, ty, ClassType::NonGeneric(class)).await;
            }
            Type::GenericAlias(alias) => return effects.constructor(db, env, ty, ClassType::Generic(alias)).await,
            Type::SubclassOf(subclass) => BindingPreparationDependency::SubclassOf(subclass),
            Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar)) => BindingPreparationDependency::InitVar,
            Type::NominalInstance(_) | Type::ProtocolInstance(_) | Type::NewTypeInstance(_) => BindingPreparationDependency::Instance,
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => BindingPreparationDependency::Dynamic,
            Type::Union(union) => BindingPreparationDependency::Union(union),
            Type::Intersection(intersection) => BindingPreparationDependency::Intersection(intersection),
            Type::EnumComplement(complement) => BindingPreparationDependency::EnumComplement(complement),
            Type::DataclassDecorator(_) => BindingPreparationDependency::DataclassDecorator,
            Type::SpecialForm(_) => BindingPreparationDependency::SpecialForm,
            Type::LiteralValue(literal) => BindingPreparationDependency::LiteralValue(literal),
            Type::KnownInstance(instance) => BindingPreparationDependency::KnownInstance(instance),
            Type::TypeAlias(alias) => BindingPreparationDependency::TypeAlias(alias),
            Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BoundSuper(_)
            | Type::ModuleLiteral(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => BindingPreparationDependency::NotCallable,
        };
        effects.dependency(db, env, ty, dependency, unknown_is_recovery).await
    }
}

pub(in crate::types) struct OrdinaryBindingPreparationEffects<'guard, 'db> {
    pub(in crate::types) recursion_guard: &'guard CallableRecursionGuard<'db>,
}

impl<'db> SynchronousBindingPreparationEffects<'db> for OrdinaryBindingPreparationEffects<'_, 'db> {
    type Error = Infallible;

    fn guarded(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error> {
        let on_cycle = || Ok(Binding::single(ty, Signature::unknown()).into());
        self.recursion_guard.visit(
            db,
            env,
            (CallableExpansion::Bindings, ty),
            on_cycle,
            || Ok(Binding::single(ty, Signature::recursion_recovery()).into()),
            #[cfg(test)]
            || Ok(Binding::single(ty, Signature::unknown()).into()),
            || SynchronousBindingPreparationEffects::body(self, db, env, ty, unknown_is_recovery),
        )
    }

    fn body(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error> {
        bindings_body_sync(
            db,
            env,
            ty,
            unknown_is_recovery,
            BindingPreparationFacts,
            self,
        )
    }

    fn forward(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(ty.bindings_with_recovery(db, env, self.recursion_guard, unknown_is_recovery))
    }

    fn function(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(call::function_bindings::function_bindings(
            db, env, function,
        ))
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<Bindings<'db>>, Self::Error> {
        Ok(known_class::known_class_bindings(db, env, ty, class))
    }

    fn constructor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassType<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(ty.constructor_bindings(db, env, class, self.recursion_guard))
    }

    fn dependency(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        dependency: BindingPreparationDependency<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(self.dependency_bindings(db, env, ty, dependency, unknown_is_recovery))
    }
}

impl<'db> OrdinaryBindingPreparationEffects<'_, 'db> {
    fn dependency_bindings(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable_type: Type<'db>,
        dependency: BindingPreparationDependency<'db>,
        unknown_is_recovery: bool,
    ) -> Bindings<'db> {
        let recursion_guard = self.recursion_guard;
        match dependency {
            BindingPreparationDependency::Recursive(recursive) => recursive
                .unfold(db, env)
                .map(|unfolded| {
                    unfolded.bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                })
                .unwrap_or_else(|| CallableBinding::not_callable(callable_type).into()),
            BindingPreparationDependency::UnboundRecursive => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            BindingPreparationDependency::Callable(callable) => CallableBinding::from_overloads(
                callable_type,
                callable.signatures(db).iter().cloned(),
            )
            .into(),

            BindingPreparationDependency::TypeVar(bound_typevar) => {
                match bound_typevar.require_bound_or_constraints(db, env) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        bound.bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => Bindings::from_union(
                        callable_type,
                        constraints.elements(db).iter().map(|ty| {
                            ty.bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                        }),
                    ),
                }
            }

            BindingPreparationDependency::BoundMethod(method) => {
                bound_method::bound_method_bindings(
                    db,
                    env,
                    callable_type,
                    method,
                    unknown_is_recovery,
                    recursion_guard,
                )
            }

            // Keep the receiver constraints and known-function handling of a direct call.
            BindingPreparationDependency::KnownBoundMethod(KnownBoundMethodType::DunderCall(
                callable,
            )) => callable
                .inner(db)
                .bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                .with_callable_type(callable_type),
            BindingPreparationDependency::KnownBoundMethod(method) => {
                method.callables(db, env).map_or_else(
                    || CallableBinding::not_callable(callable_type).into(),
                    |callables| {
                        Bindings::from_union(
                            callable_type,
                            callables.iter().map(|callable| {
                                CallableBinding::from_overloads(
                                    callable_type,
                                    callable.signatures(db).iter().cloned(),
                                )
                                .into()
                            }),
                        )
                    },
                )
            }

            BindingPreparationDependency::WrapperDescriptor(wrapper_descriptor) => {
                known_class::wrapper_descriptor_bindings(db, env, callable_type, wrapper_descriptor)
            }

            // TODO: We should probably also check the original return type of the function
            // that was decorated with `@dataclass_transform`, to see if it is consistent with
            // with what we configure here.
            BindingPreparationDependency::DataclassTransformer => Binding::single(
                callable_type,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "func",
                    )))
                    .with_annotated_type(Type::object())]),
                    Type::unknown(),
                ),
            )
            .into(),

            BindingPreparationDependency::SubclassOf(subclass_of_type) => match subclass_of_type
                .subclass_of()
            {
                SubclassOfInner::Dynamic(dynamic_type) => Binding::single(
                    callable_type,
                    Signature::dynamic(Type::Dynamic(dynamic_type)),
                )
                .into(),
                SubclassOfInner::Class(class) => {
                    callable_type.constructor_bindings(db, env, class, recursion_guard)
                }
                SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map_or_else(
                    || Binding::single(callable_type, Signature::dynamic(Type::unknown())).into(),
                    |origin| {
                        let bindings =
                            callable_type.constructor_bindings(db, env, *origin, recursion_guard);
                        if protocol.materialization_kind(db).is_some() {
                            bindings.with_constructed_instance_type(
                                db,
                                Type::ProtocolInstance(protocol),
                            )
                        } else {
                            bindings
                        }
                    },
                ),
                SubclassOfInner::TypeVar(tvar) => {
                    let constructor_instance_type = Type::TypeVar(tvar);
                    let bindings = match tvar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            let constructor = bound.constructor_for_typevar_bound(db, env);
                            if let Type::ClassLiteral(class) = constructor
                                && let Some(bindings) =
                                    callable_type.known_class_literal_bindings(db, env, class)
                            {
                                bindings
                            } else {
                                constructor.bindings_impl(db, env, recursion_guard)
                            }
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            Bindings::from_union(
                                callable_type,
                                constraints.elements(db).iter().map(|ty| {
                                    ty.to_meta_type(db, env)
                                        .bindings_impl(db, env, recursion_guard)
                                }),
                            )
                        }
                    };
                    // Some built-in constructors, including `object`, are special-cased as regular
                    // callable bindings. Wrap them so that every bound or constrained call has
                    // constructor context and constructs `T`; existing constructor bindings keep
                    // their original kind.
                    bindings
                        .into_constructor_bindings(
                            constructor_instance_type,
                            ConstructorCallableKind::MetaclassCall,
                        )
                        .with_constructed_instance_type(db, constructor_instance_type)
                }
            },

            BindingPreparationDependency::InitVar => {
                let parameter = Parameter::positional_or_keyword(Name::new_static("type"))
                    .with_annotated_type(Type::any());
                let signature = Signature::new(Parameters::standard([parameter]), Type::any());
                Binding::single(callable_type, signature).into()
            }

            BindingPreparationDependency::Instance => {
                constructor::effects::inline_result(call::bindings::instance_bindings_with(
                    db,
                    env,
                    callable_type,
                    recursion_guard,
                    &call::bindings::InlineBindingsEffects { recursion_guard },
                ))
                .unwrap_or_else(|error| match error {
                    #[cfg(test)]
                    constructor::effects::ConstructorError::Incomplete(_) => {
                        Binding::single(callable_type, Signature::unknown()).into()
                    }
                })
            }

            // Dynamic types are callable, and the return type is the same dynamic type. Similarly,
            // `Never` is always callable and returns `Never`.
            BindingPreparationDependency::Dynamic => {
                let signature = if callable_type.is_unknown() && unknown_is_recovery {
                    Signature::recursion_recovery()
                } else {
                    Signature::dynamic(callable_type)
                };
                Binding::single(callable_type, signature).into()
            }

            // Note that the result is not callable if none of the union elements are callable.
            BindingPreparationDependency::Union(union) => Bindings::from_union(
                callable_type,
                union.elements(db).iter().map(|element| {
                    element.bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                }),
            ),

            // A narrowed `type[T: Base] & type[Child]` still needs to construct `T & Child`,
            // but its constructor must come from `Child`, not from `Base` as an independent,
            // competing alternative. Flattening the projected instance lets intersection
            // simplification select that constructor without discarding unrelated providers.
            BindingPreparationDependency::Intersection(intersection)
                if intersection.positive(db).iter().all(|element| {
                    // A metaclass instance also has an instance-space projection, but it can
                    // provide an independent `__call__`. Only simplify actual class-object
                    // variants so `type[Base] & Meta` retains both callable candidates.
                    matches!(
                        element.resolve_type_alias(db),
                        Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
                    )
                }) && let Some(instance_type) =
                    callable_type.to_instance_approximation(db, env)
                    && let Type::NominalInstance(lookup_instance) =
                        instance_type.flatten_typevars(db, env)
                    && let Some(bindings) = {
                        let bindings = lookup_instance.to_meta_type(db, env).bindings_impl(
                            db,
                            env,
                            recursion_guard,
                        );
                        bindings.has_only_constructor_items().then_some(bindings)
                    } =>
            {
                bindings
                    .with_constructed_instance_type(db, instance_type)
                    .with_callable_type(callable_type)
            }

            BindingPreparationDependency::Intersection(intersection) => {
                Bindings::from_intersection(
                    callable_type,
                    intersection.positive_elements_or_object(db).map(|element| {
                        element.bindings_with_recovery(
                            db,
                            env,
                            recursion_guard,
                            unknown_is_recovery,
                        )
                    }),
                )
            }

            BindingPreparationDependency::EnumComplement(complement) => complement
                .to_intersection(db, env)
                .bindings_impl(db, env, recursion_guard),

            BindingPreparationDependency::DataclassDecorator => {
                let typevar = BoundTypeVarInstance::synthetic(
                    db,
                    env,
                    Name::new_static("T"),
                    TypeVarVariance::Invariant,
                );
                let typevar_meta = SubclassOfType::from(db, env, typevar);
                let context = GenericContext::from_typevar_instances(db, env, [typevar]);
                let parameters = [Parameter::positional_only(Some(Name::new_static("cls")))
                    .with_annotated_type(typevar_meta)];
                // Intersect with `Any` for the return type to reflect the fact that the `dataclass()`
                // decorator adds methods to the class
                let returns =
                    IntersectionType::from_two_elements(db, env, typevar_meta, Type::any());
                let signature = Signature::new_generic(
                    Some(context),
                    Parameters::standard(parameters),
                    returns,
                );
                Binding::single(callable_type, signature).into()
            }

            // TODO: some `SpecialForm`s are callable (e.g. TypedDicts)
            BindingPreparationDependency::SpecialForm => {
                CallableBinding::not_callable(callable_type).into()
            }

            BindingPreparationDependency::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => enum_literal
                    .enum_class_instance(db, env)
                    .bindings_impl(db, env, recursion_guard),
                _ => CallableBinding::not_callable(callable_type).into(),
            },

            BindingPreparationDependency::KnownInstance(KnownInstanceType::NewType(newtype)) => {
                Binding::single(
                    callable_type,
                    Signature::new(
                        Parameters::standard([Parameter::positional_only(None)
                            .with_annotated_type(newtype.base(db).instance_type(db, env))]),
                        Type::NewTypeInstance(newtype),
                    ),
                )
                .into()
            }

            BindingPreparationDependency::KnownInstance(
                KnownInstanceType::FunctoolsPartial(partial)
                | KnownInstanceType::FunctoolsPartialCall(partial),
            ) => Type::Callable(partial.partial(db)).bindings_impl(db, env, recursion_guard),

            BindingPreparationDependency::KnownInstance(KnownInstanceType::MethodWrapper(
                wrapper,
            )) => match wrapper.kind(db) {
                known_instance::MethodWrapperKind::Staticmethod => wrapper
                    .wrapped(db)
                    .bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery)
                    .with_callable_type(callable_type),
                known_instance::MethodWrapperKind::Classmethod => {
                    CallableBinding::not_callable(callable_type).into()
                }
            },

            BindingPreparationDependency::KnownInstance(known_instance) => known_instance
                .instance_fallback(db, env)
                .bindings_impl(db, env, recursion_guard),

            BindingPreparationDependency::TypeAlias(alias) => alias
                .value_type(db)
                .bindings_with_recovery(db, env, recursion_guard, unknown_is_recovery),

            BindingPreparationDependency::NotCallable => {
                CallableBinding::not_callable(callable_type).into()
            }
        }
    }
}
