use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::{NodeIndex, PythonVersion};
use ruff_text_size::{Ranged, TextRange};

use super::{
    ClassLiteral, ClassMemberResult, CodeGeneratorKind, DynamicMetaclassConflict, FieldKind,
    InstanceMemberResult, MroIterator, MroLookup,
};
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::member::Member;
use crate::types::mro::{DynamicMroError, Mro};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    ClassBase, ClassType, DataclassFlags, DataclassParams, KnownClass, KnownInstanceType,
    MemberLookupPolicy, SubclassOfType, Type, TypeQualifiers, UnionType,
    definition_expression_type, extract_fixed_length_iterable_element_types,
};
use crate::{Db, FxIndexMap, Program};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

/// Field information for synthesizing dataclass methods.
#[derive(Debug, Clone)]
pub(super) struct DataclassFieldInfo<'db> {
    /// The field name (or alias if provided).
    pub(super) name: Name,
    /// The declared type of the field.
    pub(super) ty: Type<'db>,
    /// The default value type, if any.
    pub(super) default_ty: Option<Type<'db>>,
    /// Whether this field should be included in `__init__`.
    pub(super) init: bool,
    /// Whether this field is keyword-only.
    pub(super) kw_only: bool,
    /// The converter types for this field, if a `converter` was specified.
    pub(super) converter: Option<(Type<'db>, Type<'db>)>,
}

/// Synthesize a dataclass class member given the dataclass flags, instance type, and fields.
pub(super) fn synthesize_dataclass_class_member<'db>(
    db: &'db dyn Db,
    name: &str,
    instance_ty: Type<'db>,
    flags: DataclassFlags,
    fields: impl Iterator<Item = DataclassFieldInfo<'db>>,
) -> Option<Type<'db>> {
    match name {
        "__init__" if flags.contains(DataclassFlags::INIT) => {
            let mut parameters = vec![
                Parameter::positional_or_keyword(Name::new_static("self"))
                    .with_annotated_type(instance_ty),
            ];

            for field in fields {
                if !field.init {
                    continue;
                }
                let mut param = if field.kw_only {
                    Parameter::keyword_only(field.name)
                } else {
                    Parameter::positional_or_keyword(field.name)
                };
                let init_ty = field
                    .converter
                    .map(|(converter_input_ty, _)| converter_input_ty)
                    .unwrap_or(field.ty);
                param = param.with_annotated_type(init_ty);
                if let Some(default) = field.default_ty {
                    param = param.with_default_type(default);
                }
                parameters.push(param);
            }

            parameters.sort_by_key(Parameter::is_keyword_only);
            let signature = Signature::new(Parameters::new(db, parameters), Type::none(db));
            Some(Type::function_like_callable(db, signature))
        }
        "__match_args__" if flags.contains(DataclassFlags::MATCH_ARGS) => {
            // __match_args__ includes only fields that are in __init__ and not keyword-only.
            let match_args = fields
                .filter(|field| field.init && !field.kw_only)
                .map(|field| Type::string_literal(db, &field.name));
            Some(Type::heterogeneous_tuple(db, match_args))
        }
        _ => synthesize_dataclass_dunder_method(db, name, instance_ty, flags),
    }
}

