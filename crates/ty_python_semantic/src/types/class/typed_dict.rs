use std::borrow::Cow;

use itertools::Either;
use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::NodeIndex;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};

use crate::place::PlaceAndQualifiers;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::ScopeId;
use crate::types::callable::CallableTypeKind;
use crate::types::generics::GenericContext;
use crate::types::member::Member;
use crate::types::mro::Mro;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::typed_dict::{
    TypedDictField, TypedDictSchema, deferred_functional_typed_dict_schema,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, ClassBase, ClassLiteral, ClassType, KnownClass,
    MemberLookupPolicy, Type, TypeContext, TypeMapping, TypeVarVariance, UnionType,
    determine_upper_bound,
};
use crate::{Db, FxIndexMap};

pub(super) fn synthesize_typed_dict_method<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    method_name: &str,
    fields: impl Fn() -> TypedDictFields<'db>,
) -> Option<Type<'db>> {
    match method_name {
        "__init__" => Some(synthesize_typed_dict_init(db, instance_ty, fields())),
        "__getitem__" => Some(synthesize_typed_dict_getitem(db, instance_ty, fields())),
        "__setitem__" => Some(synthesize_typed_dict_setitem(db, instance_ty, fields())),
        "__delitem__" => Some(synthesize_typed_dict_delitem(db, instance_ty, fields())),
        "get" => Some(synthesize_typed_dict_get(db, instance_ty, fields())),
        "update" => Some(synthesize_typed_dict_update(db, instance_ty, fields())),
        "pop" => Some(synthesize_typed_dict_pop(db, instance_ty, fields())),
        "setdefault" => Some(synthesize_typed_dict_setdefault(db, instance_ty, fields())),
        "__or__" | "__ror__" | "__ior__" => {
            Some(synthesize_typed_dict_merge(db, instance_ty, method_name))
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
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let keyword_fields: Vec<_> = fields
        .iter()
        .filter(|(name, _)| is_identifier(name))
        .collect();

    let keyword_rest_param = (keyword_fields.len() != fields.len())
        .then(|| Parameter::keyword_variadic(Name::new_static("kwargs")));

    let self_param =
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty);

    let map_param = Parameter::positional_only(Some(Name::new_static("__map")))
        .with_annotated_type(instance_ty);

    let params_with_default = keyword_fields.iter().map(|(name, field)| {
        Parameter::keyword_only((*name).clone())
            .with_annotated_type(field.declared_ty)
            .with_default_type(field.declared_ty)
    });

    let map_overload = Signature::new(
        Parameters::new(
            db,
            [self_param.clone(), map_param]
                .into_iter()
                .chain(params_with_default)
                .chain(keyword_rest_param.clone()),
        ),
        Type::none(db),
    );

    let keyword_field_params = keyword_fields.iter().map(|(name, field)| {
        let param = Parameter::keyword_only((*name).clone()).with_annotated_type(field.declared_ty);
        if field.is_required() {
            param
        } else {
            param.with_default_type(field.declared_ty)
        }
    });

    let keyword_overload = Signature::new(
        Parameters::new(
            db,
            std::iter::once(self_param)
                .chain(keyword_field_params)
                .chain(keyword_rest_param),
        ),
        Type::none(db),
    );

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads([map_overload, keyword_overload]),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `__getitem__` method for a `TypedDict`.
fn synthesize_typed_dict_getitem<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let overloads = fields.iter().map(|(field_name, field)| {
        let key_type = Type::string_literal(db, field_name);
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_type),
        ];
        Signature::new(Parameters::new(db, parameters), field.declared_ty)
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `__setitem__` method for a `TypedDict`.
fn synthesize_typed_dict_setitem<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let mut writeable_fields = fields
        .iter()
        .filter(|(_, field)| !field.is_read_only())
        .peekable();

    if writeable_fields.peek().is_none() {
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key")))
                .with_annotated_type(Type::Never),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(Type::any()),
        ];
        let signature = Signature::new(Parameters::new(db, parameters), Type::none(db));
        return Type::function_like_callable(db, signature);
    }

    let overloads = writeable_fields.map(|(field_name, field)| {
        let key_type = Type::string_literal(db, field_name);
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_type),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(field.declared_ty),
        ];
        Signature::new(Parameters::new(db, parameters), Type::none(db))
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `__delitem__` method for a `TypedDict`.
fn synthesize_typed_dict_delitem<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let mut deletable_fields = fields
        .iter()
        .filter(|(_, field)| !field.is_required())
        .peekable();

    if deletable_fields.peek().is_none() {
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key")))
                .with_annotated_type(Type::Never),
        ];
        let signature = Signature::new(Parameters::new(db, parameters), Type::none(db));
        return Type::function_like_callable(db, signature);
    }

    let overloads = deletable_fields.map(|(field_name, _)| {
        let key_type = Type::string_literal(db, field_name);
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_type),
        ];
        Signature::new(Parameters::new(db, parameters), Type::none(db))
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `get` method for a `TypedDict`.
fn synthesize_typed_dict_get<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
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
                Parameters::new(db, get_sig_params),
                if field.is_required() {
                    field.declared_ty
                } else {
                    UnionType::from_two_elements(db, field.declared_ty, Type::none(db))
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
                Parameters::new(db, get_with_default_sig_params),
                if field.is_required() {
                    field.declared_ty
                } else {
                    UnionType::from_two_elements(db, field.declared_ty, Type::TypeVar(t_default))
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
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(instance_ty),
                            Parameter::positional_only(Some(Name::new_static("key")))
                                .with_annotated_type(key_type),
                            Parameter::positional_only(Some(Name::new_static("default")))
                                .with_annotated_type(field.declared_ty),
                        ],
                    ),
                    field.declared_ty,
                );
                Either::Right(
                    [get_sig, get_with_typed_default_sig, get_with_default_sig].into_iter(),
                )
            }
        })
        // Fallback overloads for unknown keys
        .chain(std::iter::once(Signature::new(
            Parameters::new(
                db,
                [
                    Parameter::positional_only(Some(Name::new_static("self")))
                        .with_annotated_type(instance_ty),
                    Parameter::positional_only(Some(Name::new_static("key")))
                        .with_annotated_type(KnownClass::Str.to_instance(db)),
                ],
            ),
            UnionType::from_two_elements(db, Type::unknown(), Type::none(db)),
        )))
        .chain(std::iter::once({
            let t_default = BoundTypeVarInstance::synthetic(
                db,
                Name::new_static("T"),
                TypeVarVariance::Covariant,
            );

            let parameterss = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(KnownClass::Str.to_instance(db)),
                Parameter::positional_only(Some(Name::new_static("default")))
                    .with_annotated_type(Type::TypeVar(t_default)),
            ];

            Signature::new_generic(
                Some(GenericContext::from_typevar_instances(db, [t_default])),
                Parameters::new(db, parameterss),
                UnionType::from_two_elements(db, Type::unknown(), Type::TypeVar(t_default)),
            )
        }));

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `update` method for a `TypedDict`.
fn synthesize_typed_dict_update<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let keyword_parameters = fields.iter().map(|(field_name, field)| {
        let ty = if field.is_read_only() {
            Type::Never
        } else {
            field.declared_ty
        };
        Parameter::keyword_only(field_name.clone())
            .with_annotated_type(ty)
            .with_default_type(ty)
    });

    let update_patch_ty = if let Type::TypedDict(typed_dict) = instance_ty {
        Type::TypedDict(typed_dict.to_update_patch(db))
    } else {
        instance_ty
    };

    let str_object_tuple =
        Type::heterogeneous_tuple(db, [KnownClass::Str.to_instance(db), Type::object()]);

    let value_ty = UnionType::from_two_elements(
        db,
        update_patch_ty,
        KnownClass::Iterable.to_specialized_instance(db, &[str_object_tuple]),
    );

    let parameters = [
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty),
        Parameter::positional_only(Some(Name::new_static("value")))
            .with_annotated_type(value_ty)
            .with_default_type(Type::none(db)),
    ]
    .into_iter()
    .chain(keyword_parameters);

    let update_signature = Signature::new(Parameters::new(db, parameters), Type::none(db));
    Type::function_like_callable(db, update_signature)
}

