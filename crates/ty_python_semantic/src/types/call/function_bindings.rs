//! Direct function bindings preserve the original special-function signatures and source overloads.

use std::convert::Infallible;

use super::super::*;
use crate::types::signatures::effects::legacy_inline;

pub(in crate::types) enum FunctionBindingSpecial {
    AssertType,
    AssertNever,
    Cast,
    Dataclass,
}

pub(in crate::types) trait FunctionBindingEffects<'db> {
    type Error;

    async fn known(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<Option<KnownFunction>, Self::Error>;
    async fn special(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable_type: Type<'db>,
        kind: FunctionBindingSpecial,
    ) -> Result<Bindings<'db>, Self::Error>;
    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error>;
    async fn bindings_from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;
}

pub(in crate::types) async fn function_bindings_with<'db, E: FunctionBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    function: FunctionType<'db>,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    let callable_type = Type::FunctionLiteral(function);
    let special = match effects.known(db, function).await? {
        Some(KnownFunction::AssertType) => Some(FunctionBindingSpecial::AssertType),
        Some(KnownFunction::AssertNever) => Some(FunctionBindingSpecial::AssertNever),
        Some(KnownFunction::Cast) => Some(FunctionBindingSpecial::Cast),
        Some(KnownFunction::Dataclass) => Some(FunctionBindingSpecial::Dataclass),
        _ => None,
    };
    if let Some(special) = special {
        return effects.special(db, env, callable_type, special).await;
    }
    let signature = effects.signature(db, function).await?;
    effects
        .bindings_from_signature(callable_type, signature)
        .await
}

struct OrdinaryFunctionBindingEffects;

pub(in crate::types) fn function_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    function: FunctionType<'db>,
) -> Bindings<'db> {
    legacy_inline(function_bindings_with(
        db,
        env,
        function,
        &OrdinaryFunctionBindingEffects,
    ))
}

impl<'db> FunctionBindingEffects<'db> for OrdinaryFunctionBindingEffects {
    type Error = Infallible;

    async fn known(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<Option<KnownFunction>, Self::Error> {
        Ok(function.known(db))
    }

    async fn special(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable_type: Type<'db>,
        kind: FunctionBindingSpecial,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(special_bindings(db, env, callable_type, kind))
    }

    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(function.signature(db))
    }

    async fn bindings_from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(CallableBinding::from_signature(callable_type, signature).into())
    }
}

fn special_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    callable_type: Type<'db>,
    kind: FunctionBindingSpecial,
) -> Bindings<'db> {
    match kind {
        FunctionBindingSpecial::AssertType => {
            let val_ty = BoundTypeVarInstance::synthetic(
                db,
                env,
                Name::new_static("T"),
                TypeVarVariance::Invariant,
            );

            Binding::single(
                callable_type,
                Signature::new_generic(
                    Some(GenericContext::from_typevar_instances(db, env, [val_ty])),
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(Type::TypeVar(val_ty)),
                        Parameter::positional_only(Some(Name::new_static("type")))
                            .with_annotated_type(object_type_form(db)),
                    ]),
                    Type::TypeVar(val_ty),
                ),
            )
            .into()
        }

        FunctionBindingSpecial::AssertNever => {
            Binding::single(
                callable_type,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "arg",
                    )))
                    // We need to set the type to `Any` here (instead of `Never`),
                    // in order for every `assert_never` call to pass the argument
                    // check. If we set it to `Never`, we'll get invalid-argument-type
                    // errors instead of `type-assertion-failure` errors.
                    .with_annotated_type(Type::any())]),
                    Type::Never,
                ),
            )
            .into()
        }

        FunctionBindingSpecial::Cast => Binding::single(
            callable_type,
            Signature::new(
                Parameters::standard([
                    Parameter::positional_or_keyword(Name::new_static("typ"))
                        .with_annotated_type(object_type_form(db)),
                    Parameter::positional_or_keyword(Name::new_static("val"))
                        .with_annotated_type(Type::any()),
                ]),
                Type::any(),
            ),
        )
        .into(),

        FunctionBindingSpecial::Dataclass => {
            let python_version = env.python_version(db);
            let bool_parameter = |name: &'static str, default: bool| {
                Parameter::keyword_only(Name::new_static(name))
                    .with_annotated_type(KnownClass::Bool.to_instance(db, env))
                    .with_default_type(Type::bool_literal(default))
            };

            let mut decorator_factory_parameters = vec![
                bool_parameter("init", true),
                bool_parameter("repr", true),
                bool_parameter("eq", true),
                bool_parameter("order", false),
                bool_parameter("unsafe_hash", false),
                bool_parameter("frozen", false),
            ];

            if python_version >= ast::PythonVersion::PY310 {
                decorator_factory_parameters.extend([
                    bool_parameter("match_args", true),
                    bool_parameter("kw_only", false),
                    bool_parameter("slots", false),
                ]);
            }

            if python_version >= ast::PythonVersion::PY311 {
                decorator_factory_parameters.push(bool_parameter("weakref_slot", false));
            }

            let parameters_with_cls = |cls_ty| {
                let mut parameters = Vec::with_capacity(decorator_factory_parameters.len() + 1);
                parameters.push(
                    Parameter::positional_only(Some(Name::new_static("cls")))
                        .with_annotated_type(cls_ty),
                );
                parameters.extend_from_slice(&decorator_factory_parameters);
                parameters
            };

            CallableBinding::from_overloads(
                callable_type,
                [
                    // def dataclass(cls: None, /, *, ...) -> Callable[[type[_T]], type[_T]]: ...
                    Signature::new(
                        Parameters::standard(parameters_with_cls(Type::none(db, env))),
                        Type::unknown(),
                    ),
                    // def dataclass(cls: type[_T], /, *, ...) -> type[_T]: ...
                    Signature::new(
                        Parameters::standard(parameters_with_cls(
                            KnownClass::Type.to_instance(db, env),
                        )),
                        Type::unknown(),
                    ),
                    // def dataclass(
                    //     *,
                    //     init: bool = True,
                    //     repr: bool = True,
                    //     eq: bool = True,
                    //     order: bool = False,
                    //     unsafe_hash: bool = False,
                    //     frozen: bool = False,
                    //     match_args: bool = True,
                    //     kw_only: bool = False,
                    //     slots: bool = False,
                    //     weakref_slot: bool = False,
                    // ) -> Callable[[type[_T]], type[_T]]: ...
                    Signature::new(
                        Parameters::standard(decorator_factory_parameters),
                        Type::unknown(),
                    ),
                ],
            )
            .into()
        }
    }
}
