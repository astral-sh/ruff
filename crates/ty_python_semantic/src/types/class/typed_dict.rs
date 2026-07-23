use crate::SemanticEnvironment;
use std::borrow::Cow;

use itertools::Either;
use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::NodeIndex;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::KnownModule;

use crate::place::PlaceAndQualifiers;
use crate::place::known_module_symbol;
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::generics::GenericContext;
use crate::types::member::Member;
use crate::types::mro::Mro;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::typed_dict::{
    TypedDictField, TypedDictOpenness, TypedDictSchema, deferred_functional_typed_dict_openness,
    deferred_functional_typed_dict_schema,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, ClassBase, ClassLiteral, ClassType, KnownClass,
    MemberLookupPolicy, Type, TypeContext, TypeMapping, TypeVarVariance, TypedDictModule,
    TypedDictType, UnionType, determine_upper_bound,
};
use crate::{Db, FxIndexMap};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

pub(super) fn synthesize_typed_dict_method<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    method_name: &str,
    fields: impl Fn() -> TypedDictFields<'db>,
) -> Option<Type<'db>> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    match method_name {
        "__init__" => Some(synthesize_typed_dict_init(env, typed_dict, fields())),
        "__getitem__" => Some(synthesize_typed_dict_getitem(env, typed_dict, fields())),
        "__setitem__" => Some(synthesize_typed_dict_setitem(env, typed_dict, fields())),
        "__delitem__" => Some(synthesize_typed_dict_delitem(env, typed_dict, fields())),
        "get" => Some(synthesize_typed_dict_get(env, typed_dict, fields())),
        "update" => Some(synthesize_typed_dict_update(env, typed_dict, fields())),
        "pop" => Some(synthesize_typed_dict_pop(env, typed_dict, fields())),
        "setdefault" => Some(synthesize_typed_dict_setdefault(env, typed_dict, fields())),
        "clear" if typed_dict.supports_arbitrary_key_deletion(env) => Some(
            synthesize_typed_dict_no_argument_method(db, typed_dict, Type::none(env)),
        ),
        "popitem" if typed_dict.supports_arbitrary_key_deletion(env) => {
            let return_ty = Type::heterogeneous_tuple(
                db,
                [KnownClass::Str.to_instance(env), typed_dict.value_type(env)],
            );
            Some(synthesize_typed_dict_no_argument_method(
                db, typed_dict, return_ty,
            ))
        }
        "__iter__" if typed_dict.openness(env).is_closed() => {
            let return_ty =
                KnownClass::Iterator.to_specialized_instance(env, &[typed_dict.key_type(env)]);
            Some(synthesize_typed_dict_no_argument_method(
                db, typed_dict, return_ty,
            ))
        }
        "items" if !typed_dict.openness(env).is_implicitly_open() => Some(
            synthesize_typed_dict_view_method(env, typed_dict, "dict_items"),
        ),
        "keys" if !typed_dict.openness(env).is_implicitly_open() => Some(
            synthesize_typed_dict_view_method(env, typed_dict, "dict_keys"),
        ),
        "values" if !typed_dict.openness(env).is_implicitly_open() => Some(
            synthesize_typed_dict_view_method(env, typed_dict, "dict_values"),
        ),
        "__or__" | "__ror__" | "__ior__" => {
            Some(synthesize_typed_dict_merge(env, instance_ty, method_name))
        }
        _ => None,
    }
}

/// Enum unifying the field schema for both dynamic and static `TypedDict` representations.
#[derive(Debug, Copy, Clone)]
pub(super) enum TypedDictFields<'db> {
    Dynamic(&'db TypedDictSchema<'db>),
    Static(&'db FxIndexMap<Name, super::Field<'db>>),
}

impl<'db> TypedDictFields<'db> {
    fn len(self) -> usize {
        match self {
            TypedDictFields::Dynamic(schema) => schema.len(),
            TypedDictFields::Static(fields) => fields.len(),
        }
    }