/// Synthesize the `pop` method for a `TypedDict`.
fn synthesize_typed_dict_pop<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let overloads = fields
        .iter()
        .filter(|(_, field)| !field.is_required())
        .flat_map(|(field_name, field)| {
            let key_type = Type::string_literal(db, field_name);

            let pop_parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
            ];
            let pop_sig = Signature::new(Parameters::new(db, pop_parameters), field.declared_ty);

            // Non-generic overload that accepts the field type as the default,
            // providing bidirectional inference context for the default argument.
            let pop_with_typed_default_sig = Signature::new(
                Parameters::new(
                    db,
                    [
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_only(Some(Name::new_static("key")))
                            .with_annotated_type(key_type),
                        Parameter::positional_only(Some(Name::new_static("default")))
                            .with_annotated_type(field.declared_ty),
                    ],
                ),
                field.declared_ty,
            );

            let t_default = BoundTypeVarInstance::synthetic(
                db,
                Name::new_static("T"),
                TypeVarVariance::Covariant,
            );

            let pop_with_default_parameters = [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(key_type),
                Parameter::positional_only(Some(Name::new_static("default")))
                    .with_annotated_type(Type::TypeVar(t_default)),
            ];
            let pop_with_default_sig = Signature::new_generic(
                Some(GenericContext::from_typevar_instances(db, [t_default])),
                Parameters::new(db, pop_with_default_parameters),
                UnionType::from_two_elements(db, field.declared_ty, Type::TypeVar(t_default)),
            );

            [pop_sig, pop_with_typed_default_sig, pop_with_default_sig]
        });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `setdefault` method for a `TypedDict`.
