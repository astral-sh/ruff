use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::NodeIndex;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};

use crate::Db;
use crate::place::PlaceAndQualifiers;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::ScopeId;
use crate::types::callable::CallableTypeKind;
use crate::types::generics::GenericContext;
use crate::types::member::Member;
use crate::types::mro::Mro;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::typed_dict::{
    FunctionalTypedDictSpec, TypedDictSchema, deferred_functional_typed_dict_spec,
    dynamic_typed_dict_schema,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, ClassBase, ClassType, KnownClass, MemberLookupPolicy, Type,
    TypeVarVariance, UnionBuilder, UnionType,
};

pub(super) fn synthesize_typed_dict_update_member<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    keyword_parameters: &[Parameter<'db>],
) -> Type<'db> {
    let update_patch_ty = if let Type::TypedDict(typed_dict) = instance_ty {
        Type::TypedDict(typed_dict.to_update_patch(db))
    } else {
        instance_ty
    };

    let value_ty = UnionBuilder::new(db)
        .add(update_patch_ty)
        .add(KnownClass::Iterable.to_specialized_instance(
            db,
            &[Type::heterogeneous_tuple(
                db,
                [KnownClass::Str.to_instance(db), Type::object()],
            )],
        ))
        .build();

    let update_signature = Signature::new(
        Parameters::new(
            db,
            [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("value")))
                    .with_annotated_type(value_ty)
                    .with_default_type(Type::none(db)),
            ]
            .into_iter()
            .chain(keyword_parameters.iter().cloned()),
        ),
        Type::none(db),
    );

    Type::function_like_callable(db, update_signature)
}

/// Represents a `TypedDict` created via the functional form:
/// ```python
/// Movie = TypedDict("Movie", {"name": str, "year": int})
/// Movie = TypedDict("Movie", {"name": str, "year": int}, total=False)
/// ```
///
/// The type of `Movie` would be `type[Movie]` where `Movie` is a `DynamicTypedDictLiteral`.
///
/// The field schema is represented by a separate [`FunctionalTypedDictSpec`].
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
        spec: FunctionalTypedDictSpec<'db>,
    },
}

#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct DynamicTypedDictLiteral<'db> {
    /// The name of the TypedDict (from the first argument).
    #[returns(ref)]
    pub name: Name,

    /// The anchor for this dynamic TypedDict, providing stable identity.
    ///
    /// - `Definition`: The call is assigned to a variable. The definition
    ///   uniquely identifies this TypedDict and can be used to find the call.
    /// - `ScopeOffset`: The call is "dangling" (not assigned). The offset
    ///   is relative to the enclosing scope's anchor node index, and the
    ///   eagerly computed spec is stored on the anchor.
    pub anchor: DynamicTypedDictAnchor<'db>,
}

impl get_size2::GetSize for DynamicTypedDictLiteral<'_> {}