    fn iter(self) -> impl Iterator<Item = (&'db Name, Cow<'db, TypedDictField<'db>>)> {
        match self {
            TypedDictFields::Dynamic(schema) => Either::Left(
                schema
                    .iter()
                    .map(|(name, field)| (name, Cow::Borrowed(field))),
            ),
            TypedDictFields::Static(fields) => Either::Right(
                fields
                    .iter()
                    .map(|(name, field)| (name, Cow::Owned(TypedDictField::from_field(field)))),
            ),
        }
    }
}

/// Synthesize the `__init__` method for a `TypedDict`.
///
/// overloads:
/// 1. `__init__(self, __map: TD, /, *, field1: T1 = ..., field2: T2 = ...) -> None`
///    Allows passing another instance of the `TypedDict` when creating a new instance.
///    Technically, `__map` could accept a subset of the `TypedDict` if the remaining
///    fields are provided as keyword arguments, but we don't model that in the
///    synthesized `__init__`, since this signature is primarily used for IDE support.
///    Fields that are not valid Python identifiers are collapsed into `**kwargs`.
/// 2. `__init__(self, *, field1: T1, field2: T2 = ...) -> None`
///    Keyword-only. Fields that are not valid Python identifiers are collapsed into `**kwargs`.
fn synthesize_typed_dict_init<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let keyword_fields: Vec<_> = fields
        .iter()
        .filter(|(name, _)| is_identifier(name))
        .collect();

    let keyword_rest_param = typed_dict
        .explicit_extra_items(env)
        .map(|extra_items| {
            Parameter::keyword_variadic(Name::new_static("kwargs"))
                .with_annotated_type(extra_items.declared_ty)
        })
        .or_else(|| {
            (keyword_fields.len() != fields.len())
                .then(|| Parameter::keyword_variadic(Name::new_static("kwargs")))
        });

    let self_param =
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty);

    let map_param =
        Parameter::positional_only(Some(Name::new_static("map"))).with_annotated_type(instance_ty);

    let params_with_default = keyword_fields.iter().map(|(name, field)| {
        Parameter::keyword_only((*name).clone())
            .with_annotated_type(field.declared_ty)
            .with_default_type(field.declared_ty)
            .with_definition(field.first_declaration())
    });

    let map_overload = Signature::new(
        Parameters::standard(
            [self_param.clone(), map_param]
                .into_iter()
                .chain(params_with_default)
                .chain(keyword_rest_param.clone()),
        ),
        Type::none(env),
    );

    let keyword_field_params = keyword_fields.iter().map(|(name, field)| {
        let param = Parameter::keyword_only((*name).clone())
            .with_annotated_type(field.declared_ty)
            .with_definition(field.first_declaration());
        if field.is_required() {
            param
        } else {
            param.with_default_type(field.declared_ty)
        }
    });

    let keyword_overload = Signature::new(
        Parameters::standard(
            std::iter::once(self_param)
                .chain(keyword_field_params)
                .chain(keyword_rest_param),
        ),
        Type::none(env),
    );

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads([map_overload, keyword_overload]),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `__getitem__` method for a `TypedDict`.
fn synthesize_typed_dict_getitem<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let overloads = fields
        .iter()
        .map(|(field_name, field)| {
            let key_type = Type::string_literal(db, field_name);
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
            ];
            Signature::new(Parameters::standard(parameters), field.declared_ty)
        })
        .chain(std::iter::once(Signature::new(
            Parameters::standard([
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(env)),
            ]),
            if typed_dict.explicit_extra_items(env).is_some() {
                typed_dict.value_type(env)
            } else {
                Type::object()
            },
        )));

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `__setitem__` method for a `TypedDict`.
fn synthesize_typed_dict_setitem<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let mut writable_fields = fields
        .iter()
        .filter(|(_, field)| !field.is_read_only())
        .peekable();
    let arbitrary_key_mutation_type = typed_dict.arbitrary_key_mutation_type(env);

    if writable_fields.peek().is_none() && arbitrary_key_mutation_type.is_none() {
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key")))
                .with_annotated_type(Type::Never),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(Type::any()),
        ];
        let signature = Signature::new(Parameters::standard(parameters), Type::none(env));
        return Type::function_like_callable(db, signature);
    }

    let overloads = writable_fields
        .map(|(field_name, field)| {
            let key_type = Type::string_literal(db, field_name);
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
                Parameter::positional_only(Some(Name::new_static("value")))
                    .with_annotated_type(field.declared_ty),
            ];
            Signature::new(Parameters::standard(parameters), Type::none(env))
        })
        .chain(arbitrary_key_mutation_type.map(|value_ty| {
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(env)),
                Parameter::positional_only(Some(Name::new_static("value")))
                    .with_annotated_type(value_ty),
            ];
            Signature::new(Parameters::standard(parameters), Type::none(env))
        }));

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `__delitem__` method for a `TypedDict`.
fn synthesize_typed_dict_delitem<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let mut deletable_fields = fields
        .iter()
        .filter(|(_, field)| !field.is_required() && !field.is_read_only())
        .peekable();
    let supports_arbitrary_key_deletion = typed_dict.supports_arbitrary_key_deletion(env);

    if deletable_fields.peek().is_none() && !supports_arbitrary_key_deletion {
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key")))
                .with_annotated_type(Type::Never),
        ];
        let signature = Signature::new(Parameters::standard(parameters), Type::none(env));
        return Type::function_like_callable(db, signature);
    }

    let overloads = deletable_fields
        .map(|(field_name, _)| {
            let key_type = Type::string_literal(db, field_name);
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
            ];
            Signature::new(Parameters::standard(parameters), Type::none(env))
        })
        .chain(supports_arbitrary_key_deletion.then(|| {
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(env)),
            ];
            Signature::new(Parameters::standard(parameters), Type::none(env))
        }));

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `get` method for a `TypedDict`.
fn synthesize_typed_dict_get<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let fallback_value_ty = if typed_dict.openness(env).is_implicitly_open() {
        Type::unknown()
    } else {
        typed_dict.value_type(env)
    };
    let overloads = fields
        .iter()
        .flat_map(|(field_name, field)| {
            let key_type = Type::string_literal(db, field_name);

            let get_sig_params = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
            ];
            let get_sig = Signature::new(
                Parameters::standard(get_sig_params),
                if field.is_required() {
                    field.declared_ty
                } else {
                    UnionType::from_two_elements(env, field.declared_ty, Type::none(env))
                },
            );

            let t_default = BoundTypeVarInstance::synthetic(
                db,
                Name::new_static("T"),
                TypeVarVariance::Covariant,
            );

            let get_with_default_sig_params = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
                Parameter::positional_only(Some(Name::new_static("default")))
                    .with_annotated_type(Type::TypeVar(t_default)),
            ];
            let get_with_default_sig = Signature::new_generic(
                Some(GenericContext::from_typevar_instances(db, [t_default])),
                Parameters::standard(get_with_default_sig_params),
                if field.is_required() {
                    field.declared_ty
                } else {
                    UnionType::from_two_elements(env, field.declared_ty, Type::TypeVar(t_default))
                },
            );

            // For non-required fields, add a non-generic overload that accepts the
            // field type as the default. This is ordered before the generic TypeVar
            // overload so that `td.get("key", {})` can use the field type as
            // bidirectional inference context for the default argument.
            if field.is_required() {
                Either::Left([get_sig, get_with_default_sig].into_iter())
            } else {
                let get_with_typed_default_sig = Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_only(Some(Name::new_static("key")))
                            .with_annotated_type(key_type),
                        Parameter::positional_only(Some(Name::new_static("default")))
                            .with_annotated_type(field.declared_ty),
                    ]),
                    field.declared_ty,
                );
                Either::Right(
                    [get_sig, get_with_typed_default_sig, get_with_default_sig].into_iter(),
                )
            }
        })
        // Fallback overloads for unknown keys
        .chain(std::iter::once(Signature::new(
            Parameters::standard([
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(env)),
            ]),
            UnionType::from_two_elements(env, fallback_value_ty, Type::none(env)),
        )))
        .chain(std::iter::once({
            let t_default = BoundTypeVarInstance::synthetic(
                db,
                Name::new_static("T"),
                TypeVarVariance::Covariant,
            );

            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(env)),
                Parameter::positional_only(Some(Name::new_static("default")))
                    .with_annotated_type(Type::TypeVar(t_default)),
            ];

            Signature::new_generic(
                Some(GenericContext::from_typevar_instances(db, [t_default])),
                Parameters::standard(parameters),
                UnionType::from_two_elements(env, fallback_value_ty, Type::TypeVar(t_default)),
            )
        }));

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `update` method for a `TypedDict`.
fn synthesize_typed_dict_update<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let keyword_parameters = fields
        .iter()
        .map(|(field_name, field)| {
            let ty = if field.is_read_only() {
                Type::Never
            } else {
                field.declared_ty
            };
            Parameter::keyword_only(field_name.clone())
                .with_annotated_type(ty)
                .with_default_type(ty)
                .with_definition(field.first_declaration())
        })
        .chain(
            typed_dict
                .explicit_extra_items(env)
                .filter(|extra_items| !extra_items.is_read_only())
                .map(|extra_items| {
                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                        .with_annotated_type(extra_items.declared_ty)
                }),
        );

    let update_patch_ty = Type::TypedDict(typed_dict.to_update_patch(env));

    let mapping_ty = typed_dict.dict_value_type(env).map(|value_ty| {
        KnownClass::Mapping
            .to_specialized_instance(env, &[KnownClass::Str.to_instance(env), value_ty])
    });
    let iterable_ty = typed_dict.arbitrary_key_mutation_type(env).map(|value_ty| {
        let item_ty = Type::heterogeneous_tuple(db, [KnownClass::Str.to_instance(env), value_ty]);
        KnownClass::Iterable.to_specialized_instance(env, &[item_ty])
    });
    let value_ty = UnionType::from_elements(
        env,
        std::iter::once(update_patch_ty)
            .chain(mapping_ty)
            .chain(iterable_ty),
    );

    let parameters = [
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty),
        Parameter::positional_only(Some(Name::new_static("value")))
            .with_annotated_type(value_ty)
            .with_default_type(Type::none(env)),
    ]
    .into_iter()
    .chain(keyword_parameters);

    let update_signature = Signature::new(Parameters::standard(parameters), Type::none(env));
    Type::function_like_callable(db, update_signature)
}