/// Synthesize a dataclass dunder method that doesn't require field information.
pub(super) fn synthesize_dataclass_dunder_method<'db>(
    db: &'db dyn Db,
    name: &str,
    instance_ty: Type<'db>,
    flags: DataclassFlags,
) -> Option<Type<'db>> {
    match name {
        "__setattr__" if flags.contains(DataclassFlags::FROZEN) => {
            // Frozen dataclasses have `__setattr__` that returns `Never` (immutable).
            let signature = Signature::new(
                Parameters::new(
                    db,
                    [
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("name")),
                        Parameter::positional_or_keyword(Name::new_static("value")),
                    ],
                ),
                Type::Never,
            );
            Some(Type::function_like_callable(db, signature))
        }
        "__lt__" | "__le__" | "__gt__" | "__ge__" if flags.contains(DataclassFlags::ORDER) => {
            let signature = Signature::new(
                Parameters::new(
                    db,
                    [
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("other"))
                            .with_annotated_type(instance_ty),
                    ],
                ),
                KnownClass::Bool.to_instance(db),
            );
            Some(Type::function_like_callable(db, signature))
        }
        "__hash__" => {
            let has_hash = flags.contains(DataclassFlags::UNSAFE_HASH)
                || (flags.contains(DataclassFlags::FROZEN) && flags.contains(DataclassFlags::EQ));
            if has_hash {
                let signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty)],
                    ),
                    KnownClass::Int.to_instance(db),
                );
                Some(Type::function_like_callable(db, signature))
            } else if flags.contains(DataclassFlags::EQ) && !flags.contains(DataclassFlags::FROZEN)
            {
                // `eq=True` without `frozen=True` sets `__hash__` to `None`.
                Some(Type::none(db))
            } else {
                None
            }
        }
        "__eq__" if flags.contains(DataclassFlags::EQ) => {
            let signature = Signature::new(
                Parameters::new(
                    db,
                    [
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("other"))
                            .with_annotated_type(KnownClass::Object.to_instance(db)),
                    ],
                ),
                KnownClass::Bool.to_instance(db),
            );
            Some(Type::function_like_callable(db, signature))
        }
        "__dataclass_fields__" => {
            let field_any = KnownClass::Field.to_specialized_instance(db, &[Type::any()]);
            Some(
                KnownClass::Dict
                    .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), field_any]),
            )
        }
        "__dataclass_params__" => Some(Type::any()),
        _ => None,
    }
}

/// A single field in a dynamic `make_dataclass` class.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct DataclassFieldSpec<'db> {
    pub name: Name,
    pub ty: Type<'db>,
    pub default_ty: Option<Type<'db>>,
    pub class_default_ty: Option<Type<'db>>,
    pub init: bool,
    pub kw_only: Option<bool>,
    pub alias: Option<Name>,
    pub converter: Option<(Type<'db>, Type<'db>)>,
    pub init_only: bool,
    pub class_var: bool,
}

/// A specification describing the fields and bases of a dynamic `make_dataclass` class.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct DataclassSpec<'db> {
    #[returns(deref)]
    pub(crate) fields: Box<[DataclassFieldSpec<'db>]>,
    pub(crate) has_known_fields: bool,
    #[returns(deref)]
    pub(crate) bases: Box<[ClassBase<'db>]>,
}

impl<'db> DataclassSpec<'db> {
    pub(crate) fn known(
        db: &'db dyn Db,
        fields: Box<[DataclassFieldSpec<'db>]>,
        bases: Box<[ClassBase<'db>]>,
    ) -> Self {
        Self::new(db, fields, true, bases)
    }

    pub(crate) fn unknown(db: &'db dyn Db) -> Self {
        Self::new(db, Box::default(), false, Box::default())
    }

    pub(crate) fn unknown_with_bases(db: &'db dyn Db, bases: Box<[ClassBase<'db>]>) -> Self {
        Self::new(db, Box::default(), false, bases)
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
            .map(|field| {
                let normalized_ty = if nested {
                    field.ty.recursive_type_normalized_impl(db, div, nested)?
                } else {
                    field
                        .ty
                        .recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div)
                };
                let normalized_default = match field.default_ty {
                    Some(default_ty) => {
                        Some(default_ty.recursive_type_normalized_impl(db, div, nested)?)
                    }
                    None => None,
                };
                let normalized_class_default = match field.class_default_ty {
                    Some(default_ty) => {
                        Some(default_ty.recursive_type_normalized_impl(db, div, nested)?)
                    }
                    None => None,
                };
                let normalized_converter = match field.converter {
                    Some((input_ty, output_ty)) if nested => Some((
                        input_ty.recursive_type_normalized_impl(db, div, true)?,
                        output_ty.recursive_type_normalized_impl(db, div, true)?,
                    )),
                    Some((input_ty, output_ty)) => Some((
                        input_ty
                            .recursive_type_normalized_impl(db, div, true)
                            .unwrap_or(div),
                        output_ty
                            .recursive_type_normalized_impl(db, div, true)
                            .unwrap_or(div),
                    )),
                    None => None,
                };
                Some(DataclassFieldSpec {
                    name: field.name.clone(),
                    ty: normalized_ty,
                    default_ty: normalized_default,
                    class_default_ty: normalized_class_default,
                    init: field.init,
                    kw_only: field.kw_only,
                    alias: field.alias.clone(),
                    converter: normalized_converter,
                    init_only: field.init_only,
                    class_var: field.class_var,
                })
            })
            .collect::<Option<Box<_>>>()?;

        let bases = self
            .bases(db)
            .iter()
            .map(|base| base.recursive_type_normalized_impl(db, div, nested))
            .collect::<Option<Box<_>>>()?;

        Some(Self::new(db, fields, self.has_known_fields(db), bases))
    }
}

impl get_size2::GetSize for DataclassSpec<'_> {}

fn dynamic_dataclass_spec_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _definition: Definition<'db>,
) -> DataclassSpec<'db> {
    DataclassSpec::unknown(db)
}

