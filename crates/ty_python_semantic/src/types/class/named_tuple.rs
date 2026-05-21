use ruff_db::{diagnostic::Span, parsed::parsed_module};
use ruff_python_ast as ast;
use ruff_python_ast::{NodeIndex, PythonVersion, name::Name};
use ruff_text_size::{Ranged, TextRange};

use crate::{
    Db, Program,
    place::{Place, PlaceAndQualifiers},
    types::{
        BindingContext, BoundTypeVarInstance, ClassBase, ClassLiteral, ClassType, GenericContext,
        KnownClass, KnownInstanceType, MemberLookupPolicy, Parameter, Parameters,
        PropertyInstanceType, Signature, SubclassOfType, Type, TypeContext, TypeMapping,
        definition_expression_type, member::Member, mro::Mro, tuple::TupleType,
    },
};
use ty_python_core::{definition::Definition, scope::ScopeId};

/// Synthesize a namedtuple class member given the field information.
///
/// This is used by both `DynamicNamedTupleLiteral` and `StaticClassLiteral` (for declarative
/// namedtuples) to avoid duplicating the synthesis logic.
///
/// The `inherited_generic_context` parameter is used for declarative namedtuples to preserve
/// generic context in the synthesized `__new__` signature.
pub(super) fn synthesize_namedtuple_class_member<'db>(
    db: &'db dyn Db,
    name: &str,
    instance_ty: Type<'db>,
    fields: impl Iterator<Item = NamedTupleField<'db>>,
    inherited_generic_context: Option<GenericContext<'db>>,
) -> Option<Type<'db>> {
    match name {
        "__new__" => {
            // __new__(cls, field1, field2, ...) -> Self
            let self_typevar =
                BoundTypeVarInstance::synthetic_self(db, instance_ty, BindingContext::Synthetic);
            let self_ty = Type::TypeVar(self_typevar);

            let variables = inherited_generic_context
                .iter()
                .flat_map(|ctx| ctx.variables(db))
                .chain(std::iter::once(self_typevar));

            let generic_context = GenericContext::from_typevar_instances(db, variables);

            // CPython generates namedtuple `__new__` as `(_cls, field1, ...)` so field names like
            // `cls` remain usable as keyword arguments at call sites.
            let first_parameter = Parameter::positional_or_keyword(Name::new_static("_cls"))
                .with_annotated_type(SubclassOfType::from(db, self_typevar));

            let parameters = std::iter::once(first_parameter).chain(fields.map(|field| {
                Parameter::positional_or_keyword(field.name)
                    .with_annotated_type(field.ty)
                    .with_optional_default_type(field.default)
                    .with_definition(field.definition)
            }));

            let signature = Signature::new_generic(
                Some(generic_context),
                Parameters::new(db, parameters),
                self_ty,
            );
            Some(Type::function_like_callable(db, signature))
        }
        "_fields" => {
            // _fields: tuple[Literal["field1"], Literal["field2"], ...]
            let field_types = fields.map(|field| Type::string_literal(db, &field.name));
            Some(Type::heterogeneous_tuple(db, field_types))
        }
        "__slots__" => {
            // __slots__: tuple[()] - always empty for namedtuples
            Some(Type::empty_tuple(db))
        }
        "_replace" | "__replace__" => {
            if name == "__replace__" && Program::get(db).python_version(db) < PythonVersion::PY313 {
                return None;
            }

            // _replace(self, *, field1=..., field2=...) -> Self
            let self_ty = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                db,
                instance_ty,
                BindingContext::Synthetic,
            ));

            let first_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                .with_annotated_type(self_ty);

            let parameters = std::iter::once(first_parameter).chain(fields.map(|field| {
                Parameter::keyword_only(field.name)
                    .with_annotated_type(field.ty)
                    .with_default_type(field.ty)
                    .with_definition(field.definition)
            }));

            let signature = Signature::new(Parameters::new(db, parameters), self_ty);
            Some(Type::function_like_callable(db, signature))
        }
        "__init__" => {
            // Namedtuples don't have a custom __init__. All construction happens in __new__.
            None
        }
        _ => {
            // Fall back to NamedTupleFallback for other synthesized methods.
            KnownClass::NamedTupleFallback
                .to_class_literal(db)
                .as_class_literal()?
                .as_static()?
                .own_class_member(db, inherited_generic_context, None, name)
                .ignore_possibly_undefined()
        }
    }
}

#[derive(Debug, salsa::Update, get_size2::GetSize, Clone, PartialEq, Eq, Hash)]
pub struct NamedTupleField<'db> {
    pub(crate) name: Name,
    pub(crate) ty: Type<'db>,
    pub(crate) default: Option<Type<'db>>,
    /// The field's first declaration for a class based named tuple.
    pub(crate) definition: Option<Definition<'db>>,
}