/// Synthesize the `pop` method for a `TypedDict`.
fn synthesize_typed_dict_pop<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let pop_overloads = |key_ty, value_ty| {
        let pop_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_ty),
        ];
        let pop_sig = Signature::new(Parameters::standard(pop_parameters), value_ty);

        // Non-generic overload that accepts the value type as the default,
        // providing bidirectional inference context for the default argument.
        let pop_with_typed_default_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_ty),
            Parameter::positional_only(Some(Name::new_static("default")))
                .with_annotated_type(value_ty),
        ];
        let pop_with_typed_default_sig = Signature::new(
            Parameters::standard(pop_with_typed_default_parameters),
            value_ty,
        );

        let t_default =
            BoundTypeVarInstance::synthetic(db, Name::new_static("T"), TypeVarVariance::Covariant);
        let pop_with_default_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_ty),
            Parameter::positional_only(Some(Name::new_static("default")))
                .with_annotated_type(Type::TypeVar(t_default)),
        ];
        let pop_with_default_sig = Signature::new_generic(
            Some(GenericContext::from_typevar_instances(db, [t_default])),
            Parameters::standard(pop_with_default_parameters),
            UnionType::from_two_elements(env, value_ty, Type::TypeVar(t_default)),
        );

        [pop_sig, pop_with_typed_default_sig, pop_with_default_sig]
    };

    let overloads = fields
        .iter()
        .filter(|(_, field)| !field.is_required() && !field.is_read_only())
        .flat_map(|(field_name, field)| {
            pop_overloads(Type::string_literal(db, field_name), field.declared_ty)
        })
        .chain(
            typed_dict
                .supports_arbitrary_key_deletion(env)
                .then(|| {
                    pop_overloads(KnownClass::Str.to_instance(env), typed_dict.value_type(env))
                })
                .into_iter()
                .flatten(),
        );

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Synthesize the `setdefault` method for a `TypedDict`.
fn synthesize_typed_dict_setdefault<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let db = env.db();
    let instance_ty = Type::TypedDict(typed_dict);
    let overloads = fields
        .iter()
        .filter(|(_, field)| !field.is_read_only())
        .map(|(field_name, field)| {
            let key_type = Type::string_literal(db, field_name);
            let parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
                Parameter::positional_only(Some(Name::new_static("default")))
                    .with_annotated_type(field.declared_ty),
            ];

            Signature::new(Parameters::standard(parameters), field.declared_ty)
        })
        .chain(
            typed_dict
                .arbitrary_key_mutation_type(env)
                .map(|default_ty| {
                    let parameters = [
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_only(Some(Name::new_static("key")))
                            .with_annotated_type(KnownClass::Str.to_instance(env)),
                        Parameter::positional_only(Some(Name::new_static("default")))
                            .with_annotated_type(default_ty),
                    ];
                    Signature::new(Parameters::standard(parameters), typed_dict.value_type(env))
                }),
        );

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

