use ruff_db::parsed::parsed_module;
use ruff_python_ast::{Keyword, name::Name};
use rustc_hash::FxHashSet;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::definition::{Definition, DefinitionKind};

use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place, Provenance, known_module_symbol};
use crate::types::class::CodeGeneratorKind;
use crate::types::member::class_member;
use crate::types::{
    ClassBase, DataclassTransformerParams, KnownClass, KnownInstanceType, KnownUnion, Parameter,
    StaticClassLiteral, Type, UnionType, definition_expression_type,
};

/// Metadata that controls Pydantic-specific model synthesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct ModelMetadata<'db> {
    // TODO: We may want to remove this field and model Pydantic behavior more explicitly.
    // (since this information is static and doesn't vary between Pydantic models). To do
    // this, we could only retain field specifier information and expose it through a new
    // `CodeGeneratorKind` method. Maybe we could even skip the field specifiers as well.
    transformer_params: DataclassTransformerParams<'db>,
    config: ModelConfig,
}

impl<'db> ModelMetadata<'db> {
    pub(in crate::types) fn from_class(
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
        transformer_params: DataclassTransformerParams<'db>,
    ) -> Self {
        Self {
            transformer_params,
            config: model_config(db, class),
        }
    }

    pub(in crate::types) const fn transformer_params(self) -> DataclassTransformerParams<'db> {
        self.transformer_params
    }

    const fn accepts_extra(self) -> bool {
        !matches!(self.config.extra, Some(ExtraBehavior::Forbid))
    }

    pub(in crate::types) const fn validates_by_alias(self) -> bool {
        self.config.validate_by_alias.enabled_or(true)
    }

    pub(in crate::types) const fn validates_by_name(self) -> bool {
        let validate_by_name = self.config.validate_by_name;
        // If `validate_by_alias=False` is set without specifying `validate_by_name`, Pydantic
        // implicitly enables validation by name.
        if matches!(validate_by_name, ConfigBoolean::Unspecified)
            && matches!(self.config.validate_by_alias, ConfigBoolean::Disabled)
        {
            true
        } else {
            validate_by_name.enabled_or(false)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ModelConfig {
    /// The `extra` configuration controls whether the synthesized constructor accepts keyword
    /// arguments that do not correspond to declared model fields.
    extra: Option<ExtraBehavior>,
    /// The `strict` configuration controls whether constructor parameters accept values that
    /// Pydantic can coerce to the declared field type.
    strict: ConfigBoolean,
    /// Whether fields with aliases can be initialized by their alias.
    validate_by_alias: ConfigBoolean,
    /// Whether fields with aliases can be initialized by their field name.
    validate_by_name: ConfigBoolean,
}

impl ModelConfig {
    const fn unknown() -> Self {
        Self {
            extra: Some(ExtraBehavior::Unknown),
            strict: ConfigBoolean::Unknown,
            validate_by_alias: ConfigBoolean::Unknown,
            validate_by_name: ConfigBoolean::Unknown,
        }
    }

    /// Merge `other` into this config, with values from `other` taking precedence.
    fn merge(&mut self, other: Self) {
        self.extra = other.extra.or(self.extra);
        self.strict = other.strict.or(self.strict);
        self.validate_by_alias = other.validate_by_alias.or(self.validate_by_alias);
        self.validate_by_name = other.validate_by_name.or(self.validate_by_name);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum ExtraBehavior {
    Allow,
    Forbid,
    Ignore,
    Unknown,
}

impl ExtraBehavior {
    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("allow") => Self::Allow,
            Some("forbid") => Self::Forbid,
            Some("ignore") => Self::Ignore,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum ConfigBoolean {
    /// No value was specified at this precedence level, so a lower-precedence value can apply.
    #[default]
    Unspecified,
    /// The setting was explicitly disabled.
    Disabled,
    /// The setting was explicitly enabled.
    Enabled,
    /// A value was specified, but it could not be resolved statically.
    Unknown,
}

impl ConfigBoolean {
    const fn or(self, other: Self) -> Self {
        if matches!(self, Self::Unspecified) {
            other
        } else {
            self
        }
    }

    /// Resolve a boolean configuration value from its inferred type.
    pub(in crate::types) fn from_type(value: Type<'_>) -> Self {
        if value == Type::bool_literal(true) {
            Self::Enabled
        } else if value == Type::bool_literal(false) {
            Self::Disabled
        } else {
            Self::Unknown
        }
    }

    /// Resolve this value to a boolean, using `default` when it is unspecified.
    ///
    /// An unknown value resolves to `true`.
    const fn enabled_or(self, default: bool) -> bool {
        match self {
            Self::Unspecified => default,
            Self::Disabled => false,
            Self::Enabled | Self::Unknown => true,
        }
    }
}

fn config_boolean(
    db: &dyn Db,
    definition: Definition<'_>,
    keyword: Option<&Keyword>,
) -> ConfigBoolean {
    keyword.map_or(ConfigBoolean::Unspecified, |keyword| {
        ConfigBoolean::from_type(definition_expression_type(db, definition, &keyword.value))
    })
}

pub(in crate::types) fn is_model(db: &dyn Db, class: StaticClassLiteral<'_>) -> bool {
    class
        .iter_mro(db, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticBaseModel))
}

/// Return `true` if fields in `class` are keyword-only constructor parameters.
///
/// Pydantic model fields are generally keyword-only, but a root model's `root` field can also be
/// passed positionally.
pub(in crate::types) fn constructor_fields_are_keyword_only(
    db: &dyn Db,
    class: StaticClassLiteral<'_>,
) -> bool {
    !is_root_model(db, class)
}

fn is_root_model(db: &dyn Db, class: StaticClassLiteral<'_>) -> bool {
    class
        .iter_mro(db, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticRootModel))
}

/// Return `true` if fields are optional constructor parameters for `class`.
///
/// A settings model can populate any field from environment variables or another configured
/// settings source, so no field value is necessarily required at the call site.
pub(in crate::types) fn constructor_fields_are_optional(
    db: &dyn Db,
    class: StaticClassLiteral<'_>,
) -> bool {
    class
        .iter_mro(db, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticBaseSettings))
}

#[salsa::tracked(
    cycle_initial=|_, _, _| ModelConfig::unknown(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn model_config<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> ModelConfig {
    let mut config = ModelConfig::default();

    // Pydantic merges the effective config from each direct base from left to right. A later base
    // therefore takes precedence over an earlier base.
    for base in class.explicit_bases(db) {
        let Some(base) = base.to_class_type(db) else {
            config = ModelConfig::unknown();
            continue;
        };
        let base_literal = base.class_literal(db);
        let Some(base) = base_literal.as_static() else {
            config.merge(ModelConfig::unknown());
            continue;
        };

        if is_model(db, base) {
            config.merge(model_config(db, base));
        } else if let Some(base_config) = inherited_model_config(db, base) {
            config.merge(base_config);
        }
    }

    if let Some(own_config) = own_model_config(db, class) {
        config.merge(own_config);
    }
    config.merge(class_keyword_config(db, class));
    config
}

fn inherited_model_config(db: &dyn Db, class: StaticClassLiteral<'_>) -> Option<ModelConfig> {
    for base in class.iter_mro(db, None).filter_map(ClassBase::into_class) {
        let Some((base, _)) = base.static_class_literal(db) else {
            return Some(ModelConfig::unknown());
        };
        if let Some(config) = own_model_config(db, base) {
            return Some(config);
        }
    }
    None
}

fn own_model_config(db: &dyn Db, class: StaticClassLiteral<'_>) -> Option<ModelConfig> {
    let model_config = class_member(db, class.body_scope(db), "model_config")
        .inner
        .place;
    let Place::Defined(DefinedPlace {
        definedness: Definedness::AlwaysDefined,
        provenance: Provenance::SingleDefinition(definition),
        ..
    }) = model_config
    else {
        return if model_config.is_undefined() {
            None
        } else {
            Some(ModelConfig::unknown())
        };
    };

    let module = parsed_module(db, class.file(db)).load(db);
    let kind = definition.kind(db);
    let value = match &kind {
        DefinitionKind::Assignment(assignment) => assignment.value(&module),
        DefinitionKind::AnnotatedAssignment(assignment) => {
            let Some(value) = assignment.value(&module) else {
                return Some(ModelConfig::default());
            };
            value
        }
        _ => {
            return Some(ModelConfig::unknown());
        }
    };

    let Some(call) = value.as_call_expr() else {
        return Some(ModelConfig::unknown());
    };
    let callee = definition_expression_type(db, definition, &call.func);
    if !callee
        .as_class_literal()
        .is_some_and(|class| class.is_known(db, KnownClass::PydanticConfigDict))
    {
        return Some(ModelConfig::unknown());
    }

    if call
        .arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return Some(ModelConfig::unknown());
    }

    let extra = call.arguments.find_keyword("extra").map(|extra| {
        let extra = definition_expression_type(db, definition, &extra.value)
            .as_string_literal()
            .map(|literal| literal.value(db));
        ExtraBehavior::from_value(extra)
    });
    let strict = config_boolean(db, definition, call.arguments.find_keyword("strict"));
    let validate_by_alias = config_boolean(
        db,
        definition,
        call.arguments.find_keyword("validate_by_alias"),
    );
    let validate_by_name = config_boolean(
        db,
        definition,
        call.arguments.find_keyword("validate_by_name"),
    );

    Some(ModelConfig {
        extra,
        strict,
        validate_by_alias,
        validate_by_name,
    })
}

fn class_keyword_config(db: &dyn Db, class: StaticClassLiteral<'_>) -> ModelConfig {
    let definition = class.definition(db);
    let module = parsed_module(db, class.file(db)).load(db);
    let kind = definition.kind(db);
    let Some(class) = kind.as_class() else {
        return ModelConfig::default();
    };
    let class_node = class.node(&module);
    let Some(arguments) = class_node.arguments.as_ref() else {
        return ModelConfig::default();
    };
    if arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return ModelConfig::unknown();
    }
    let extra = arguments.find_keyword("extra").map(|extra| {
        let extra = definition_expression_type(db, definition, &extra.value)
            .as_string_literal()
            .map(|literal| literal.value(db));
        ExtraBehavior::from_value(extra)
    });
    let strict = config_boolean(db, definition, arguments.find_keyword("strict"));
    let validate_by_alias =
        config_boolean(db, definition, arguments.find_keyword("validate_by_alias"));
    let validate_by_name =
        config_boolean(db, definition, arguments.find_keyword("validate_by_name"));

    ModelConfig {
        extra,
        strict,
        validate_by_alias,
        validate_by_name,
    }
}

/// Return the input type accepted by a Pydantic field's synthesized constructor parameter.
pub(in crate::types) fn constructor_parameter_type<'db>(
    db: &'db dyn Db,
    field_type: Type<'db>,
    field_strict: ConfigBoolean,
    metadata: ModelMetadata<'db>,
) -> Type<'db> {
    if field_strict.or(metadata.config.strict) == ConfigBoolean::Enabled {
        return field_type;
    }

    lax_input_type(db, field_type)
}

