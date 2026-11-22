use std::borrow::Cow;

use crate::ProgramEnvironment;
use ruff_db::{diagnostic::Span, parsed::parsed_module};
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_text_size::TextRange;

use crate::{
    Db,
    place::{Place, PlaceAndQualifiers},
    types::{
        BindingContext, BoundTypeVarInstance, CallableType, ClassBase, ClassLiteral, ClassType,
        GenericContext, KnownClass, KnownInstanceType, MemberLookupPolicy, Parameter, Parameters,
        PropertyInstanceType, SelfTypeVarOrigin, Signature, SubclassOfType, Type, TypeContext,
        TypeMapping, UnionBuilder,
        callable::CallableTypeKind,
        class::{DynamicClassHeaderAnchor, DynamicClassScopeOffset, dynamic_class_header_range},
        definition_expression_type,
        infer::InferenceFlags,
        member::Member,
        mro::Mro,
        signatures::CallableSignature,
        tuple::{FixedLengthTuple, TupleSpec, TupleSpecBuilder, TupleType},
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
    env: &ProgramEnvironment<'db>,
    name: &str,
    class: ClassType<'db>,
    fields: impl Iterator<Item = NamedTupleField<'db>>,
    inherited_generic_context: Option<GenericContext<'db>>,
) -> Option<Type<'db>> {
    let instance_ty = Type::instance(db, env, class);
    let origin = SelfTypeVarOrigin::Class(class.class_literal(db));
    match name {
        "__new__" => {
            // __new__(cls, field1, field2, ...) -> Self
            let self_typevar = BoundTypeVarInstance::synthetic_self(
                db,
                origin,
                instance_ty,
                BindingContext::Synthetic(env.program(db)),
            );
            let self_ty = Type::TypeVar(self_typevar);

            let variables = inherited_generic_context
                .iter()
                .flat_map(|ctx| ctx.variables(db))
                .chain(std::iter::once(self_typevar));

            let generic_context = GenericContext::from_typevar_instances(db, env, variables);

            // CPython generates namedtuple `__new__` as `(_cls, field1, ...)` so field names like
            // `cls` remain usable as keyword arguments at call sites.
            let first_parameter = Parameter::positional_or_keyword(Name::new_static("_cls"))
                .with_annotated_type(SubclassOfType::from(db, env, self_typevar));

            let signature =
                |parameters| Signature::new_generic(Some(generic_context), parameters, self_ty);
            if let ClassLiteral::DynamicNamedTuple(named_tuple) = class.class_literal(db)
                && !named_tuple.has_known_fields(db)
            {
                let parameters = partial_constructor_parameters(
                    db,
                    env,
                    &first_parameter,
                    named_tuple.spec(db).fields(db),
                );
                return Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(parameters.into_iter().map(signature)),
                    CallableTypeKind::FunctionLike,
                )));
            }

            let parameters = std::iter::once(first_parameter).chain(fields.map(|field| {
                Parameter::positional_or_keyword(field.name)
                    .with_annotated_type(field.ty)
                    .with_optional_default_type(field.default)
                    .with_definition(field.definition)
            }));

            let signature = signature(Parameters::standard(parameters));
            Some(Type::function_like_callable(db, signature))
        }
        "__match_args__" => {
            if env.python_version(db) < PythonVersion::PY310 {
                return None;
            }

            // __match_args__: tuple[Literal["field1"], Literal["field2"], ...]
            let field_types = fields.map(|field| Type::string_literal(db, &field.name));
            Some(Type::heterogeneous_tuple(db, env, field_types))
        }
        "_fields" => {
            // _fields: tuple[Literal["field1"], Literal["field2"], ...]
            let field_types = fields.map(|field| Type::string_literal(db, &field.name));
            Some(Type::heterogeneous_tuple(db, env, field_types))
        }
        "__slots__" => {
            // __slots__: tuple[()] - always empty for namedtuples
            Some(Type::empty_tuple(db, env))
        }
        "_replace" | "__replace__" => {
            if name == "__replace__" && env.python_version(db) < PythonVersion::PY313 {
                return None;
            }

            // _replace(self, *, field1=..., field2=...) -> Self
            let self_ty = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                db,
                origin,
                instance_ty,
                BindingContext::Synthetic(env.program(db)),
            ));

            let first_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                .with_annotated_type(self_ty);

            let parameters = std::iter::once(first_parameter).chain(fields.map(|field| {
                Parameter::keyword_only(field.name)
                    .with_annotated_type(field.ty)
                    .with_default_type(field.ty)
                    .with_definition(field.definition)
            }));

            let signature = Signature::new(Parameters::standard(parameters), self_ty);
            Some(Type::function_like_callable(db, signature))
        }
        "__init__" => {
            // Namedtuples don't have a custom __init__. All construction happens in __new__.
            None
        }
        _ => {
            // Fall back to NamedTupleFallback for other synthesized methods.
            KnownClass::NamedTupleFallback
                .to_class_literal(db, env)
                .as_class_literal()?
                .as_static()?
                .own_class_member(db, env, inherited_generic_context, None, name)
                .ignore_possibly_undefined()
        }
    }
}