/// A namedtuple created via the functional form `namedtuple(name, fields)` or
/// `NamedTuple(name, fields)`.
///
/// For example:
/// ```python
/// from collections import namedtuple
/// Point = namedtuple("Point", ["x", "y"])
///
/// from typing import NamedTuple
/// Person = NamedTuple("Person", [("name", str), ("age", int)])
/// ```
///
/// The type of `Point` would be `type[Point]` where `Point` is a `DynamicNamedTupleLiteral`.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct DynamicNamedTupleLiteral<'db> {
    /// The name of the namedtuple (from the first argument).
    #[returns(ref)]
    pub name: Name,

    /// The anchor for this dynamic namedtuple, providing stable identity.
    ///
    /// - `Definition`: The call is assigned to a variable. The definition
    ///   uniquely identifies this namedtuple and can be used to find the call.
    /// - `ScopeOffset`: The call is "dangling" (not assigned). The offset
    ///   is relative to the enclosing scope's anchor node index.
    #[returns(ref)]
    pub anchor: DynamicNamedTupleAnchor<'db>,
}

impl get_size2::GetSize for DynamicNamedTupleLiteral<'_> {}

#[salsa::tracked]
impl<'db> DynamicNamedTupleLiteral<'db> {
    /// Returns the definition where this namedtuple is created, if it was assigned to a variable.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) => Some(*definition),
            DynamicNamedTupleAnchor::ScopeOffset { .. } => None,
        }
    }

    /// Returns the scope in which this dynamic class was created.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) => definition.scope(db),
            DynamicNamedTupleAnchor::ScopeOffset { scope, .. } => *scope,
        }
    }

    /// Returns an instance type for this dynamic namedtuple.
    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self.into()))
    }

    /// Returns the range of the namedtuple call expression.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let file = scope.file(db);
        let module = parsed_module(db, file).load(db);

        match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) => {
                // For definitions, get the range from the definition's value.
                // The namedtuple call is the value of the assignment.
                definition
                    .kind(db)
                    .value(&module)
                    .expect("DynamicClassAnchor::Definition should only be used for assignments")
                    .range()
            }
            DynamicNamedTupleAnchor::ScopeOffset { offset, .. } => {
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

    /// Returns a [`Span`] pointing to the namedtuple call expression.
    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    /// Compute the MRO for this namedtuple.
    ///
    /// The MRO is the MRO of the class's tuple base class, prepended by `self`.
    /// For example, `namedtuple("Point", [("x", int), ("y", int)])` has the following MRO:
    ///
    /// 1. `<class 'Point'>`
    /// 2. `<class 'tuple[int, int]'>`
    /// 3. `<class 'Sequence[int]'>`
    /// 4. `<class 'Reversible[int]'>`
    /// 5. `<class 'Collection[int]'>`
    /// 6. `<class 'Iterable[int]'>`
    /// 7. `<class 'Container[int]'>`
    /// 8. `typing.Protocol`
    /// 9. `typing.Generic`
    /// 10. `<class 'object'>`
    #[salsa::tracked(
        returns(ref),
        heap_size=ruff_memory_usage::heap_size,
        cycle_initial=|db, _, self_| Mro::from_error(
            db, ClassType::NonGeneric(ClassLiteral::DynamicNamedTuple(self_)),
        ),
    )]
    pub(crate) fn mro(self, db: &'db dyn Db) -> Mro<'db> {
        let self_base = ClassBase::Class(ClassType::NonGeneric(self.into()));
        let tuple_class = self.tuple_base_class(db);
        std::iter::once(self_base)
            .chain(tuple_class.iter_mro(db))
            .collect()
    }

    /// Get the metaclass of this dynamic namedtuple.
    ///
    /// Namedtuples always have `type` as their metaclass.
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let _ = self;
        KnownClass::Type.to_class_literal(db)
    }

    /// Compute the specialized tuple class that this namedtuple inherits from.
    ///
    /// For example, `namedtuple("Point", [("x", int), ("y", int)])` inherits from `tuple[int, int]`.
    pub(crate) fn tuple_base_class(self, db: &'db dyn Db) -> ClassType<'db> {
        // If fields are unknown, return `tuple[Unknown, ...]` to avoid false positives
        // like index-out-of-bounds errors.
        if !self.has_known_fields(db) {
            return TupleType::homogeneous(db, Type::unknown()).to_class_type(db);
        }

        let field_types = self.fields(db).iter().map(|field| field.ty);
        TupleType::heterogeneous(db, field_types)
            .map(|t| t.to_class_type(db))
            .unwrap_or_else(|| {
                KnownClass::Tuple
                    .to_class_literal(db)
                    .as_class_literal()
                    .expect("tuple should be a class literal")
                    .default_specialization(db)
            })
    }

    /// Look up an instance member defined directly on this class (not inherited).
    ///
    /// `NamedTuple` fields are exposed via synthesized descriptors on the class rather than
    /// instance attributes. If fields are unknown (dynamic), return `Any` for any attribute.
    pub(super) fn own_instance_member(self, db: &'db dyn Db, _name: &str) -> Member<'db> {
        if !self.has_known_fields(db) {
            return Member::definitely_declared(Type::any());
        }

        Member::unbound()
    }

    /// Look up an instance member by name (including superclasses).
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // First check own instance members.
        let result = self.own_instance_member(db, name);
        if !result.is_undefined() {
            return result.inner;
        }

        // Fall back to the tuple base type for other attributes.
        Type::instance(db, self.tuple_base_class(db)).instance_member(db, name)
    }

    /// Look up a class-level member by name.
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        // First check synthesized members and fields.
        let member = self.own_class_member(db, name);
        if !member.is_undefined() {
            return member.inner;
        }

        // Fall back to tuple class members.
        let result = self
            .tuple_base_class(db)
            .class_literal(db)
            .class_member(db, name, policy);

        // If fields are unknown (dynamic) and the attribute wasn't found,
        // return `Any` instead of failing.
        if !self.has_known_fields(db) && result.place.is_undefined() {
            return Place::bound(Type::any()).into();
        }

        result
    }

    /// Look up a class-level member defined directly on this class (not inherited).
    ///
    /// This only checks synthesized members and field properties, without falling
    /// back to tuple or other base classes.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        // Handle synthesized namedtuple attributes.
        if let Some(ty) = self.synthesized_class_member(db, name) {
            return Member::definitely_declared(ty);
        }

        // Check if it's a field name (returns a property descriptor).
        for field in self.fields(db) {
            if field.name == name {
                return Member::definitely_declared(create_field_property(db, field.ty));
            }
        }

        Member::default()
    }

    /// Generate synthesized class members for namedtuples.
    fn synthesized_class_member(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        let instance_ty = self.to_instance(db);

        // When fields are unknown, handle constructor and field-specific methods specially.
        if !self.has_known_fields(db) {
            match name {
                // For constructors, return a gradual signature that accepts any arguments.
                "__new__" | "__init__" => {
                    let signature = Signature::new(Parameters::gradual_form(), instance_ty);
                    return Some(Type::function_like_callable(db, signature));
                }
                // For other field-specific methods, fall through to NamedTupleFallback.
                "_fields" | "_replace" | "__replace__" => {
                    return KnownClass::NamedTupleFallback
                        .to_class_literal(db)
                        .as_class_literal()?
                        .as_static()?
                        .own_class_member(db, None, None, name)
                        .ignore_possibly_undefined()
                        .map(|ty| {
                            ty.apply_type_mapping(
                                db,
                                &TypeMapping::ReplaceSelf {
                                    new_upper_bound: instance_ty,
                                },
                                TypeContext::default(),
                            )
                        });
                }
                _ => {}
            }
        }

        let result = synthesize_namedtuple_class_member(
            db,
            name,
            instance_ty,
            self.fields(db).iter().cloned(),
            None,
        );
        // For fallback members from NamedTupleFallback, apply type mapping to handle
        // `Self` types. The explicitly synthesized members (__new__, _fields, _replace,
        // __replace__) don't need this mapping.
        if matches!(
            name,
            "__new__" | "_fields" | "_replace" | "__replace__" | "__slots__"
        ) {
            result
        } else {
            result.map(|ty| {
                ty.apply_type_mapping(
                    db,
                    &TypeMapping::ReplaceSelf {
                        new_upper_bound: instance_ty,
                    },
                    TypeContext::default(),
                )
            })
        }
    }

    fn spec(self, db: &'db dyn Db) -> NamedTupleSpec<'db> {
        #[salsa::tracked(
            cycle_initial=|db, _, _| NamedTupleSpec::unknown(db),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn deferred_spec<'db>(db: &'db dyn Db, definition: Definition<'db>) -> NamedTupleSpec<'db> {
            let module = parsed_module(db, definition.file(db)).load(db);
            let node = definition
                .kind(db)
                .value(&module)
                .expect("Expected `NamedTuple` definition to be an assignment")
                .as_call_expr()
                .expect("Expected `NamedTuple` definition r.h.s. to be a call expression");
            match definition_expression_type(db, definition, &node.arguments.args[1]) {
                Type::KnownInstance(KnownInstanceType::NamedTupleSpec(spec)) => spec,
                _ => NamedTupleSpec::unknown(db),
            }
        }

        match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { spec, .. }
            | DynamicNamedTupleAnchor::ScopeOffset { spec, .. } => *spec,
            DynamicNamedTupleAnchor::TypingDefinition(definition) => deferred_spec(db, *definition),
        }
    }

    fn fields(self, db: &'db dyn Db) -> &'db [NamedTupleField<'db>] {
        self.spec(db).fields(db)
    }

    /// Returns the field declared directly on this dynamic named tuple, if any.
    pub(crate) fn field(self, db: &'db dyn Db, name: &Name) -> Option<&'db NamedTupleField<'db>> {
        self.fields(db).iter().find(|field| field.name == *name)
    }

    pub(super) fn has_known_fields(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_fields(db)
    }
}