fn synthesize_typed_dict_no_argument_method<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    return_ty: Type<'db>,
) -> Type<'db> {
    Type::function_like_callable(
        db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(Type::TypedDict(typed_dict))]),
            return_ty,
        ),
    )
}

/// Synthesize `items`, `keys`, or `values` for a closed or extra-items `TypedDict`.
fn synthesize_typed_dict_view_method<'db>(
    env: &SemanticEnvironment<'db>,
    typed_dict: TypedDictType<'db>,
    view_name: &str,
) -> Type<'db> {
    let db = env.db();
    let return_ty = known_module_symbol(env, KnownModule::CollectionsAbcInternal, view_name)
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .map(|class| {
            class.apply_specialization(env, |generic_context| {
                generic_context
                    .specialize(db, &[typed_dict.key_type(env), typed_dict.value_type(env)])
            })
        })
        .and_then(|class| Type::from(class).to_instance_approximation(env))
        .unwrap_or_else(Type::unknown);

    synthesize_typed_dict_no_argument_method(db, typed_dict, return_ty)
}

/// Synthesize a merge operator (`__or__`, `__ror__`, or `__ior__`) for a `TypedDict`.
fn synthesize_typed_dict_merge<'db>(
    env: &SemanticEnvironment<'db>,
    instance_ty: Type<'db>,
    name: &str,
) -> Type<'db> {
    let db = env.db();
    let mut overloads: smallvec::SmallVec<[Signature<'db>; 3]>;

    let first_overload_value_ty = if name == "__ior__"
        && let Type::TypedDict(typed_dict) = instance_ty
    {
        Type::TypedDict(typed_dict.to_update_patch(env))
    } else {
        instance_ty
    };

    let first_overload_parameters = [
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty),
        Parameter::positional_only(Some(Name::new_static("value")))
            .with_annotated_type(first_overload_value_ty),
    ];

    overloads = smallvec::smallvec![Signature::new(
        Parameters::standard(first_overload_parameters),
        instance_ty,
    )];

    if name != "__ior__" {
        let partial_ty = if let Type::TypedDict(td) = instance_ty {
            Type::TypedDict(td.to_partial(env))
        } else {
            instance_ty
        };

        let dict_param_ty = KnownClass::Dict
            .to_specialized_instance(env, &[KnownClass::Str.to_instance(env), Type::any()]);

        let dict_return_ty = KnownClass::Dict.to_specialized_instance(
            env,
            &[
                KnownClass::Str.to_instance(env),
                KnownClass::Object.to_instance(env),
            ],
        );

        let overload_two_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(partial_ty),
        ];
        overloads.push(Signature::new(
            Parameters::standard(overload_two_parameters),
            instance_ty,
        ));

        let overload_three_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(dict_param_ty),
        ];
        overloads.push(Signature::new(
            Parameters::standard(overload_three_parameters),
            dict_return_ty,
        ));
    }

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    ))
}