#[derive(Debug, get_size2::GetSize, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub struct NamedTupleField<'db> {
    pub(crate) name: Name,
    pub(crate) ty: Type<'db>,
    pub(crate) default: Option<Type<'db>>,
    /// The field's first declaration for a class based named tuple.
    pub(crate) definition: Option<Definition<'db>>,
}

/// A field or a variable-length sequence of fields in a functional named tuple.
#[derive(Debug, get_size2::GetSize, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum FunctionalNamedTupleField<'db> {
    Named(NamedTupleField<'db>),
    /// One position whose name is unknown.
    Unnamed(Type<'db>),
    /// Zero or more positions with the given element type.
    Variadic(Type<'db>),
}

impl<'db> FunctionalNamedTupleField<'db> {
    pub(in crate::types) fn required(name: Option<&str>, ty: Type<'db>) -> Self {
        match name {
            Some(name) => Self::Named(NamedTupleField {
                name: Name::new(name),
                ty,
                default: None,
                definition: None,
            }),
            None => Self::Unnamed(ty),
        }
    }

    fn as_named(&self) -> Option<&NamedTupleField<'db>> {
        match self {
            Self::Named(field) => Some(field),
            Self::Unnamed(_) | Self::Variadic(_) => None,
        }
    }

    const fn is_variadic(&self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    pub(in crate::types) fn ty(&self) -> Type<'db> {
        match self {
            Self::Named(field) => field.ty,
            Self::Unnamed(ty) | Self::Variadic(ty) => *ty,
        }
    }
}

fn field_tuple<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    fields: &[FunctionalNamedTupleField<'db>],
) -> TupleType<'db> {
    let mut tuple = TupleSpecBuilder::with_capacity(fields.len());
    for field in fields {
        if let FunctionalNamedTupleField::Variadic(ty) = field {
            tuple = tuple.concat(db, env, &TupleSpec::homogeneous(*ty));
        } else {
            tuple.push(field.ty());
        }
    }
    TupleType::new(db, env, &tuple.build())
}