#[salsa::tracked(
    cycle_initial = dynamic_dataclass_spec_cycle_initial,
    heap_size = ruff_memory_usage::heap_size
)]
fn deferred_functional_dataclass_spec<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DataclassSpec<'db> {
    fn fields_argument(arguments: &ast::Arguments) -> Option<&ast::Expr> {
        arguments.args.get(1).or_else(|| {
            arguments
                .find_keyword("fields")
                .map(|keyword| &keyword.value)
        })
    }

    let module = parsed_module(db, definition.file(db)).load(db);
    let node = definition
        .kind(db)
        .value(&module)
        .expect("Expected `make_dataclass` definition to be an assignment")
        .as_call_expr()
        .expect("Expected `make_dataclass` definition r.h.s. to be a call expression");

    if let Some(fields_arg) = fields_argument(&node.arguments)
        && let Type::KnownInstance(KnownInstanceType::DataclassSpec(spec)) =
            definition_expression_type(db, definition, fields_arg)
    {
        return spec;
    }

    let bases = node
        .arguments
        .find_keyword("bases")
        .map(|keyword| {
            extract_fixed_length_iterable_element_types(db, &keyword.value, |expr| {
                definition_expression_type(db, definition, expr)
            })
            .map(|bases| {
                bases
                    .iter()
                    .filter_map(|base| ClassBase::try_from_type(db, *base, None))
                    .collect()
            })
            .unwrap_or_else(|| Box::from([ClassBase::unknown()]))
        })
        .unwrap_or_default();

    DataclassSpec::unknown_with_bases(db, bases)
}

/// Anchor for identifying a dynamic `make_dataclass` class literal.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum DynamicDataclassAnchor<'db> {
    /// The `make_dataclass()` call is assigned to a variable.
    ///
    /// The `Definition` uniquely identifies this dataclass. Field metadata and bases are computed
    /// lazily during deferred inference so recursive references resolve correctly.
    Definition(Definition<'db>),

    /// The `make_dataclass()` call is "dangling" (not assigned to a variable).
    ///
    /// The offset is relative to the enclosing scope's anchor node index. The eagerly computed
    /// `spec` preserves field metadata for inline uses like
    /// `make_dataclass("Point", [("x", int)])(x=1)`.
    ScopeOffset {
        scope: ScopeId<'db>,
        offset: u32,
        spec: DataclassSpec<'db>,
    },
}