/// Represents a `TypedDict` created via the functional form:
/// ```python
/// Movie = TypedDict("Movie", {"name": str, "year": int})
/// Movie = TypedDict("Movie", {"name": str, "year": int}, total=False)
/// ```
///
/// The type of `Movie` would be `type[Movie]` where `Movie` is a `DynamicTypedDictLiteral`.
///
/// The field schema is represented by a separate [`TypedDictSchema`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum DynamicTypedDictAnchor<'db> {
    /// The `TypedDict()` call is assigned to a variable.
    ///
    /// The `Definition` uniquely identifies this `TypedDict`. Field types are computed lazily
    /// during deferred inference so recursive `TypedDict` definitions can resolve correctly.
    Definition(Definition<'db>),

    /// The `TypedDict()` call is "dangling" (not assigned to a variable).
    ///
    /// The offset is relative to the enclosing scope's anchor node index. The eagerly
    /// computed `spec` preserves field types for inline uses like
    /// `TypedDict("Point", {"x": int})(x=1)`.
    ScopeOffset {
        scope: ScopeId<'db>,
        offset: u32,
        schema: TypedDictSchema<'db>,
        openness: TypedDictOpenness<'db>,
    },
}

impl<'db> DynamicTypedDictAnchor<'db> {
    fn recursive_type_normalized_impl(
        &self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Definition(definition) => Some(Self::Definition(*definition)),
            Self::ScopeOffset {
                scope,
                offset,
                schema,
                openness,
            } => Some(Self::ScopeOffset {
                scope: *scope,
                offset: *offset,
                schema: schema.recursive_type_normalized_impl(env, div, nested)?,
                openness: openness.recursive_type_normalized_impl(env, div, nested)?,
            }),
        }
    }
}