/// Synthesize overloads for the ways a caller can divide the fields between positional and
/// keyword arguments. For example, with `[*fields, ("last", bytes)]`, either the positional
/// arguments end in a `bytes`, or the caller must supply `last` as a keyword.
fn partial_constructor_parameters<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: &Parameter<'db>,
    fields: &[FunctionalNamedTupleField<'db>],
) -> Vec<Parameters<'db>> {
    const MAX_PRECISE_SPLITS: usize = 32;

    let first_variadic = fields
        .iter()
        .position(FunctionalNamedTupleField::is_variadic)
        .unwrap_or(fields.len());
    let parameters = |fixed: &[_], variadic: &[_], keywords: &[_]| {
        parameters_for_split(db, env, receiver.clone(), fixed, variadic, keywords)
    };
    let mut overloads = Vec::new();
    for split in (0..=fields.len()).rev() {
        let (positional, keywords) = fields.split_at(split);
        // Moving a named field into the fixed prefix still lets the caller pass it by keyword.
        if split < first_variadic
            && let Some(FunctionalNamedTupleField::Named(_)) = keywords.first()
        {
            continue;
        }
        // The split after a variadic segment allows any number of its fields positionally,
        // including zero. The split before it only adds an overload if the preceding segment
        // is also variadic and can contribute its own type to both `*args` and `**kwargs`.
        if let Some(FunctionalNamedTupleField::Variadic(_)) = keywords.first()
            && !positional
                .last()
                .is_some_and(FunctionalNamedTupleField::is_variadic)
        {
            continue;
        }
        // Each overload can contain a parameter per field. To avoid quadratic growth, combine
        // the remaining splits into one overload. It requires the named fields after the split
        // as keywords and accepts arbitrary positional and keyword arguments for earlier fields.
        if overloads.len() == MAX_PRECISE_SPLITS && !positional.is_empty() {
            let prefix = [FunctionalNamedTupleField::Variadic(Type::unknown())];
            overloads.push(parameters(&[], &prefix, keywords));
            break;
        }
        let (fixed, variadic) = positional.split_at(first_variadic.min(split));
        overloads.push(parameters(fixed, variadic, keywords));
    }
    overloads
}