#[salsa::tracked]
impl<'db> DynamicTypedDictLiteral<'db> {
    /// Returns the definition where this `TypedDict` is created, if it was assigned to a variable.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => Some(definition),
            DynamicTypedDictAnchor::ScopeOffset { .. } => None,
        }
    }

    /// Returns the scope in which this dynamic `TypedDict` was created.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => definition.scope(db),
            DynamicTypedDictAnchor::ScopeOffset { scope, .. } => scope,
        }
    }

    /// Returns an instance type for this dynamic `TypedDict`.
    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self.into()))
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

    fn spec(self, db: &'db dyn Db) -> FunctionalTypedDictSpec<'db> {
        #[salsa::tracked(
            cycle_initial = deferred_spec_initial,
            heap_size = ruff_memory_usage::heap_size
        )]
        fn deferred_spec<'db>(
            db: &'db dyn Db,
            definition: Definition<'db>,
        ) -> FunctionalTypedDictSpec<'db> {
            deferred_functional_typed_dict_spec(db, definition)
        }

        fn deferred_spec_initial<'db>(
            db: &'db dyn Db,
            _id: salsa::Id,
            _definition: Definition<'db>,
        ) -> FunctionalTypedDictSpec<'db> {
            FunctionalTypedDictSpec::unknown(db)
        }

        match self.anchor(db) {
            DynamicTypedDictAnchor::Definition(definition) => deferred_spec(db, definition),
            DynamicTypedDictAnchor::ScopeOffset { spec, .. } => spec,
        }
    }

    pub(crate) fn items(self, db: &'db dyn Db) -> &'db TypedDictSchema<'db> {
        self.spec(db).items(db)
    }

    pub(crate) fn has_known_fields(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_fields(db)
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

    /// Look up an instance member defined directly on this `TypedDict` (not inherited).
    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        if !self.has_known_fields(db) {
            // When fields are unknown, return Any for any field lookup.
            return Member::definitely_declared(Type::any());
        }

        // Look up the field by name using the computed schema.
        let schema = dynamic_typed_dict_schema(db, self);
        if let Some(field) = schema.get(name) {
            // For TypedDict, field access via attribute is not the primary way
            // to interact with them (dict indexing is), but we still allow it.
            return Member::definitely_declared(field.declared_ty);
        }

        Member::default()
    }

    /// Look up a class-level member defined directly on this `TypedDict` (not inherited).
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let instance_ty = self.to_instance(db);

        // When fields are unknown, handle constructors specially.
        if !self.has_known_fields(db) && matches!(name, "__init__" | "update") {
            let signature = if name == "__init__" {
                Signature::new(Parameters::gradual_form(), Type::none(db))
            } else {
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(instance_ty),
                            Parameter::variadic(Name::new_static("args")),
                            Parameter::keyword_variadic(Name::new_static("kwargs")),
                        ],
                    ),
                    Type::none(db),
                )
            };
            return Member::definitely_declared(Type::function_like_callable(db, signature));
        }

        // Get the computed schema for field lookups.
        let schema = dynamic_typed_dict_schema(db, self);

        match name {
            "__init__" => {
                // TypedDict constructors accept two forms:
                // 1. __init__(self, mapping: dict[str, object], /) -> None
                // 2. __init__(self, *, field1: T1, field2: T2, ...) -> None

                // Overload 1: Accept a dict literal as positional argument
                let dict_type = KnownClass::Dict.to_specialized_instance(
                    db,
                    &[KnownClass::Str.to_instance(db), Type::object()],
                );
                let dict_signature = Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(instance_ty),
                            Parameter::positional_only(Some(Name::new_static("__m")))
                                .with_annotated_type(dict_type),
                        ],
                    ),
                    Type::none(db),
                );

                // Overload 2: Accept keyword arguments for each field
                let mut kw_parameters = vec![
                    Parameter::positional_or_keyword(Name::new_static("self"))
                        .with_annotated_type(instance_ty),
                ];

                for (field_name, field) in schema {
                    let mut param = Parameter::keyword_only(field_name.clone())
                        .with_annotated_type(field.declared_ty);
                    if !field.is_required() {
                        // Optional fields have a default (conceptually the key being absent).
                        param = param.with_default_type(field.declared_ty);
                    }
                    kw_parameters.push(param);
                }

                let kw_signature =
                    Signature::new(Parameters::new(db, kw_parameters), Type::none(db));

                Member::definitely_declared(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads([dict_signature, kw_signature]),
                    CallableTypeKind::FunctionLike,
                )))
            }
            "__required_keys__" => {
                // frozenset of required key names
                let required_keys: Box<[Type<'db>]> = schema
                    .iter()
                    .filter(|(_, field)| field.is_required())
                    .map(|(name, _)| Type::string_literal(db, name.as_str()))
                    .collect();
                let union = UnionType::from_elements(db, required_keys.iter().copied());
                Member::definitely_declared(
                    KnownClass::FrozenSet.to_specialized_instance(db, &[union]),
                )
            }
            "__optional_keys__" => {
                // frozenset of optional key names
                let optional_keys: Box<[Type<'db>]> = schema
                    .iter()
                    .filter(|(_, field)| !field.is_required())
                    .map(|(name, _)| Type::string_literal(db, name.as_str()))
                    .collect();
                let union = UnionType::from_elements(db, optional_keys.iter().copied());
                Member::definitely_declared(
                    KnownClass::FrozenSet.to_specialized_instance(db, &[union]),
                )
            }
            "__getitem__" => {
                // __getitem__(self, key: Literal["name"]) -> type for each field
                let overloads = schema.iter().map(|(field_name, field)| {
                    let key_type = Type::string_literal(db, field_name.as_str());
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(instance_ty),
                                Parameter::positional_only(Some(Name::new_static("key")))
                                    .with_annotated_type(key_type),
                            ],
                        ),
                        field.declared_ty,
                    )
                });
                Member::definitely_declared(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            "__setitem__" => {
                // __setitem__(self, key: Literal["name"], value: type) -> None for each non-readonly field
                let overloads: Vec<_> = schema
                    .iter()
                    .map(|(field_name, field)| {
                        let key_type = Type::string_literal(db, field_name.as_str());
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                    Parameter::positional_only(Some(Name::new_static("value")))
                                        .with_annotated_type(field.declared_ty),
                                ],
                            ),
                            Type::none(db),
                        )
                    })
                    .collect();
                if overloads.is_empty() {
                    // No fields, return a callable that takes no keys
                    Member::definitely_declared(Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(Type::Never),
                                    Parameter::positional_only(Some(Name::new_static("value")))
                                        .with_annotated_type(Type::Never),
                                ],
                            ),
                            Type::none(db),
                        )),
                        CallableTypeKind::FunctionLike,
                    )))
                } else {
                    Member::definitely_declared(Type::Callable(CallableType::new(
                        db,
                        CallableSignature::from_overloads(overloads),
                        CallableTypeKind::FunctionLike,
                    )))
                }
            }
            "__delitem__" => {
                // __delitem__(self, key: Literal["name"]) -> None for each non-required field
                let deletable: Vec<_> = schema
                    .iter()
                    .filter(|(_, field)| !field.is_required())
                    .map(|(field_name, _)| {
                        let key_type = Type::string_literal(db, field_name.as_str());
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                ],
                            ),
                            Type::none(db),
                        )
                    })
                    .collect();
                if deletable.is_empty() {
                    // No deletable fields, return a callable with Never key type
                    Member::definitely_declared(Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(Type::Never),
                                ],
                            ),
                            Type::none(db),
                        )),
                        CallableTypeKind::FunctionLike,
                    )))
                } else {
                    Member::definitely_declared(Type::Callable(CallableType::new(
                        db,
                        CallableSignature::from_overloads(deletable),
                        CallableTypeKind::FunctionLike,
                    )))
                }
            }
            "get" => {
                // get(key: Literal["name"]) -> type | None for each field
                // get(key: Literal["name"], default: T) -> type | T for each field
                let overloads = schema
                    .iter()
                    .flat_map(|(field_name, field)| {
                        let key_type = Type::string_literal(db, field_name.as_str());

                        // For a required key, `.get()` always returns the value type.
                        // For a non-required key, `.get()` returns union with None/default.
                        let get_sig = Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                ],
                            ),
                            if field.is_required() {
                                field.declared_ty
                            } else {
                                UnionType::from_elements(db, [field.declared_ty, Type::none(db)])
                            },
                        );

                        let t_default = BoundTypeVarInstance::synthetic(
                            db,
                            Name::new_static("T"),
                            TypeVarVariance::Covariant,
                        );

                        let get_with_default_sig = Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [t_default])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                    Parameter::positional_only(Some(Name::new_static("default")))
                                        .with_annotated_type(Type::TypeVar(t_default)),
                                ],
                            ),
                            if field.is_required() {
                                field.declared_ty
                            } else {
                                UnionType::from_elements(
                                    db,
                                    [field.declared_ty, Type::TypeVar(t_default)],
                                )
                            },
                        );

                        [get_sig, get_with_default_sig]
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
                        UnionType::from_elements(db, [Type::unknown(), Type::none(db)]),
                    )))
                    .chain(std::iter::once({
                        let t_default = BoundTypeVarInstance::synthetic(
                            db,
                            Name::new_static("T"),
                            TypeVarVariance::Covariant,
                        );

                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [t_default])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(KnownClass::Str.to_instance(db)),
                                    Parameter::positional_only(Some(Name::new_static("default")))
                                        .with_annotated_type(Type::TypeVar(t_default)),
                                ],
                            ),
                            UnionType::from_elements(
                                db,
                                [Type::unknown(), Type::TypeVar(t_default)],
                            ),
                        )
                    }));

                Member::definitely_declared(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            "update" => {
                let keyword_parameters: Vec<_> = schema
                    .iter()
                    .map(|(field_name, field)| {
                        Parameter::keyword_only(field_name.clone())
                            .with_annotated_type(field.declared_ty)
                            .with_default_type(field.declared_ty)
                    })
                    .collect();

                Member::definitely_declared(synthesize_typed_dict_update_member(
                    db,
                    instance_ty,
                    &keyword_parameters,
                ))
            }
            _ => Member::default(),
        }
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
        KnownClass::TypedDictFallback
            .to_class_literal(db)
            .find_name_in_mro_with_policy(db, name, policy)
            .expect(
                "`find_name_in_mro_with_policy` will return `Some()` when called on class literal",
            )
    }

    /// Look up an instance member by name (including superclasses).
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // First check own instance members (TypedDict fields).
        let result = self.own_instance_member(db, name);
        if !result.is_undefined() {
            return result.inner;
        }

        // Fall back to the dict instance members.
        let dict_instance = KnownClass::Dict.to_instance(db);
        dict_instance.instance_member(db, name)
    }
}