/// A dataclass created via the functional form `make_dataclass(name, fields, ...)`.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct DynamicDataclassLiteral<'db> {
    #[returns(ref)]
    pub name: Name,
    pub dataclass_params: DataclassParams<'db>,
    #[returns(ref)]
    pub anchor: DynamicDataclassAnchor<'db>,
    #[returns(deref)]
    pub members: Box<[(Name, Type<'db>)]>,
    pub has_dynamic_namespace: bool,
}

impl get_size2::GetSize for DynamicDataclassLiteral<'_> {}

#[expect(clippy::unnecessary_wraps)]
fn dynamic_dataclass_mro_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    self_: DynamicDataclassLiteral<'db>,
) -> Result<Mro<'db>, DynamicMroError<'db>> {
    Ok(Mro::from_error(
        db,
        ClassType::NonGeneric(ClassLiteral::DynamicDataclass(self_)),
    ))
}

pub(super) fn dynamic_metaclass_from_bases<'db>(
    db: &'db dyn Db,
    bases: &[ClassBase<'db>],
) -> Result<Type<'db>, DynamicMetaclassConflict<'db>> {
    let Some((candidate_base, rest)) = bases.split_first() else {
        return Ok(KnownClass::Type.to_class_literal(db));
    };

    let mut candidate = candidate_base.metaclass(db);
    let mut candidate_base = candidate_base;

    for base in rest {
        let base_metaclass = base.metaclass(db);

        let Some(candidate_class) = candidate.to_class_type(db) else {
            continue;
        };
        let Some(base_metaclass_class) = base_metaclass.to_class_type(db) else {
            continue;
        };

        if base_metaclass_class.is_subclass_of(db, candidate_class) {
            candidate = base_metaclass;
            candidate_base = base;
            continue;
        }

        if candidate_class.is_subclass_of(db, base_metaclass_class) {
            continue;
        }

        return Err(DynamicMetaclassConflict {
            metaclass1: candidate_class,
            base1: *candidate_base,
            metaclass2: base_metaclass_class,
            base2: *base,
        });
    }

    Ok(candidate)
}