/// Return the documented Python input type accepted by Pydantic for `field_type` in lax mode.
fn lax_input_type<'db>(db: &'db dyn Db, field_type: Type<'db>) -> Type<'db> {
    lax_input_type_impl(db, field_type, &mut FxHashSet::default())
}

fn lax_input_type_impl<'db>(
    db: &'db dyn Db,
    field_type: Type<'db>,
    expanding_types: &mut FxHashSet<Type<'db>>,
) -> Type<'db> {
    if field_type.is_none(db) || matches!(field_type, Type::LiteralValue(_) | Type::SubclassOf(_)) {
        return field_type;
    }

    if let Type::TypeAlias(alias) = field_type {
        if !expanding_types.insert(field_type) {
            return Type::any();
        }
        let result = lax_input_type_impl(db, alias.value_type(db), expanding_types);
        expanding_types.remove(&field_type);
        return result;
    }

    if field_type.as_union().and_then(|union| union.known(db)) == Some(KnownUnion::Float) {
        return lax_alias(db, "LaxFloat");
    }

    if let Type::Union(union) = field_type {
        return UnionType::from_elements_leave_aliases(
            db,
            union
                .elements(db)
                .iter()
                .map(|element| lax_input_type_impl(db, *element, expanding_types)),
        );
    }

    if let Some(input_type) = root_model_input_type(db, field_type, expanding_types) {
        return input_type;
    }

    let known_class = field_type
        .nominal_class(db)
        .and_then(|class| class.known(db));

    if matches!(
        known_class,
        Some(
            KnownClass::List
                | KnownClass::Set
                | KnownClass::FrozenSet
                | KnownClass::Deque
                | KnownClass::Sequence
                | KnownClass::Iterable
                | KnownClass::Tuple
        )
    ) {
        let Ok(elements) = field_type.try_iterate(db) else {
            return Type::any();
        };
        let element_type =
            lax_input_type_impl(db, elements.homogeneous_element_type(db), expanding_types);
        return KnownClass::Iterable.to_specialized_instance(db, &[element_type]);
    }

    if matches!(known_class, Some(KnownClass::Dict | KnownClass::Mapping)) {
        let Some(specialization) =
            known_class.and_then(|known_class| field_type.known_specialization(db, known_class))
        else {
            return Type::any();
        };
        let [key_type, value_type] = specialization.types(db) else {
            return Type::any();
        };
        let value_type = lax_input_type_impl(db, *value_type, expanding_types);
        return KnownClass::Mapping.to_specialized_instance(db, &[*key_type, value_type]);
    }

    let builtin_alias = match known_class {
        Some(KnownClass::Bool) => Some("LaxBool"),
        Some(KnownClass::Bytes) => Some("LaxBytes"),
        Some(KnownClass::Int) => Some("LaxInt"),
        Some(KnownClass::Str) => Some("LaxStr"),
        Some(KnownClass::Path) => Some("LaxPath"),
        _ => None,
    };
    if let Some(alias) = builtin_alias {
        return lax_alias(db, alias);
    }

    let Some((module, symbol, class)) = instance_symbol(db, field_type) else {
        return Type::any();
    };
    let symbol_alias = match (module, symbol) {
        (KnownModule::Datetime, "date") => Some("LaxDate"),
        (KnownModule::Datetime, "datetime") => Some("LaxDatetime"),
        (KnownModule::Datetime, "time") => Some("LaxTime"),
        (KnownModule::Datetime, "timedelta") => Some("LaxTimedelta"),
        (KnownModule::Decimal, "Decimal") => Some("LaxDecimal"),
        (KnownModule::Uuid, "UUID") => Some("LaxUUID"),
        (KnownModule::Ipaddress, "IPv4Address") => Some("LaxIPv4Address"),
        (KnownModule::Ipaddress, "IPv4Interface") => Some("LaxIPv4Interface"),
        (KnownModule::Ipaddress, "IPv4Network") => Some("LaxIPv4Network"),
        (KnownModule::Ipaddress, "IPv6Address") => Some("LaxIPv6Address"),
        (KnownModule::Ipaddress, "IPv6Interface") => Some("LaxIPv6Interface"),
        (KnownModule::Ipaddress, "IPv6Network") => Some("LaxIPv6Network"),
        (KnownModule::PydanticTypes, "ByteSize") => Some("LaxByteSize"),
        _ => None,
    };
    if let Some(alias) = symbol_alias {
        return lax_alias(db, alias);
    }

    let alias = if (module, symbol) == (KnownModule::Re, "Pattern") {
        let Some(specialization) = field_type.specialization_of(db, class) else {
            return Type::any();
        };
        let [pattern_type] = specialization.types(db) else {
            return Type::any();
        };
        if pattern_type
            .nominal_class(db)
            .is_some_and(|class| class.is_known(db, KnownClass::Str))
        {
            "LaxStrPattern"
        } else if pattern_type
            .nominal_class(db)
            .is_some_and(|class| class.is_known(db, KnownClass::Bytes))
        {
            "LaxBytesPattern"
        } else {
            return Type::any();
        }
    } else {
        return Type::any();
    };

    lax_alias(db, alias)
}