/// Build one constructor overload. The positional fields consist of a fixed prefix followed
/// by `variadic`, which starts at the first variable-length segment and runs up to the split.
fn parameters_for_split<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Parameter<'db>,
    fixed: &[FunctionalNamedTupleField<'db>],
    variadic: &[FunctionalNamedTupleField<'db>],
    keywords: &[FunctionalNamedTupleField<'db>],
) -> Parameters<'db> {
    let mut parameters = vec![receiver];
    // To pass an unnamed field positionally, the caller must also pass every earlier
    // field positionally. The following named fields can be passed either way.
    let last_unnamed = fixed.iter().rposition(|f| f.as_named().is_none());
    for (index, field) in fixed.iter().enumerate() {
        let name = field.as_named().map(|f| f.name.clone());
        let parameter = match name {
            Some(name) if last_unnamed.is_none_or(|last| index > last) => {
                Parameter::positional_or_keyword(name)
            }
            name => Parameter::positional_only(name),
        };
        parameters.push(parameter.with_annotated_type(field.ty()));
    }
    if let [FunctionalNamedTupleField::Variadic(ty)] = variadic {
        parameters.push(Parameter::variadic(Name::new_static("args")).with_annotated_type(*ty));
    } else if !variadic.is_empty() {
        // A starred annotation describes the arguments together. For example,
        // `*args: *tuple[*tuple[Unknown, ...], bytes]` requires a final `bytes`.
        parameters.push(
            Parameter::variadic(Name::new_static("args"))
                .with_annotated_type(Type::tuple(field_tuple(db, env, variadic)))
                .with_starred_annotation(),
        );
    }

    // The split after a variadic segment also covers splits within it, so its fields
    // can be supplied through both `*args` and `**kwargs`.
    let mut keyword_types = UnionBuilder::new(db, env);
    if let Some(FunctionalNamedTupleField::Variadic(ty)) = variadic.last() {
        keyword_types = keyword_types.add(*ty);
    }
    for field in keywords {
        if let Some(field) = field.as_named() {
            parameters
                .push(Parameter::keyword_only(field.name.clone()).with_annotated_type(field.ty));
        } else {
            // We cannot require a particular keyword for an indeterminate name.
            keyword_types = keyword_types.add(field.ty());
        }
    }
    if let Some(ty) = keyword_types.try_build() {
        parameters
            .push(Parameter::keyword_variadic(Name::new_static("kwargs")).with_annotated_type(ty));
    }
    Parameters::standard(parameters)
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
    /// - `ScopeOffset`: The call is "dangling" (not assigned). Its location
    ///   is relative to the enclosing scope.
    #[returns(ref)]
    pub anchor: DynamicNamedTupleAnchor<'db>,
}

impl get_size2::GetSize for DynamicNamedTupleLiteral<'_> {}

impl<'db> DynamicNamedTupleLiteral<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.name(db),
            self.anchor(db)
                .recursive_type_normalized_impl(db, env, div, nested)?,
        ))
    }
}

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
    fn to_instance(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        Type::instance(db, env, ClassType::NonGeneric(self.into()))
    }

    /// Returns the range of the namedtuple call expression.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let anchor = match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) => {
                DynamicClassHeaderAnchor::Definition(*definition)
            }
            DynamicNamedTupleAnchor::ScopeOffset { offset, .. } => {
                DynamicClassHeaderAnchor::ScopeOffset(*offset)
            }
        };
        dynamic_class_header_range(db, self.scope(db), anchor)
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
        cycle_initial=|db, _, self_: DynamicNamedTupleLiteral<'db>| Mro::from_error(
            db,
            &ProgramEnvironment::from_scope(self_.scope(db)),
            ClassType::NonGeneric(ClassLiteral::DynamicNamedTuple(self_)),
        ),
    )]
    pub(crate) fn mro(self, db: &'db dyn Db) -> Mro<'db> {
        let env = ProgramEnvironment::from_scope(self.scope(db));
        let self_base = ClassBase::Class(ClassType::NonGeneric(self.into()));
        let tuple_class = self.tuple_base_class(db, &env);
        std::iter::once(self_base)
            .chain(tuple_class.iter_mro(db))
            .collect()
    }

    /// Returns the metaclass of this namedtuple.
    ///
    /// Namedtuples always have `type` as their metaclass.
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let env = ProgramEnvironment::from_scope(self.scope(db));
        KnownClass::Type.to_class_literal(db, &env)
    }

    /// Compute the specialized tuple class that this namedtuple inherits from.
    ///
    /// For example, `namedtuple("Point", [("x", int), ("y", int)])` inherits from `tuple[int, int]`.
    pub(crate) fn tuple_base_class(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> ClassType<'db> {
        field_tuple(db, env, self.spec(db).fields(db)).to_class_type(db)
    }

    /// Look up an instance member defined directly on this class (not inherited).
    ///
    /// Return an unbound member for a known field so lookup can use its synthesized class
    /// descriptor. For an unrecognized name, use the fallback if the name could belong to a field.
    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        if let Some(ty) = self.spec(db).attribute_fallback(db)
            && self.field(db, name).is_none()
        {
            return Member::definitely_declared(ty);
        }

        Member::unbound()
    }

    /// Look up an instance member by name (including superclasses).
    pub(crate) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        // First check own instance members.
        let result = self.own_instance_member(db, name);
        if !result.is_undefined() {
            return result.inner;
        }

        // Fall back to the tuple base type for other attributes.
        Type::instance(db, env, self.tuple_base_class(db, env)).instance_member(db, env, name)
    }

    /// Look up a class-level member by name.
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
            .tuple_base_class(db, env)
            .class_literal(db)
            .class_member(db, env, name, policy);

        if result.place.is_undefined()
            && let Some(ty) = self.spec(db).attribute_fallback(db)
        {
            return Place::bound(ty).into();
        }

        result
    }

    /// Look up a class-level member defined directly on this class (not inherited).
    ///
    /// This only checks synthesized members and field properties, without falling
    /// back to tuple or other base classes.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let env = ProgramEnvironment::from_scope(self.scope(db));
        // Handle synthesized namedtuple attributes.
        if let Some(ty) = self.synthesized_class_member(db, &env, name) {
            return Member::definitely_declared(ty);
        }

        // Check if it's a field name (returns a property descriptor).
        if let Some(field) = self.field(db, name) {
            return Member::definitely_declared(create_field_property(db, field.ty));
        }

        Member::default()
    }

    /// Generate synthesized class members for namedtuples.
    fn synthesized_class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Option<Type<'db>> {
        let instance_ty = self.to_instance(db, env);

        // When fields are unknown, handle constructor and field-specific methods specially.
        if !self.has_known_fields(db) {
            match name {
                "__new__" | "__init__"
                    if let [FunctionalNamedTupleField::Variadic(ty)] = self.spec(db).fields(db)
                        && ty.is_unknown() =>
                {
                    let signature = Signature::new(Parameters::gradual_form(), instance_ty);
                    return Some(Type::function_like_callable(db, signature));
                }
                // For other field-specific methods, fall through to NamedTupleFallback.
                "__match_args__" | "_fields" | "_replace" | "__replace__" => {
                    return KnownClass::NamedTupleFallback
                        .to_class_literal(db, env)
                        .as_class_literal()?
                        .as_static()?
                        .own_class_member(db, env, None, None, name)
                        .ignore_possibly_undefined()
                        .map(|ty| {
                            ty.apply_type_mapping(
                                db,
                                env,
                                &TypeMapping::ReplaceSelf {
                                    new_origin: SelfTypeVarOrigin::Class(self.into()),
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
            env,
            name,
            ClassType::NonGeneric(self.into()),
            self.spec(db).named_fields(db).cloned(),
            None,
        );
        // For fallback members from NamedTupleFallback, apply type mapping to handle
        // `Self` types. The explicitly synthesized members (__new__, _fields, _replace,
        // __replace__) don't need this mapping.
        if matches!(
            name,
            "__match_args__" | "__new__" | "_fields" | "_replace" | "__replace__" | "__slots__"
        ) {
            result
        } else {
            result.map(|ty| {
                ty.apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::ReplaceSelf {
                        new_origin: SelfTypeVarOrigin::Class(self.into()),
                        new_upper_bound: instance_ty,
                    },
                    TypeContext::default(),
                )
            })
        }
    }

    fn spec(self, db: &'db dyn Db) -> NamedTupleSpec<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|db, _, _| NamedTupleSpec::unknown(db, Type::unknown()),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn deferred_spec<'db>(db: &'db dyn Db, definition: Definition<'db>) -> NamedTupleSpec<'db> {
            let python_file = definition.python_file(db);
            let module = parsed_module(db, python_file).load(db);
            let node = definition
                .kind(db)
                .value(&module)
                .expect("Expected `NamedTuple` definition to be an assignment")
                .as_call_expr()
                .expect("Expected `NamedTuple` definition r.h.s. to be a call expression");
            match definition_expression_type(db, definition, &node.arguments.args[1]) {
                Type::KnownInstance(KnownInstanceType::NamedTupleSpec(spec)) => spec,
                ty => NamedTupleSpec::from_fields_type(db, definition.scope(db), ty, |_| {}),
            }
        }

        match self.anchor(db) {
            DynamicNamedTupleAnchor::CollectionsDefinition { spec, .. }
            | DynamicNamedTupleAnchor::ScopeOffset { spec, .. } => *spec,
            DynamicNamedTupleAnchor::TypingDefinition(definition) => deferred_spec(db, *definition),
        }
    }

    /// Returns the field declared directly on this dynamic named tuple, if any.
    pub(crate) fn field(self, db: &'db dyn Db, name: &str) -> Option<&'db NamedTupleField<'db>> {
        self.spec(db)
            .named_fields(db)
            .find(|field| field.name == name)
    }

    pub(super) fn has_known_fields(self, db: &'db dyn Db) -> bool {
        self.spec(db).attribute_fallback(db).is_none()
    }
}

/// Anchor for identifying a dynamic `namedtuple`/`NamedTuple` class literal.
///
/// This enum provides stable identity for `DynamicNamedTupleLiteral` instances:
/// - For assigned calls, the `Definition` uniquely identifies the class.
/// - For dangling calls, a relative offset provides stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
    /// The [`DynamicClassScopeOffset`] locates the call relative to the enclosing scope,
    /// including when the call is inside a string annotation.
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
        offset: DynamicClassScopeOffset,
        spec: NamedTupleSpec<'db>,
    },
}