#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct DynamicTypedDictLiteral<'db> {
    /// The name of the TypedDict (from the first argument).
    #[returns(ref)]
    pub(crate) name: Name,

    /// The anchor for this dynamic TypedDict, providing stable identity.
    ///
    /// - `Definition`: The call is assigned to a variable. The definition
    ///   uniquely identifies this TypedDict and can be used to find the call.
    /// - `ScopeOffset`: The call is "dangling" (not assigned). The offset
    ///   is relative to the enclosing scope's anchor node index, and the
    ///   eagerly computed spec is stored on the anchor.
    #[returns(ref)]
    pub(crate) anchor: DynamicTypedDictAnchor<'db>,

    #[returns(copy)]
    pub(crate) typed_dict_module: TypedDictModule,
}

impl get_size2::GetSize for DynamicTypedDictLiteral<'_> {}

impl<'db> DynamicTypedDictLiteral<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let db = env.db();
        Some(Self::new(
            db,
            self.name(db),
            self.anchor(db)
                .recursive_type_normalized_impl(env, div, nested)?,
            self.typed_dict_module(db),
        ))
    }
}

#[salsa::tracked]
impl<'db> DynamicTypedDictLiteral<'db> {
    /// Returns the definition where this `TypedDict` is created, if it was assigned to a variable.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => Some(*definition),
            DynamicTypedDictAnchor::ScopeOffset { .. } => None,
        }
    }

    /// Returns the scope in which this dynamic `TypedDict` was created.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => definition.scope(db),
            DynamicTypedDictAnchor::ScopeOffset { scope, .. } => *scope,
        }
    }

    /// Returns the range of the `TypedDict` call expression.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let module = parsed_module(db, scope.python_file(db)).load(db);

        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => {
                // For definitions, get the range from the definition's value.
                // The TypedDict call is the value of the assignment.
                definition
                    .kind(db)
                    .value(&module)
                    .expect(
                        "DynamicTypedDictAnchor::Definition should only be used for assignments",
                    )
                    .range()
            }
            DynamicTypedDictAnchor::ScopeOffset { offset, .. } => {
                // For dangling calls, compute the absolute index from the offset.
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("anchor should not be NodeIndex::NONE");
                let absolute_index = NodeIndex::from(anchor_u32 + offset);

                // Get the node and return its range.
                let node: &ast::ExprCall = module
                    .get_by_index(absolute_index)
                    .try_into()
                    .expect("scope offset should point to ExprCall");
                node.range()
            }
        }
    }

    /// Returns a [`Span`] pointing to the `TypedDict` call expression.
    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    pub(crate) fn items(self, env: &SemanticEnvironment<'db>) -> &'db TypedDictSchema<'db> {
        let db = env.db();
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => {
                deferred_functional_typed_dict_schema(env, *definition)
            }
            DynamicTypedDictAnchor::ScopeOffset { schema, .. } => schema,
        }
    }

    pub(crate) fn openness(self, env: &SemanticEnvironment<'db>) -> TypedDictOpenness<'db> {
        let db = env.db();
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => {
                deferred_functional_typed_dict_openness(env, *definition)
            }
            DynamicTypedDictAnchor::ScopeOffset { openness, .. } => *openness,
        }
    }

    /// Get the MRO for this `TypedDict`.
    ///
    /// Functional `TypedDict` classes have the same MRO as class-based ones:
    /// [self, `TypedDict`, object]
    pub(crate) fn mro(self, env: &SemanticEnvironment<'db>) -> &'db Mro<'db> {
        let db = env.db();
        debug_assert_eq!(env.program(), self.scope(db).program(db));
        self.mro_inner(db)
    }

    #[salsa::tracked(returns(ref), heap_size = ruff_memory_usage::heap_size)]
    fn mro_inner(self, db: &'db dyn Db) -> Mro<'db> {
        let self_base = ClassBase::Class(ClassType::NonGeneric(self.into()));
        let env = SemanticEnvironment::from_file(db, self.scope(db).python_file(db));
        let object_class = ClassType::object(&env);
        Mro::from([
            self_base,
            ClassBase::TypedDict(self.typed_dict_module(db)),
            ClassBase::Class(object_class),
        ])
    }

    /// Returns the metaclass of this `TypedDict`.
    ///
    /// `TypedDict`s use `type` as their metaclass.
    #[expect(clippy::unused_self)]
    pub(crate) fn metaclass(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        KnownClass::Type.to_class_literal(env)
    }

    /// Look up a class-level member defined directly on this `TypedDict` (not inherited).
    pub(super) fn own_class_member(
        self,
        env: &SemanticEnvironment<'db>,
        name: &str,
    ) -> Member<'db> {
        let typed_dict =
            TypedDictType::new(ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(self)));
        synthesize_typed_dict_method(env, typed_dict, name, || {
            TypedDictFields::Dynamic(self.items(env))
        })
        .map(Member::definitely_declared)
        .unwrap_or_default()
    }

    /// Look up a class-level member by name (including superclasses).
    pub(crate) fn class_member(
        self,
        env: &SemanticEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let db = env.db();
        // First check synthesized members (like __getitem__, __init__, get, etc.).
        let member = self.own_class_member(env, name);
        if !member.is_undefined() {
            return member.inner;
        }

        // Fall back to TypedDictFallback for methods like __contains__, items, keys, etc.
        // This mirrors the behavior of StaticClassLiteral::typed_dict_member.
        typed_dict_class_member(
            env,
            ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(self)),
            self.typed_dict_module(db),
            policy,
            name,
        )
    }
}