/// Return the input type accepted for a Pydantic root model field.
///
/// This builds a union of the root model instance and its transformed raw root type. For example,
/// a field annotated with an `IntList` derived from `RootModel[list[int]]` accepts both an
/// `IntList` instance and an `Iterable[LaxInt]`.
fn root_model_input_type<'db>(
    db: &'db dyn Db,
    field_type: Type<'db>,
    expanding_types: &mut FxHashSet<Type<'db>>,
) -> Option<Type<'db>> {
    let (class, specialization) = field_type.nominal_class(db)?.static_class_literal(db)?;
    if !is_root_model(db, class) {
        return None;
    }

    let Some(field_policy @ CodeGeneratorKind::Pydantic(_)) =
        CodeGeneratorKind::from_class(db, class.into())
    else {
        return Some(Type::any());
    };
    let Some(root_field) = class.fields(db, specialization, field_policy).get("root") else {
        return Some(Type::any());
    };

    if !expanding_types.insert(field_type) {
        return Some(Type::any());
    }
    let root_input_type = lax_input_type_impl(db, root_field.declared_ty, expanding_types);

    expanding_types.remove(&field_type);
    Some(UnionType::from_two_elements(
        db,
        field_type,
        root_input_type,
    ))
}

/// Return the known module, name, and class literal for an instance's nominal class.
fn instance_symbol<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<(KnownModule, &'db str, StaticClassLiteral<'db>)> {
    let class = ty.nominal_class(db)?.class_literal(db).as_static()?;
    let module = file_to_module(db, class.file(db))?.known(db)?;
    Some((module, class.name(db).as_str(), class))
}