fn synthesize_typed_dict_setdefault<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: TypedDictFields<'db>,
) -> Type<'db> {
    let overloads = fields.iter().map(|(field_name, field)| {
        let key_type = Type::string_literal(db, field_name);
        let parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("key"))).with_annotated_type(key_type),
            Parameter::positional_only(Some(Name::new_static("default")))
                .with_annotated_type(field.declared_ty),
        ];

        Signature::new(Parameters::new(db, parameters), field.declared_ty)
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize a merge operator (`__or__`, `__ror__`, or `__ior__`) for a `TypedDict`.
fn synthesize_typed_dict_merge<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    name: &str,
) -> Type<'db> {
    let mut overloads: smallvec::SmallVec<[Signature<'db>; 3]>;

    let first_overload_parameters = [
        Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(instance_ty),
        Parameter::positional_only(Some(Name::new_static("value")))
            .with_annotated_type(instance_ty),
    ];

    overloads = smallvec::smallvec![Signature::new(
        Parameters::new(db, first_overload_parameters,),
        instance_ty,
    )];

    if name != "__ior__" {
        let partial_ty = if let Type::TypedDict(td) = instance_ty {
            Type::TypedDict(td.to_partial(db))
        } else {
            instance_ty
        };

        let dict_param_ty = KnownClass::Dict
            .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()]);

        let dict_return_ty = KnownClass::Dict.to_specialized_instance(
            db,
            &[
                KnownClass::Str.to_instance(db),
                KnownClass::Object.to_instance(db),
            ],
        );

        let overload_two_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(partial_ty),
        ];
        overloads.push(Signature::new(
            Parameters::new(db, overload_two_parameters),
            instance_ty,
        ));

        let overload_three_parameters = [
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance_ty),
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(dict_param_ty),
        ];
        overloads.push(Signature::new(
            Parameters::new(db, overload_three_parameters),
            dict_return_ty,
        ));
    }

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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
    },
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
}

impl get_size2::GetSize for DynamicTypedDictLiteral<'_> {}

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

    /// Returns an instance type for this dynamic `TypedDict`.
    pub(crate) fn to_instance(self) -> Type<'db> {
        Type::typed_dict(ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(self)))
    }

    /// Returns the range of the `TypedDict` call expression.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let file = scope.file(db);
        let module = parsed_module(db, file).load(db);

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

    pub(crate) fn items(self, db: &'db dyn Db) -> &'db TypedDictSchema<'db> {
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => {
                deferred_functional_typed_dict_schema(db, *definition)
            }
            DynamicTypedDictAnchor::ScopeOffset { schema, .. } => schema,
        }
    }

    /// Get the MRO for this `TypedDict`.
    ///
    /// Functional `TypedDict` classes have the same MRO as class-based ones:
    /// [self, `TypedDict`, object]
    #[salsa::tracked(returns(ref), heap_size = ruff_memory_usage::heap_size)]
    pub(crate) fn mro(self, db: &'db dyn Db) -> Mro<'db> {
        let self_base = ClassBase::Class(ClassType::NonGeneric(self.into()));
        let object_class = ClassType::object(db);
        Mro::from([
            self_base,
            ClassBase::TypedDict,
            ClassBase::Class(object_class),
        ])
    }

    /// Get the metaclass of this `TypedDict`.
    ///
    /// `TypedDict`s use `type` as their metaclass.
    #[expect(clippy::unused_self)]
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        KnownClass::Type.to_class_literal(db)
    }

    /// Look up a class-level member defined directly on this `TypedDict` (not inherited).
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        synthesize_typed_dict_method(db, self.to_instance(), name, || {
            TypedDictFields::Dynamic(self.items(db))
        })
        .map(Member::definitely_declared)
        .unwrap_or_default()
    }

    /// Look up a class-level member by name (including superclasses).
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        // First check synthesized members (like __getitem__, __init__, get, etc.).
        let member = self.own_class_member(db, name);
        if !member.is_undefined() {
            return member.inner;
        }

        // Fall back to TypedDictFallback for methods like __contains__, items, keys, etc.
        // This mirrors the behavior of StaticClassLiteral::typed_dict_member.
        typed_dict_class_member(db, ClassLiteral::DynamicTypedDict(self), policy, name)
    }
}

pub(super) fn typed_dict_class_member<'db>(
    db: &'db dyn Db,
    self_class: ClassLiteral<'db>,
    lookup_policy: MemberLookupPolicy,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    KnownClass::TypedDictFallback
        .to_class_literal(db)
        .find_name_in_mro_with_policy(db, name, lookup_policy)
        .expect("Will return Some() when called on class literal")
        .map_type(|ty| {
            let new_upper_bound = determine_upper_bound(db, self_class, ClassBase::is_typed_dict);
            let mapping = TypeMapping::ReplaceSelf { new_upper_bound };
            ty.apply_type_mapping(db, &mapping, TypeContext::default())
        })
}