pub(super) fn typed_dict_fallback_class_member<'db>(
    env: &SemanticEnvironment<'db>,
    module: TypedDictModule,
    lookup_policy: MemberLookupPolicy,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    let fallback = match module {
        TypedDictModule::Typing => KnownClass::TypedDictFallback,
        TypedDictModule::TypingExtensions => KnownClass::ExtensionTypedDictFallback,
    };

    fallback
        .to_class_literal(env)
        .find_name_in_mro_with_policy(env, name, lookup_policy)
        .expect("Will return Some() when called on class literal")
}

pub(super) fn typed_dict_class_member<'db>(
    env: &SemanticEnvironment<'db>,
    class: ClassType<'db>,
    module: TypedDictModule,
    lookup_policy: MemberLookupPolicy,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    let db = env.db();
    let self_class = class.class_literal(db);
    let fallback_member = typed_dict_fallback_class_member(env, module, lookup_policy, name)
        .map_type(|ty| {
            let new_upper_bound = determine_upper_bound(env, self_class, ClassBase::is_typed_dict);
            let mapping = TypeMapping::ReplaceSelf { new_upper_bound };
            ty.apply_type_mapping(env, &mapping, TypeContext::default())
        });
    if !fallback_member.is_undefined() {
        return fallback_member;
    }

    if let Some(value_ty) = TypedDictType::new(class).dict_value_type(env)
        && let Some(dict_class) = KnownClass::Dict
            .to_specialized_class_type(env, &[KnownClass::Str.to_instance(env), value_ty])
    {
        let member = dict_class.class_member(env, name, lookup_policy);
        if !member.is_undefined() {
            return member;
        }
    }

    fallback_member
}