/// Return a lax-input alias like `LaxInt` from `ty_extensions.pydantic`.
fn lax_alias<'db>(db: &'db dyn Db, name: &str) -> Type<'db> {
    match known_module_symbol(db, KnownModule::TyExtensionsPydantic, name)
        .place
        .ignore_possibly_undefined()
    {
        Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(alias))) => {
            Type::TypeAlias(alias)
        }
        _ => Type::any(),
    }
}

/// Return `true` if `class` should accept extra keywords in its synthesized constructor.
pub(in crate::types) fn model_init_accepts_extra(
    db: &dyn Db,
    class: StaticClassLiteral<'_>,
    metadata: ModelMetadata<'_>,
) -> bool {
    if !metadata.accepts_extra() {
        return false;
    }

    for base in class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.static_class_literal(db))
        .map(|(base, _)| base)
    {
        if base.is_known(db, KnownClass::PydanticBaseModel) {
            return true;
        }

        if !class_member(db, base.body_scope(db), "__init__").is_undefined() {
            return false;
        }
    }

    false
}

/// Create the catch-all keyword parameter for a Pydantic model constructor.
///
/// Start with `extra` and append underscores until the name does not collide with a model field.
pub(in crate::types) fn extra_parameter<'db>(parameters: &[Parameter<'db>]) -> Parameter<'db> {
    let mut name = String::from("extra");

    while parameters
        .iter()
        .filter_map(Parameter::name)
        .any(|parameter_name| parameter_name.as_str() == name)
    {
        name.push('_');
    }

    Parameter::keyword_variadic(Name::new(name)).with_annotated_type(Type::any())
}