/// Anchor for identifying a dynamic `namedtuple`/`NamedTuple` class literal.
///
/// This enum provides stable identity for `DynamicNamedTupleLiteral` instances:
/// - For assigned calls, the `Definition` uniquely identifies the class.
/// - For dangling calls, a relative offset provides stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum DynamicNamedTupleAnchor<'db> {
    /// We're dealing with a `collections.namedtuple()` call
    /// that's assigned to a variable.
    ///
    /// The `Definition` uniquely identifies this class. The `namedtuple()`
    /// call expression is the `value` of the assignment, so we can get its
    /// range from the definition.
    CollectionsDefinition {
        definition: Definition<'db>,
        spec: NamedTupleSpec<'db>,
    },

    /// We're dealing with a `typing.NamedTuple()` call
    /// that's assigned to a variable.
    ///
    /// The `Definition` uniquely identifies this class. The `NamedTuple()`
    /// call expression is the `value` of the assignment, so we can get its
    /// range from the definition.
    ///
    /// Unlike the `CollectionsDefinition` variant, this variant does not
    /// hold a `NamedTupleSpec`. This is because the spec for a
    /// `typing.NamedTuple` call can contain forward references and recursive
    /// references that must be evaluated lazily. The spec is computed
    /// on-demand from the definition.
    TypingDefinition(Definition<'db>),

    /// We're dealing with a `namedtuple()` or `NamedTuple` call that is
    /// "dangling" (not assigned to a variable).
    ///
    /// The offset is relative to the enclosing scope's anchor node index.
    /// For module scope, this is equivalent to an absolute index (anchor is 0).
    ///
    /// Dangling calls can always store the spec. They *can* contain
    /// forward references if they appear in class bases:
    ///
    /// ```python
    /// from typing import NamedTuple
    ///
    /// class F(NamedTuple("F", [("x", "F | None")]):
    ///     pass
    /// ```
    ///
    /// But this doesn't matter, because all class bases are deferred in their
    /// entirety during type inference.
    ScopeOffset {
        scope: ScopeId<'db>,
        offset: u32,
        spec: NamedTupleSpec<'db>,
    },
}