impl<'db> DynamicNamedTupleAnchor<'db> {
    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::CollectionsDefinition { definition, spec } => Some(Self::CollectionsDefinition {
                definition: *definition,
                spec: spec.recursive_type_normalized_impl(db, env, div, nested)?,
            }),
            Self::TypingDefinition(definition) => Some(Self::TypingDefinition(*definition)),
            Self::ScopeOffset {
                scope,
                offset,
                spec,
            } => Some(Self::ScopeOffset {
                scope: *scope,
                offset: *offset,
                spec: spec.recursive_type_normalized_impl(db, env, div, nested)?,
            }),
        }
    }
}

/// A specification describing the fields of a dynamic `namedtuple`
/// or `NamedTuple` class.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct NamedTupleSpec<'db> {
    #[returns(deref)]
    pub(crate) fields: Box<[FunctionalNamedTupleField<'db>]>,
    /// Type of an unrecognized attribute that might be a field. `None` if all names are known.
    #[returns(copy)]
    attribute_fallback: Option<Type<'db>>,
}

impl<'db> NamedTupleSpec<'db> {
    pub(in crate::types) fn named_fields(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = &'db NamedTupleField<'db>> {
        self.fields(db)
            .iter()
            .filter_map(FunctionalNamedTupleField::as_named)
    }

    /// Infer the fields of an indirect `fields` argument and report the first problem.
    pub(in crate::types) fn from_fields_type(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        ty: Type<'db>,
        report: impl FnOnce(Cow<'static, str>),
    ) -> Self {
        let env = ProgramEnvironment::from_scope(scope);
        let expected = "`fields` must be a list literal or a fixed-length tuple";
        let mut problem = None;
        let pairs = Self::field_sequence(db, &env, ty, expected, &mut problem);
        let mut fields = Vec::new();
        Self::extend_fields(db, scope, &pairs, &mut fields, &mut problem);
        if let Some(reason) = problem {
            report(reason);
        }
        Self::from_fields(db, fields.into_boxed_slice())
    }

    /// Append fields from a sequence of `(name, annotation)` pairs. For a variable-length
    /// sequence, retain its fixed prefix and suffix for lookup by name and index.
    pub(in crate::types) fn extend_fields(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        pairs: &TupleSpec<'db>,
        fields: &mut Vec<FunctionalNamedTupleField<'db>>,
        problem: &mut Option<Cow<'static, str>>,
    ) {
        if matches!(pairs, TupleSpec::Variable(_)) {
            problem.get_or_insert("Cannot determine the number of fields".into());
        }
        let mut field = |pair| Self::field_from_type(db, scope, pair, problem);
        match pairs {
            TupleSpec::Fixed(pairs) => fields.extend(pairs.iter_all_elements().map(field)),
            TupleSpec::Variable(pairs) => {
                fields.extend(pairs.iter_prefix_elements().map(&mut field));
                let variable = field(pairs.variable().element_type(db));
                fields.push(FunctionalNamedTupleField::Variadic(variable.ty()));
                fields.extend(pairs.iter_suffix_elements().map(field));
            }
        }
    }

    /// For an indirect field list or pair, expect a tuple. If given another iterable,
    /// record the problem and use its element types for recovery.
    fn field_sequence(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        expected: &'static str,
        problem: &mut Option<Cow<'static, str>>,
    ) -> Cow<'db, TupleSpec<'db>> {
        ty.resolve_type_alias(db)
            .tuple_instance_spec(db, env)
            .unwrap_or_else(|| {
                problem.get_or_insert(expected.into());
                ty.iterate(db, env)
            })
    }

    /// Recover the name and declared type of a computed field pair independently, recording
    /// the first inference problem in `problem`.
    pub(in crate::types) fn field_from_type(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        pair: Type<'db>,
        problem: &mut Option<Cow<'static, str>>,
    ) -> FunctionalNamedTupleField<'db> {
        let env = ProgramEnvironment::from_scope(scope);
        let expected = "Expected a `(name, type)` pair for each field";
        let pair = Self::field_sequence(db, &env, pair, expected, problem);
        let elements = pair.as_fixed_length();
        let Some([name, value]) = elements.map(FixedLengthTuple::all_elements) else {
            problem.get_or_insert(expected.into());
            return FunctionalNamedTupleField::Unnamed(Type::unknown());
        };
        let name = name
            .resolve_type_alias(db)
            .as_string_literal()
            .map(|s| s.value(db));
        if name.is_none() {
            problem.get_or_insert("Expected a string literal for each field name".into());
        }
        let ty = value
            .default_specialize(db, &env)
            .in_type_expression(db, scope, None, InferenceFlags::empty())
            .unwrap_or_else(|_| {
                if let Some(name) = name {
                    problem.get_or_insert_with(|| {
                        format!(
                            "Cannot determine the type of field `{name}` from `{}`",
                            value.display(db, &env)
                        )
                        .into()
                    });
                }
                Type::unknown()
            });
        FunctionalNamedTupleField::required(name, ty)
    }

    pub(in crate::types) fn from_fields(
        db: &'db dyn Db,
        fields: Box<[FunctionalNamedTupleField<'db>]>,
    ) -> Self {
        let fallback = fields
            .iter()
            .any(|field| field.as_named().is_none())
            .then(Type::unknown);
        Self::new(db, fields, fallback)
    }

    /// Create a spec for a named tuple whose fields are entirely unknown.
    pub(crate) fn unknown(db: &'db dyn Db, attribute_fallback: Type<'db>) -> Self {
        let fields = Box::from([FunctionalNamedTupleField::Variadic(Type::unknown())]);
        Self::new(db, fields, Some(attribute_fallback))
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let normalize = |ty: Type<'db>| {
            let normalized = ty.recursive_type_normalized_impl(db, env, div, true);
            if nested {
                normalized
            } else {
                Some(normalized.unwrap_or(div))
            }
        };
        let fields = self
            .fields(db)
            .iter()
            .map(|field| {
                let ty = normalize(field.ty())?;
                Some(match field {
                    FunctionalNamedTupleField::Named(field) => {
                        let default = match field.default {
                            Some(default) => Some(normalize(default)?),
                            None => None,
                        };
                        FunctionalNamedTupleField::Named(NamedTupleField {
                            ty,
                            default,
                            ..field.clone()
                        })
                    }
                    FunctionalNamedTupleField::Unnamed(_) => FunctionalNamedTupleField::Unnamed(ty),
                    FunctionalNamedTupleField::Variadic(_) => {
                        FunctionalNamedTupleField::Variadic(ty)
                    }
                })
            })
            .collect::<Option<Box<_>>>()?;
        Some(Self::new(db, fields, self.attribute_fallback(db)))
    }
}

impl get_size2::GetSize for NamedTupleSpec<'_> {}

/// Create a property type for a namedtuple field.
fn create_field_property<'db>(db: &'db dyn Db, field_ty: Type<'db>) -> Type<'db> {
    let property_getter_signature = Signature::new(
        Parameters::standard([Parameter::positional_only(Some(Name::new_static("self")))]),
        field_ty,
    );
    let property_getter = Type::single_callable(db, property_getter_signature);
    let property = PropertyInstanceType::new(db, Some(property_getter), None, None);
    Type::PropertyInstance(property)
}
