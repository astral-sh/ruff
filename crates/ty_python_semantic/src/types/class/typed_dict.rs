use std::borrow::Borrow;

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
    TypedDictField, TypedDictSchema, deferred_functional_typed_dict_schema,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, ClassBase, ClassType, KnownClass, MemberLookupPolicy, Type,
    TypeVarVariance, UnionBuilder, UnionType,
};

/// Synthesize the `__getitem__` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_getitem<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let overloads = fields.into_iter().map(|(field_name, field)| {
        let field_name = field_name.borrow();
        let field = field.borrow();
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

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `__setitem__` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_setitem<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let mut writeable_fields = fields
        .into_iter()
        .filter(|(_, field)| !(*field).borrow().is_read_only())
        .peekable();

    if writeable_fields.peek().is_none() {
        return Type::Callable(CallableType::new(
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
                            .with_annotated_type(Type::any()),
                    ],
                ),
                Type::none(db),
            )),
            CallableTypeKind::FunctionLike,
        ));
    }

    let overloads = writeable_fields.map(|(field_name, field)| {
        let field_name = field_name.borrow();
        let field = field.borrow();
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
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `__delitem__` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_delitem<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let mut deletable_fields = fields
        .into_iter()
        .filter(|(_, field)| !(*field).borrow().is_required())
        .peekable();

    if deletable_fields.peek().is_none() {
        return Type::Callable(CallableType::new(
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
        ));
    }

    let overloads = deletable_fields.map(|(field_name, _)| {
        let field_name = field_name.borrow();
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
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `get` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_get<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let overloads = fields
        .into_iter()
        .flat_map(|(field_name, field)| {
            let field_name = field_name.borrow();
            let field = field.borrow();
            let key_type = Type::string_literal(db, field_name.as_str());

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
                    UnionType::from_two_elements(db, field.declared_ty, Type::none(db))
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
                    UnionType::from_two_elements(db, field.declared_ty, Type::TypeVar(t_default))
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
            UnionType::from_two_elements(db, Type::unknown(), Type::none(db)),
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
pub(super) fn synthesize_typed_dict_update<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let keyword_parameters: Vec<_> = fields
        .into_iter()
        .map(|(field_name, field)| {
            let field_name = field_name.borrow();
            let field = field.borrow();
            let ty = if field.is_read_only() {
                Type::Never
            } else {
                field.declared_ty
            };
            Parameter::keyword_only(field_name.clone())
                .with_annotated_type(ty)
                .with_default_type(ty)
        })
        .collect();

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
            .chain(keyword_parameters),
        ),
        Type::none(db),
    );

    Type::function_like_callable(db, update_signature)
}

/// Synthesize the `pop` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_pop<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let overloads = fields
        .into_iter()
        .filter(|(_, field)| !(*field).borrow().is_required())
        .flat_map(|(field_name, field)| {
            let field_name = field_name.borrow();
            let field = field.borrow();
            let key_type = Type::string_literal(db, field_name.as_str());

            let pop_sig = Signature::new(
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
            );

            let t_default = BoundTypeVarInstance::synthetic(
                db,
                Name::new_static("T"),
                TypeVarVariance::Covariant,
            );

            let pop_with_default_sig = Signature::new_generic(
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
                UnionType::from_two_elements(db, field.declared_ty, Type::TypeVar(t_default)),
            );

            [pop_sig, pop_with_default_sig]
        });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize the `setdefault` method for a `TypedDict`.
pub(super) fn synthesize_typed_dict_setdefault<'db, N, F>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    fields: impl IntoIterator<Item = (N, F)>,
) -> Type<'db>
where
    N: Borrow<Name>,
    F: Borrow<TypedDictField<'db>>,
{
    let overloads = fields.into_iter().map(|(field_name, field)| {
        let field_name = field_name.borrow();
        let field = field.borrow();
        let key_type = Type::string_literal(db, field_name.as_str());

        Signature::new(
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
        )
    });

    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::FunctionLike,
    ))
}

/// Synthesize a merge operator (`__or__`, `__ror__`, or `__ior__`) for a `TypedDict`.
pub(super) fn synthesize_typed_dict_merge<'db>(
    db: &'db dyn Db,
    instance_ty: Type<'db>,
    name: &str,
) -> Type<'db> {
    let mut overloads = vec![Signature::new(
        Parameters::new(
            db,
            [
                Parameter::positional_only(Some(Name::new_static("self")))
                    .with_annotated_type(instance_ty),
                Parameter::positional_only(Some(Name::new_static("value")))
                    .with_annotated_type(instance_ty),
            ],
        ),
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

        overloads.push(Signature::new(
            Parameters::new(
                db,
                [
                    Parameter::positional_only(Some(Name::new_static("self")))
                        .with_annotated_type(instance_ty),
                    Parameter::positional_only(Some(Name::new_static("value")))
                        .with_annotated_type(partial_ty),
                ],
            ),
            instance_ty,
        ));
        overloads.push(Signature::new(
            Parameters::new(
                db,
                [
                    Parameter::positional_only(Some(Name::new_static("self")))
                        .with_annotated_type(instance_ty),
                    Parameter::positional_only(Some(Name::new_static("value")))
                        .with_annotated_type(dict_param_ty),
                ],
            ),
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
    pub name: Name,

    /// The anchor for this dynamic TypedDict, providing stable identity.
    ///
    /// - `Definition`: The call is assigned to a variable. The definition
    ///   uniquely identifies this TypedDict and can be used to find the call.
    /// - `ScopeOffset`: The call is "dangling" (not assigned). The offset
    ///   is relative to the enclosing scope's anchor node index, and the
    ///   eagerly computed spec is stored on the anchor.
    #[returns(ref)]
    pub anchor: DynamicTypedDictAnchor<'db>,
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
        let instance_ty = self.to_instance(db);

        let synthesized = match name {
            "__getitem__" => synthesize_typed_dict_getitem(db, instance_ty, self.items(db)),
            "__setitem__" => synthesize_typed_dict_setitem(db, instance_ty, self.items(db)),
            "__delitem__" => synthesize_typed_dict_delitem(db, instance_ty, self.items(db)),
            "get" => synthesize_typed_dict_get(db, instance_ty, self.items(db)),
            "update" => synthesize_typed_dict_update(db, instance_ty, self.items(db)),
            "pop" => synthesize_typed_dict_pop(db, instance_ty, self.items(db)),
            "setdefault" => synthesize_typed_dict_setdefault(db, instance_ty, self.items(db)),
            "__or__" | "__ror__" | "__ior__" => synthesize_typed_dict_merge(db, instance_ty, name),
            _ => return Member::default(),
        };

        Member::definitely_declared(synthesized)
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
    #[expect(clippy::unused_self)]
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // Fall back to TypedDictFallback for instance members.
        KnownClass::TypedDictFallback
            .to_instance(db)
            .instance_member(db, name)
    }
}