/// A specification describing the fields of a dynamic `namedtuple`
/// or `NamedTuple` class.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct NamedTupleSpec<'db> {
    #[returns(deref)]
    pub(crate) fields: Box<[NamedTupleField<'db>]>,

    pub(crate) has_known_fields: bool,
}

impl<'db> NamedTupleSpec<'db> {
    /// Create a [`NamedTupleSpec`] with the given fields.
    pub(crate) fn known(db: &'db dyn Db, fields: Box<[NamedTupleField<'db>]>) -> Self {
        Self::new(db, fields, true)
    }

    /// Create a [`NamedTupleSpec`] that indicates a namedtuple class has unknown fields.
    pub(crate) fn unknown(db: &'db dyn Db) -> Self {
        Self::new(db, Box::default(), false)
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let fields = self
            .fields(db)
            .iter()
            .map(|f| {
                Some(NamedTupleField {
                    name: f.name.clone(),
                    ty: if nested {
                        f.ty.recursive_type_normalized_impl(db, div, nested)?
                    } else {
                        f.ty.recursive_type_normalized_impl(db, div, nested)
                            .unwrap_or(div)
                    },
                    default: None,
                    definition: f.definition,
                })
            })
            .collect::<Option<Box<_>>>()?;

        Some(Self::new(db, fields, self.has_known_fields(db)))
    }
}

impl get_size2::GetSize for NamedTupleSpec<'_> {}

/// Create a property type for a namedtuple field.
fn create_field_property<'db>(db: &'db dyn Db, field_ty: Type<'db>) -> Type<'db> {
    let property_getter_signature = Signature::new(
        Parameters::new(
            db,
            [Parameter::positional_only(Some(Name::new_static("self")))],
        ),
        field_ty,
    );
    let property_getter = Type::single_callable(db, property_getter_signature);
    let property = PropertyInstanceType::new(db, Some(property_getter), None, None);
    Type::PropertyInstance(property)
}