#[salsa::tracked]
impl<'db> DynamicDataclassLiteral<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicDataclassAnchor::Definition(definition) => Some(*definition),
            DynamicDataclassAnchor::ScopeOffset { .. } => None,
        }
    }

    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicDataclassAnchor::Definition(definition) => definition.scope(db),
            DynamicDataclassAnchor::ScopeOffset { scope, .. } => *scope,
        }
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self.into()))
    }

    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let file = scope.file(db);
        let module = parsed_module(db, file).load(db);

        match self.anchor(db) {
            DynamicDataclassAnchor::Definition(definition) => definition
                .kind(db)
                .value(&module)
                .expect("DynamicDataclassAnchor::Definition should only be used for assignments")
                .range(),
            DynamicDataclassAnchor::ScopeOffset { offset, .. } => {
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("anchor should not be NodeIndex::NONE");
                let absolute_index = NodeIndex::from(anchor_u32 + *offset);
                let node: &ast::ExprCall = module
                    .get_by_index(absolute_index)
                    .try_into()
                    .expect("scope offset should point to ExprCall");
                node.range()
            }
        }
    }

    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    fn spec(self, db: &'db dyn Db) -> DataclassSpec<'db> {
        match self.anchor(db) {
            DynamicDataclassAnchor::Definition(definition) => {
                deferred_functional_dataclass_spec(db, *definition)
            }
            DynamicDataclassAnchor::ScopeOffset { spec, .. } => *spec,
        }
    }

    pub(crate) fn fields(self, db: &'db dyn Db) -> &'db [DataclassFieldSpec<'db>] {
        self.spec(db).fields(db)
    }

    pub(crate) fn has_known_fields(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_fields(db)
    }

    pub(crate) fn bases(self, db: &'db dyn Db) -> &'db [ClassBase<'db>] {
        self.spec(db).bases(db)
    }

    fn field_info_from_spec(
        field: &DataclassFieldSpec<'db>,
        kw_only_default: bool,
    ) -> DataclassFieldInfo<'db> {
        DataclassFieldInfo {
            name: field.alias.clone().unwrap_or_else(|| field.name.clone()),
            ty: field.ty,
            default_ty: field.default_ty,
            init: field.init,
            kw_only: field.kw_only.unwrap_or(kw_only_default),
            converter: field.converter,
        }
    }

    fn fields_for_synthesis(self, db: &'db dyn Db) -> Option<Vec<DataclassFieldInfo<'db>>> {
        let mut fields = FxIndexMap::default();

        for base in self.iter_mro(db).rev() {
            let Some(class) = base.into_class() else {
                continue;
            };
            let (class_literal, specialization) = class.class_literal_and_specialization(db);

            match class_literal {
                ClassLiteral::DynamicDataclass(dataclass) => {
                    if !dataclass.has_known_fields(db) {
                        return None;
                    }

                    let kw_only_default = dataclass
                        .dataclass_params(db)
                        .flags(db)
                        .contains(DataclassFlags::KW_ONLY);
                    for field in dataclass.fields(db) {
                        if field.class_var {
                            continue;
                        }
                        fields.insert(
                            field.name.clone(),
                            Self::field_info_from_spec(field, kw_only_default),
                        );
                    }
                }
                ClassLiteral::Static(static_class) => {
                    let Some(field_policy @ CodeGeneratorKind::DataclassLike(_)) =
                        CodeGeneratorKind::from_class(db, class_literal, specialization)
                    else {
                        continue;
                    };

                    for (field_name, field) in
                        static_class.own_fields(db, specialization, field_policy)
                    {
                        if field.is_kw_only_sentinel(db) {
                            continue;
                        }

                        let FieldKind::Dataclass {
                            default_ty,
                            init,
                            kw_only,
                            alias,
                            converter,
                            ..
                        } = &field.kind
                        else {
                            continue;
                        };

                        fields.insert(
                            field_name.clone(),
                            DataclassFieldInfo {
                                name: alias
                                    .as_ref()
                                    .map(|alias| Name::new(alias.as_ref()))
                                    .unwrap_or_else(|| field_name.clone()),
                                ty: field.declared_ty,
                                default_ty: *default_ty,
                                init: *init,
                                kw_only: kw_only.unwrap_or(false),
                                converter: *converter,
                            },
                        );
                    }
                }
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => {}
            }
        }

        Some(fields.into_values().collect())
    }

    pub(super) fn converter_input_type_for_field(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<Type<'db>> {
        self.iter_mro(db).find_map(|base| {
            let class = base.into_class()?;
            let (class_literal, specialization) = class.class_literal_and_specialization(db);
            match class_literal {
                ClassLiteral::DynamicDataclass(dataclass) => dataclass
                    .fields(db)
                    .iter()
                    .find(|field| field.name == name)
                    .and_then(|field| field.converter.map(|(input_ty, _)| input_ty)),
                ClassLiteral::Static(static_class) => static_class
                    .converter_input_type_for_field(db, name)
                    .map(|ty| ty.apply_optional_specialization(db, specialization)),
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => None,
            }
        })
    }

    #[salsa::tracked(
        returns(ref),
        heap_size = ruff_memory_usage::heap_size,
        cycle_initial = dynamic_dataclass_mro_cycle_initial
    )]
    pub(crate) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, DynamicMroError<'db>> {
        Mro::of_dynamic(db, self.into())
    }

    pub(crate) fn iter_mro(self, db: &'db dyn Db) -> MroIterator<'db> {
        MroIterator::new(db, ClassLiteral::DynamicDataclass(self), None)
    }

    pub(crate) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<Type<'db>, DynamicMetaclassConflict<'db>> {
        let bases = self.bases(db);

        if !bases.is_empty() && self.try_mro(db).is_err() {
            return Ok(SubclassOfType::subclass_of_unknown());
        }

        dynamic_metaclass_from_bases(db, bases)
    }

    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.try_metaclass(db)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        for field in self.fields(db) {
            if field.name.as_str() == name && !field.init_only && !field.class_var {
                return Member::definitely_declared(field.ty);
            }
        }

        if !self.has_known_fields(db) {
            return Member::definitely_declared(Type::any());
        }

        Member::unbound()
    }

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match MroLookup::new(db, self.iter_mro(db)).instance_member(name) {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => KnownClass::TypedDictFallback
                .to_instance(db)
                .instance_member(db, name),
        }
    }

    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let result = MroLookup::new(db, self.iter_mro(db)).class_member(name, policy, None, false);

        let result = match result {
            ClassMemberResult::Done(result) => result.finalize(db),
            ClassMemberResult::TypedDict => KnownClass::TypedDictFallback
                .to_class_literal(db)
                .find_name_in_mro_with_policy(db, name, policy)
                .expect("Will return Some() when called on class literal"),
        };

        if !self.has_known_fields(db) && result.place.is_undefined() {
            return Place::bound(Type::any()).into();
        }

        result
    }

    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        if matches!(name, "__dataclass_fields__" | "__dataclass_params__")
            && let Some(ty) = self.synthesized_class_member(db, name)
        {
            return Member {
                inner: Place::declared(ty).with_qualifiers(TypeQualifiers::CLASS_VAR),
            };
        }

        if let Some(field) = self
            .fields(db)
            .iter()
            .find(|field| field.name.as_str() == name)
        {
            return field
                .class_default_ty
                .map(Member::definitely_declared)
                .unwrap_or_else(Member::unbound);
        }

        if let Some(ty) = self.namespace_member(db, name) {
            return Member::definitely_declared(ty);
        }

        if let Some(ty) = self.synthesized_class_member(db, name) {
            return Member::definitely_declared(ty);
        }

        self.has_dynamic_namespace(db)
            .then(Type::unknown)
            .map(Member::definitely_declared)
            .unwrap_or_default()
    }

    fn namespace_member(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        self.members(db)
            .iter()
            .find_map(|(member_name, ty)| (name == member_name).then_some(*ty))
    }

    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        const ORDERING_METHODS: &[&str] = &["__lt__", "__le__", "__gt__", "__ge__"];
        ORDERING_METHODS
            .iter()
            .any(|name| !self.own_class_member(db, name).is_undefined())
    }

    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        Self::new(
            db,
            self.name(db).clone(),
            dataclass_params.unwrap_or_else(|| self.dataclass_params(db)),
            self.anchor(db).clone(),
            self.members(db),
            self.has_dynamic_namespace(db),
        )
    }

    fn synthesized_class_member(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        let instance_ty = self.to_instance(db);
        let params = self.dataclass_params(db);
        let flags = params.flags(db);

        if !self.has_known_fields(db) {
            match name {
                "__new__" | "__init__" => {
                    let signature = Signature::new(Parameters::gradual_form(), instance_ty);
                    return Some(Type::function_like_callable(db, signature));
                }
                _ => {}
            }
        }

        if name == "__weakref__"
            && Program::get(db).python_version(db) >= PythonVersion::PY311
            && flags.contains(DataclassFlags::WEAKREF_SLOT)
            && flags.contains(DataclassFlags::SLOTS)
        {
            return Some(UnionType::from_elements(db, [Type::any(), Type::none(db)]));
        }

        let Some(fields) = self.fields_for_synthesis(db) else {
            match name {
                "__new__" | "__init__" => {
                    let signature = Signature::new(Parameters::gradual_form(), instance_ty);
                    return Some(Type::function_like_callable(db, signature));
                }
                _ => return None,
            }
        };

        synthesize_dataclass_class_member(db, name, instance_ty, flags, fields.into_iter())
    }
}
