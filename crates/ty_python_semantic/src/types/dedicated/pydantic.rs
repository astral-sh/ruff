use crate::SemanticContext;
use char_str::CharStr;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{ArgOrKeyword, Arguments, Expr, ExprCall, ExprDict, Keyword, name::Name};
use rustc_hash::FxHashSet;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    place_table, use_def_map,
};

use crate::diagnostic::format_enumeration;
use crate::place::{DefinedPlace, Definedness, Place, Provenance, known_module_symbol};
use crate::reachability::DeclarationsIteratorExtension;
use crate::types::call::Bindings;
use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::PYDANTIC_DISCARDED_EXTRA_ARGUMENT;
use crate::types::ide_support::{ImportAliasResolution, definitions_for_name};
use crate::types::infer::function_known_decorators;
use crate::types::known_instance::FieldInstance;
use crate::types::member::class_member;
use crate::types::special_form::SpecialFormType;
use crate::types::{
    ClassBase, ClassType, DataclassTransformerParams, FunctionType, KnownClass, KnownFunction,
    KnownInstanceType, KnownUnion, Parameter, Specialization, StaticClassLiteral, Type, UnionType,
    definition_expression_type,
};
use crate::{Db, SemanticModel};

/// Metadata that controls Pydantic-specific model synthesis.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct ModelMetadata<'db> {
    #[returns(deref)]
    pub(in crate::types) field_specifiers: Box<[Type<'db>]>,
    #[returns(copy)]
    config: ModelConfig,
}

impl get_size2::GetSize for ModelMetadata<'_> {}

impl<'db> ModelMetadata<'db> {
    pub(in crate::types) fn from_class(
        ctx: &SemanticContext<'db>,
        class: StaticClassLiteral<'db>,
        transformer_params: DataclassTransformerParams<'db>,
    ) -> Self {
        let db = ctx.db();
        Self::new(
            db,
            transformer_params.field_specifiers(db),
            model_config(ctx, class),
        )
    }

    fn accepts_extra(self, db: &'db dyn Db) -> bool {
        !matches!(self.config(db).extra, Some(ExtraBehavior::Forbid))
    }

    fn discards_extra(self, db: &'db dyn Db) -> bool {
        matches!(self.config(db).extra, None | Some(ExtraBehavior::Ignore))
    }

    pub(in crate::types) fn validates_by_alias(self, db: &'db dyn Db) -> bool {
        let (validate_by_alias, _) = self.config(db).validation_config();
        validate_by_alias.enabled_or(true)
    }

    pub(in crate::types) fn validates_by_name(self, db: &'db dyn Db) -> bool {
        let (_, validate_by_name) = self.config(db).validation_config();
        validate_by_name.enabled_or(false)
    }

    pub(in crate::types) fn is_frozen(self, db: &'db dyn Db) -> bool {
        self.config(db).frozen.is_enabled()
    }
}

/// Pydantic-specific metadata resolved from a field's annotation (via `Annotation`)
/// and right-hand side `Field(...)` specifier.
///
/// For example:
/// ```py
/// class Model(BaseModel):
///     value: Annotated[int, Strict()] = Field(default=0)
/// ```
pub(in crate::types) struct FieldMetadata<'db> {
    pub(in crate::types) default_ty: Option<Type<'db>>,
    pub(in crate::types) init: bool,
    pub(in crate::types) alias: Option<Box<str>>,
    pub(in crate::types) strict: ConfigBoolean,
}

impl Default for FieldMetadata<'_> {
    fn default() -> Self {
        Self {
            default_ty: None,
            init: true,
            alias: None,
            strict: ConfigBoolean::Unspecified,
        }
    }
}

impl<'db> FieldMetadata<'db> {
    /// Collect Pydantic field metadata from the right-hand side of a field's assignment.
    ///
    /// For example, collect the default value and alias from the following field assignment:
    /// ```py
    /// field: int = Field(default=0, alias="field_alias")
    /// ```
    fn collect_from_rhs_type(
        &mut self,
        ctx: &SemanticContext<'db>,
        rhs_type: Option<Type<'db>>,
        specialization: Option<Specialization<'db>>,
    ) {
        match rhs_type {
            Some(Type::KnownInstance(KnownInstanceType::Field(field))) => {
                self.merge_field(ctx, field, specialization);
            }
            Some(rhs_type) => self.default_ty = Some(rhs_type),
            None => {}
        }
    }

    /// Collect Pydantic field metadata from a field's annotation.
    ///
    /// For example, collect the strictness metadata from the following field annotation:
    /// ```py
    /// field: Annotated[int, Strict()]
    /// ```
    ///
    /// This method also handles the case where the annotation is an alias to an `Annotated` type, such as:
    /// ```py
    /// field: StrictInt
    /// ```
    /// where `StrictInt` is defined as `StrictInt = Annotated[int, Strict()]`.
    fn collect_from_annotation(
        &mut self,
        ctx: &SemanticContext<'db>,
        definition: Definition<'db>,
        specialization: Option<Specialization<'db>>,
    ) {
        let db = ctx.db();
        let module = parsed_module(db, definition.python_file(db)).load(db);
        let DefinitionKind::AnnotatedAssignment(assignment) = definition.kind(db) else {
            return;
        };
        let annotation = assignment.annotation(&module);

        if self.collect_from_annotated(ctx, definition, annotation, specialization) {
            return;
        }

        let name = match annotation {
            Expr::Name(name) => name,
            Expr::Subscript(subscript) => {
                let Expr::Name(name) = subscript.value.as_ref() else {
                    return;
                };
                name
            }
            _ => return,
        };

        // The following part is unfortunate. Pydantic defines `StrictInt` and the other aliases
        // using `StrictInt = Annotated[int, Strict()]`. Since we don't retain the `Annotated`
        // metadata, we need to follow the alias back to its definition and parse the metadata
        // from there.
        let model = SemanticModel::new(db, definition.python_file(db));
        let Some(alias_definition) = definitions_for_name(
            &model,
            name.id.as_str(),
            name.into(),
            ImportAliasResolution::ResolveAliases,
        )
        .into_iter()
        .find_map(|resolved| resolved.definition()) else {
            return;
        };

        let module = parsed_module(db, alias_definition.python_file(db)).load(db);
        let kind = alias_definition.kind(db);
        let value = match &kind {
            DefinitionKind::Assignment(assignment) => assignment.value(&module),
            DefinitionKind::AnnotatedAssignment(assignment) => {
                let Some(value) = assignment.value(&module) else {
                    return;
                };
                value
            }
            _ => return,
        };

        self.collect_from_annotated(ctx, alias_definition, value, specialization);
    }

    /// Collect Pydantic field metadata from the `Annotated` part of a field's annotation.
    fn collect_from_annotated(
        &mut self,
        ctx: &SemanticContext<'db>,
        definition: Definition<'db>,
        annotation: &Expr,
        specialization: Option<Specialization<'db>>,
    ) -> bool {
        let db = ctx.db();
        let Some(subscript) = annotation.as_subscript_expr() else {
            return false;
        };
        if definition_expression_type(ctx, definition, &subscript.value)
            != Type::SpecialForm(SpecialFormType::Annotated)
        {
            return false;
        }
        let Some(arguments) = subscript
            .slice
            .as_tuple_expr()
            .and_then(|tuple| tuple.elts.get(1..))
        else {
            return false;
        };

        for metadata in arguments {
            let Some(call) = metadata.as_call_expr() else {
                continue;
            };
            let callee = definition_expression_type(ctx, definition, &call.func);

            if callee
                .as_class_literal()
                .is_some_and(|class| class.is_known(db, KnownClass::PydanticStrict))
            {
                let strict = call.arguments.find_argument_value("strict", 0).map_or(
                    ConfigBoolean::Enabled,
                    |strict| {
                        ConfigBoolean::from_type(definition_expression_type(
                            ctx, definition, strict,
                        ))
                    },
                );
                self.merge_strict(strict);
            } else if matches!(
                callee,
                Type::FunctionLiteral(function)
                    if function.is_known(db, KnownFunction::PydanticField)
            ) {
                let field_type = definition_expression_type(ctx, definition, metadata);
                if let Type::KnownInstance(KnownInstanceType::Field(field)) = field_type {
                    self.merge_field(ctx, field, specialization);
                } else {
                    self.merge_field_call(ctx, definition, call, field_type, specialization);
                }
            }
        }

        true
    }

    fn merge_strict(&mut self, strict: ConfigBoolean) {
        self.strict = strict;
    }

    fn merge_field(
        &mut self,
        ctx: &SemanticContext<'db>,
        field: FieldInstance<'db>,
        specialization: Option<Specialization<'db>>,
    ) {
        let db = ctx.db();
        if let Some(default_type) = field.default_type(db) {
            self.default_ty = Some(default_type.apply_optional_specialization(ctx, specialization));
        }
        self.init &= field.init(db);
        if let Some(alias) = field.alias(db) {
            self.alias = Some(alias.clone());
        }
        if !field.strict(db).is_unspecified() {
            self.strict = field.strict(db);
        }
    }

    fn merge_field_call(
        &mut self,
        ctx: &SemanticContext<'db>,
        definition: Definition<'db>,
        call: &ExprCall,
        call_type: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) {
        let db = ctx.db();
        if let Some(default) = call.arguments.find_argument_value("default", 0) {
            let default_type = definition_expression_type(ctx, definition, default);
            if !default_type.is_instance_of(db, KnownClass::EllipsisType) {
                self.default_ty =
                    Some(default_type.apply_optional_specialization(ctx, specialization));
            }
        } else if call.arguments.find_keyword("default_factory").is_some() {
            self.default_ty = Some(call_type.apply_optional_specialization(ctx, specialization));
        }

        if let Some(init) = call.arguments.find_keyword("init") {
            let init = definition_expression_type(ctx, definition, &init.value);
            self.init &= !init.bool(ctx).is_always_false();
        }

        if let Some(alias) = call
            .arguments
            .find_keyword("validation_alias")
            .or_else(|| call.arguments.find_keyword("alias"))
        {
            self.alias = definition_expression_type(ctx, definition, &alias.value)
                .as_string_literal()
                .map(|literal| Box::from(literal.value(db)));
        }

        if let Some(strict) = call.arguments.find_keyword("strict") {
            let strict = definition_expression_type(ctx, definition, &strict.value);
            if !strict.is_none(db) {
                self.merge_strict(ConfigBoolean::from_type(strict));
            }
        }
    }
}

/// Resolve a Pydantic field's metadata from its annotation and right-hand side.
pub(in crate::types) fn field_metadata<'db>(
    ctx: &SemanticContext<'db>,
    definition: Option<Definition<'db>>,
    rhs_type: Option<Type<'db>>,
    specialization: Option<Specialization<'db>>,
) -> FieldMetadata<'db> {
    let mut metadata = FieldMetadata::default();
    if let Some(definition) = definition {
        metadata.collect_from_annotation(ctx, definition, specialization);
    }
    metadata.collect_from_rhs_type(ctx, rhs_type, specialization);
    metadata
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct ModelConfig {
    /// The `extra` configuration controls whether the synthesized constructor accepts keyword
    /// arguments that do not correspond to declared model fields.
    extra: Option<ExtraBehavior>,
    /// The `strict` configuration controls whether constructor parameters accept values that
    /// Pydantic can coerce to the declared field type.
    strict: ConfigBoolean,
    /// Whether model fields can be populated from attributes on arbitrary objects.
    from_attributes: ConfigBoolean,
    /// Whether assignments to fields on model instances are forbidden.
    frozen: ConfigBoolean,
    /// Whether fields with aliases can be initialized by their alias.
    validate_by_alias: ConfigBoolean,
    /// Whether fields with aliases can be initialized by their field name.
    validate_by_name: ConfigBoolean,
    /// The deprecated setting that enables validation by both alias and field name.
    populate_by_name: ConfigBoolean,
}

impl ModelConfig {
    const fn unknown() -> Self {
        Self {
            extra: Some(ExtraBehavior::Unknown),
            strict: ConfigBoolean::Unknown,
            from_attributes: ConfigBoolean::Unknown,
            frozen: ConfigBoolean::Unknown,
            validate_by_alias: ConfigBoolean::Unknown,
            validate_by_name: ConfigBoolean::Unknown,
            populate_by_name: ConfigBoolean::Unknown,
        }
    }

    /// Merge `other` into this config, with values from `other` taking precedence.
    fn merge(&mut self, other: Self) {
        self.extra = other.extra.or(self.extra);
        self.strict = other.strict.or(self.strict);
        self.from_attributes = other.from_attributes.or(self.from_attributes);
        self.frozen = other.frozen.or(self.frozen);
        self.validate_by_alias = other.validate_by_alias.or(self.validate_by_alias);
        self.validate_by_name = other.validate_by_name.or(self.validate_by_name);
        self.populate_by_name = other.populate_by_name.or(self.populate_by_name);
    }

    /// Resolve compatibility behavior after inherited and local configuration has been merged.
    fn validation_config(self) -> (ConfigBoolean, ConfigBoolean) {
        let mut validate_by_alias = self.validate_by_alias;
        let mut validate_by_name = self.validate_by_name;

        // `populate_by_name` enables validation by both alias and field name. The newer
        // `validate_by_name` setting takes precedence when both are specified.
        if validate_by_name.is_unspecified() && self.populate_by_name.is_specified() {
            validate_by_alias = ConfigBoolean::Enabled;
            validate_by_name = self.populate_by_name;
        }

        // If `validate_by_alias=False` is set without specifying `validate_by_name`, Pydantic
        // implicitly enables validation by name.
        if validate_by_alias.is_disabled() && validate_by_name.is_unspecified() {
            validate_by_name = ConfigBoolean::Enabled;
        }

        (validate_by_alias, validate_by_name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
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
    const fn is_unspecified(self) -> bool {
        matches!(self, Self::Unspecified)
    }

    const fn is_specified(self) -> bool {
        matches!(self, Self::Disabled | Self::Enabled | Self::Unknown)
    }

    const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }

    const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    const fn or(self, other: Self) -> Self {
        if self.is_unspecified() { other } else { self }
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
    ctx: &SemanticContext<'_>,
    definition: Definition<'_>,
    keyword: Option<&Keyword>,
) -> ConfigBoolean {
    keyword.map_or(ConfigBoolean::Unspecified, |keyword| {
        ConfigBoolean::from_type(definition_expression_type(ctx, definition, &keyword.value))
    })
}

pub(in crate::types) fn is_model(ctx: &SemanticContext<'_>, class: StaticClassLiteral<'_>) -> bool {
    let db = ctx.db();
    class
        .iter_mro(ctx, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticBaseModel))
}

/// Return whether a field specifier's `default` argument provides a default value.
///
/// Pydantic's `Field(...)` uses the ellipsis as a required-field sentinel, so it does not provide
/// a default value.
pub(in crate::types) fn field_provides_default(
    db: &dyn Db,
    function: FunctionType<'_>,
    default: Type<'_>,
) -> bool {
    !default.is_instance_of(db, KnownClass::EllipsisType)
        || !function.is_known(db, KnownFunction::PydanticField)
}

/// Return `true` if fields in `class` are keyword-only constructor parameters.
///
/// Pydantic model fields are generally keyword-only, but a root model's `root` field can also be
/// passed positionally.
pub(in crate::types) fn constructor_fields_are_keyword_only(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> bool {
    !is_root_model(ctx, class)
}

fn is_root_model<'db>(ctx: &SemanticContext<'db>, class: StaticClassLiteral<'db>) -> bool {
    let db = ctx.db();
    debug_assert_eq!(ctx.program(), class.program(db));
    is_root_model_inner(db, class)
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn is_root_model_inner<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    let ctx = SemanticContext::from_file(db, class.python_file(db));
    class
        .iter_mro(&ctx, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticRootModel))
}

/// Return `true` if fields are optional constructor parameters for `class`.
///
/// A settings model can populate any field from environment variables or another configured
/// settings source, so no field value is necessarily required at the call site.
pub(in crate::types) fn constructor_fields_are_optional(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> bool {
    let db = ctx.db();
    class
        .iter_mro(ctx, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticBaseSettings))
}

/// Add the specialized constructor parameters accepted by a settings model.
///
/// Pydantic settings models accept underscore-prefixed parameters that override values from
/// `model_config` for a single instantiation. These parameters are defined on
/// `BaseSettings.__init__`, so we reuse them instead of duplicating their names and types.
pub(in crate::types) fn extend_settings_constructor_parameters<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
    parameters: &mut Vec<Parameter<'db>>,
) {
    let db = ctx.db();
    let Some(base_settings) = class
        .iter_mro(ctx, None)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.static_class_literal(db))
        .map(|(base, _)| base)
        .find(|base| base.is_known(db, KnownClass::PydanticBaseSettings))
    else {
        return;
    };

    let Some(init) = class_member(ctx, base_settings.body_scope(db), "__init__")
        .ignore_possibly_undefined()
        .and_then(Type::as_function_literal)
    else {
        return;
    };
    let Some(signature) = init.signature(ctx).iter().next() else {
        return;
    };

    parameters.extend(
        signature
            .parameters()
            .iter()
            .filter(|parameter| {
                parameter.name().is_some_and(|name| {
                    name.as_str().starts_with('_') && !name.as_str().starts_with("__")
                })
            })
            .cloned(),
    );
}

fn model_config<'db>(ctx: &SemanticContext<'db>, class: StaticClassLiteral<'db>) -> ModelConfig {
    let db = ctx.db();
    debug_assert_eq!(ctx.program(), class.program(db));
    model_config_inner(db, class)
}

#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, _, _| ModelConfig::unknown(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn model_config_inner<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> ModelConfig {
    let ctx = SemanticContext::from_file(db, class.python_file(db));
    let mut config = ModelConfig::default();

    // Pydantic merges the effective config from each direct base from left to right. A later base
    // therefore takes precedence over an earlier base.
    for base in class.explicit_bases(&ctx) {
        let Some(base) = base.to_class_type(&ctx) else {
            config = ModelConfig::unknown();
            continue;
        };
        let base_literal = base.class_literal(db);
        let Some(base) = base_literal.as_static() else {
            config.merge(ModelConfig::unknown());
            continue;
        };

        if is_model(&ctx, base) {
            config.merge(model_config(&ctx, base));
        } else if let Some(base_config) = inherited_model_config(&ctx, base) {
            config.merge(base_config);
        }
    }

    if let Some(own_config) = own_model_config(&ctx, class) {
        config.merge(own_config);
    }
    config.merge(class_keyword_config(&ctx, class));
    config
}

fn inherited_model_config(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> Option<ModelConfig> {
    let db = ctx.db();
    for base in class.iter_mro(ctx, None).filter_map(ClassBase::into_class) {
        let Some((base, _)) = base.static_class_literal(db) else {
            return Some(ModelConfig::unknown());
        };
        if let Some(config) = own_model_config(ctx, base) {
            return Some(config);
        }
    }
    None
}

fn own_model_config(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> Option<ModelConfig> {
    let db = ctx.db();
    let model_config = class_member(ctx, class.body_scope(db), "model_config")
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

    let module = parsed_module(db, class.python_file(db)).load(db);
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

    if let Some(dict) = value.as_dict_expr() {
        return Some(model_config_from_dict(ctx, definition, dict));
    }

    let Some(call) = value.as_call_expr() else {
        return Some(ModelConfig::unknown());
    };
    let callee = definition_expression_type(ctx, definition, &call.func);
    if !callee.as_class_literal().is_some_and(|class| {
        class.is_known(db, KnownClass::PydanticConfigDict) || class.is_known(db, KnownClass::Dict)
    }) {
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

    // Keep this list of recognized options in sync with `model_config_from_dict` and
    // `class_keyword_config`.
    let extra = call.arguments.find_keyword("extra").map(|extra| {
        let extra = definition_expression_type(ctx, definition, &extra.value)
            .as_string_literal()
            .map(|literal| literal.value(db));
        ExtraBehavior::from_value(extra)
    });
    let strict = config_boolean(ctx, definition, call.arguments.find_keyword("strict"));
    let from_attributes = config_boolean(
        ctx,
        definition,
        call.arguments.find_keyword("from_attributes"),
    );
    let frozen = config_boolean(ctx, definition, call.arguments.find_keyword("frozen"));
    let validate_by_alias = config_boolean(
        ctx,
        definition,
        call.arguments.find_keyword("validate_by_alias"),
    );
    let validate_by_name = config_boolean(
        ctx,
        definition,
        call.arguments.find_keyword("validate_by_name"),
    );
    let populate_by_name = config_boolean(
        ctx,
        definition,
        call.arguments.find_keyword("populate_by_name"),
    );

    Some(ModelConfig {
        extra,
        strict,
        from_attributes,
        frozen,
        validate_by_alias,
        validate_by_name,
        populate_by_name,
    })
}

fn model_config_from_dict(
    ctx: &SemanticContext<'_>,
    definition: Definition<'_>,
    dict: &ExprDict,
) -> ModelConfig {
    let db = ctx.db();
    let mut config = ModelConfig::default();

    for item in dict {
        let Some(key) = item
            .key
            .as_ref()
            .and_then(|key| key.as_string_literal_expr())
        else {
            // A dynamic key or dictionary unpacking can override any recognized option. Ignoring
            // it could incorrectly preserve a lower-precedence configuration value.
            return ModelConfig::unknown();
        };
        let value = definition_expression_type(ctx, definition, &item.value);

        // Keep this match in sync with the options recognized for `ConfigDict` calls in
        // `own_model_config` and for class keywords in `class_keyword_config`.
        match key.value.to_str() {
            "extra" => {
                config.extra = Some(ExtraBehavior::from_value(
                    value.as_string_literal().map(|literal| literal.value(db)),
                ));
            }
            "strict" => config.strict = ConfigBoolean::from_type(value),
            "from_attributes" => config.from_attributes = ConfigBoolean::from_type(value),
            "frozen" => config.frozen = ConfigBoolean::from_type(value),
            "validate_by_alias" => config.validate_by_alias = ConfigBoolean::from_type(value),
            "validate_by_name" => config.validate_by_name = ConfigBoolean::from_type(value),
            "populate_by_name" => config.populate_by_name = ConfigBoolean::from_type(value),
            _ => {}
        }
    }

    config
}

fn class_keyword_config(ctx: &SemanticContext<'_>, class: StaticClassLiteral<'_>) -> ModelConfig {
    let db = ctx.db();
    let definition = class.definition(db);
    let module = parsed_module(db, class.python_file(db)).load(db);
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
    // Keep this list of recognized options in sync with `own_model_config` and
    // `model_config_from_dict`.
    let extra = arguments.find_keyword("extra").map(|extra| {
        let extra = definition_expression_type(ctx, definition, &extra.value)
            .as_string_literal()
            .map(|literal| literal.value(db));
        ExtraBehavior::from_value(extra)
    });
    let strict = config_boolean(ctx, definition, arguments.find_keyword("strict"));
    let from_attributes =
        config_boolean(ctx, definition, arguments.find_keyword("from_attributes"));
    let frozen = config_boolean(ctx, definition, arguments.find_keyword("frozen"));
    let validate_by_alias =
        config_boolean(ctx, definition, arguments.find_keyword("validate_by_alias"));
    let validate_by_name =
        config_boolean(ctx, definition, arguments.find_keyword("validate_by_name"));
    let populate_by_name =
        config_boolean(ctx, definition, arguments.find_keyword("populate_by_name"));

    ModelConfig {
        extra,
        strict,
        from_attributes,
        frozen,
        validate_by_alias,
        validate_by_name,
        populate_by_name,
    }
}

/// Return the input type accepted by a Pydantic field's synthesized constructor parameter.
pub(in crate::types) fn constructor_parameter_type<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
    field_name: &Name,
    field_type: Type<'db>,
    field_strict: ConfigBoolean,
    metadata: ModelMetadata<'db>,
) -> Type<'db> {
    let db = ctx.db();
    if has_before_or_plain_field_validator(ctx, class, field_name.clone()) {
        return Type::any();
    }

    if field_strict.or(metadata.config(db).strict).is_enabled() {
        return field_type;
    }

    lax_input_type(ctx, field_type)
}

/// Return whether `field_name` has a Pydantic field validator that receives the raw input.
///
/// A before validator can transform arbitrary values before Pydantic validates them against the
/// declared field type, while a plain validator bypasses that validation entirely. We therefore
/// cannot derive a useful input type from the field annotation alone.
pub(in crate::types) fn has_before_or_plain_field_validator<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
    field_name: Name,
) -> bool {
    debug_assert_eq!(ctx.program(), class.program(ctx.db()));
    has_before_or_plain_field_validator_inner(ctx.db(), class, field_name)
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn has_before_or_plain_field_validator_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    field_name: Name,
) -> bool {
    let ctx = SemanticContext::from_file(db, class.python_file(db));
    let field_name = CharStr::from(field_name);

    // Pydantic inherits validators unless a subclass defines a symbol with the same method name.
    let mut shadowed_symbols = FxHashSet::default();

    for base in class.iter_mro(&ctx, None).filter_map(ClassBase::into_class) {
        if base.is_known(db, KnownClass::PydanticBaseModel) {
            break;
        }
        let Some((base, _)) = base.static_class_literal(db) else {
            continue;
        };
        let body_scope = base.body_scope(db);
        let use_def = use_def_map(db, body_scope);
        let table = place_table(db, body_scope);

        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            let name = table.symbol(symbol_id).name().clone();
            if !shadowed_symbols.insert(name) {
                continue;
            }
            if declarations.any_reachable(&ctx, |declaration| {
                declaration.is_defined_and(|definition| {
                    function_has_before_or_plain_field_validator(
                        &ctx,
                        definition,
                        field_name.as_str(),
                    )
                })
            }) {
                return true;
            }
        }
    }

    false
}

fn function_has_before_or_plain_field_validator<'db>(
    ctx: &SemanticContext<'db>,
    definition: Definition<'db>,
    field_name: &str,
) -> bool {
    let db = ctx.db();
    let DefinitionKind::Function(function) = definition.kind(db) else {
        return false;
    };
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let function_node = function.node(&module);
    if function_node.decorator_list.is_empty() {
        return false;
    }
    let decorators = function_known_decorators(ctx, definition);

    function_node.decorator_list.iter().any(|decorator| {
        let Some(call) = decorator.expression.as_call_expr() else {
            return false;
        };
        let Some(Type::FunctionLiteral(function)) = decorators.expression_type(call.func.as_ref())
        else {
            return false;
        };
        if !function.is_known(db, KnownFunction::PydanticFieldValidator) {
            return false;
        }

        let Some(mode) = call.arguments.find_keyword("mode") else {
            return false;
        };
        if decorators
            .expression_type(&mode.value)
            .and_then(Type::as_string_literal)
            .is_none_or(|mode| !matches!(mode.value(db), "before" | "plain"))
        {
            return false;
        }

        call.arguments.args.iter().any(|field| {
            decorators
                .expression_type(field)
                .and_then(Type::as_string_literal)
                .is_some_and(|field| {
                    let field = field.value(db);
                    field == "*" || field == field_name
                })
        })
    })
}

/// Return the documented Python input type accepted by Pydantic for `field_type` in lax mode.
fn lax_input_type<'db>(ctx: &SemanticContext<'db>, field_type: Type<'db>) -> Type<'db> {
    lax_input_type_impl(ctx, field_type, &mut FxHashSet::default())
}

fn lax_input_type_impl<'db>(
    ctx: &SemanticContext<'db>,
    field_type: Type<'db>,
    expanding_types: &mut FxHashSet<Type<'db>>,
) -> Type<'db> {
    let db = ctx.db();
    if field_type.is_none(db) || matches!(field_type, Type::LiteralValue(_) | Type::SubclassOf(_)) {
        return field_type;
    }

    if let Type::TypeAlias(alias) = field_type {
        if !expanding_types.insert(field_type) {
            return Type::any();
        }
        let result = lax_input_type_impl(ctx, alias.value_type(ctx), expanding_types);
        expanding_types.remove(&field_type);
        return result;
    }

    if field_type.as_union().and_then(|union| union.known(db)) == Some(KnownUnion::Float) {
        return lax_alias(ctx, "LaxFloat");
    }

    if let Type::Union(union) = field_type {
        return UnionType::from_elements_leave_aliases(
            ctx,
            union
                .elements(db)
                .iter()
                .map(|element| lax_input_type_impl(ctx, *element, expanding_types)),
        );
    }

    if let Some(input_type) = root_model_input_type(ctx, field_type, expanding_types) {
        return input_type;
    }

    if let Some(input_type) = model_input_type(ctx, field_type) {
        return input_type;
    }

    let known_class = field_type
        .nominal_class(ctx)
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
        let Ok(elements) = field_type.try_iterate(ctx) else {
            return Type::any();
        };
        let element_type =
            lax_input_type_impl(ctx, elements.homogeneous_element_type(ctx), expanding_types);
        return KnownClass::Iterable.to_specialized_instance(ctx, &[element_type]);
    }

    if matches!(known_class, Some(KnownClass::Dict | KnownClass::Mapping)) {
        let Some(specialization) =
            known_class.and_then(|known_class| field_type.known_specialization(ctx, known_class))
        else {
            return Type::any();
        };
        let [key_type, value_type] = specialization.types(db) else {
            return Type::any();
        };
        let value_type = lax_input_type_impl(ctx, *value_type, expanding_types);
        return KnownClass::Mapping.to_specialized_instance(ctx, &[*key_type, value_type]);
    }

    let builtin_alias = match known_class {
        Some(KnownClass::Bool) => Some("LaxBool"),
        Some(KnownClass::Bytes) => Some("LaxBytes"),
        Some(KnownClass::Float) => Some("LaxFloat"),
        Some(KnownClass::Int) => Some("LaxInt"),
        Some(KnownClass::Str) => Some("LaxStr"),
        Some(KnownClass::Path) => Some("LaxPath"),
        _ => None,
    };
    if let Some(alias) = builtin_alias {
        return lax_alias(ctx, alias);
    }

    let Some((module, symbol, class)) = instance_symbol(ctx, field_type) else {
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
        return lax_alias(ctx, alias);
    }

    let alias = if (module, symbol) == (KnownModule::Re, "Pattern") {
        let Some(specialization) = field_type.specialization_of(ctx, class) else {
            return Type::any();
        };
        let [pattern_type] = specialization.types(db) else {
            return Type::any();
        };
        if pattern_type
            .nominal_class(ctx)
            .is_some_and(|class| class.is_known(db, KnownClass::Str))
        {
            "LaxStrPattern"
        } else if pattern_type
            .nominal_class(ctx)
            .is_some_and(|class| class.is_known(db, KnownClass::Bytes))
        {
            "LaxBytesPattern"
        } else {
            return Type::any();
        }
    } else {
        return Type::any();
    };

    lax_alias(ctx, alias)
}

/// Return the input type accepted for a Pydantic root model field.
///
/// This builds a union of the root model instance and its transformed raw root type. For example,
/// a field annotated with an `IntList` derived from `RootModel[list[int]]` accepts both an
/// `IntList` instance and an `Iterable[LaxInt]`.
fn root_model_input_type<'db>(
    ctx: &SemanticContext<'db>,
    field_type: Type<'db>,
    expanding_types: &mut FxHashSet<Type<'db>>,
) -> Option<Type<'db>> {
    let db = ctx.db();
    let (class, specialization) = field_type.nominal_class(ctx)?.static_class_literal(db)?;
    if !is_root_model(ctx, class) {
        return None;
    }

    let Some(field_policy @ CodeGeneratorKind::Pydantic(_)) =
        CodeGeneratorKind::from_class(ctx, class.into())
    else {
        return Some(Type::any());
    };
    let Some(root_field) = class.fields(ctx, specialization, field_policy).get("root") else {
        return Some(Type::any());
    };

    if !expanding_types.insert(field_type) {
        return Some(Type::any());
    }
    let root_input_type = lax_input_type_impl(ctx, root_field.declared_ty, expanding_types);

    expanding_types.remove(&field_type);
    // In lax mode, Pydantic accepts a Box[str] when a Box[int] is expected, so we widen
    // to a gradual specialization here. Widening to `Box[LaxStr]` would only work for
    // covariant generics.
    let model_instance = Type::instance(ctx, class.unknown_specialization(ctx));
    Some(UnionType::from_two_elements(
        ctx,
        model_instance,
        root_input_type,
    ))
}

/// Return the input type accepted for an ordinary Pydantic model field.
///
/// By default, Pydantic accepts either an instance of the model or a mapping of string keys to
/// input values. Other custom validators can accept additional input types, which are not modeled
/// here.
fn model_input_type<'db>(ctx: &SemanticContext<'db>, field_type: Type<'db>) -> Option<Type<'db>> {
    let db = ctx.db();
    let (class, _) = field_type.nominal_class(ctx)?.static_class_literal(db)?;
    if !is_model(ctx, class) || is_root_model(ctx, class) {
        return None;
    }

    // Attribute-based validation can accept arbitrary objects that do not implement `Mapping`.
    if model_config(ctx, class).from_attributes.enabled_or(false) {
        return Some(Type::any());
    }

    // In lax mode, Pydantic accepts a Box[str] when a Box[int] is expected, so we widen
    // to a gradual specialization here. Widening to `Box[LaxStr]` would only work for
    // covariant generics.
    let model_instance = Type::instance(ctx, class.unknown_specialization(ctx));
    let mapping = KnownClass::Mapping
        .to_specialized_instance(ctx, &[KnownClass::Str.to_instance(ctx), Type::any()]);
    Some(UnionType::from_two_elements(ctx, model_instance, mapping))
}

/// Return the known module, name, and class literal for an instance's nominal class.
fn instance_symbol<'db>(
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
) -> Option<(KnownModule, &'db str, StaticClassLiteral<'db>)> {
    let db = ctx.db();
    let class = ty.nominal_class(ctx)?.class_literal(db).as_static()?;
    let module = file_to_module(db, class.python_file(db))?.known(db)?;
    Some((module, class.name(db).as_str(), class))
}

/// Return a lax-input alias like `LaxInt` from `ty_extensions.pydantic`.
fn lax_alias<'db>(ctx: &SemanticContext<'db>, name: &str) -> Type<'db> {
    match known_module_symbol(ctx, KnownModule::TyExtensionsPydantic, name)
        .place
        .ignore_possibly_undefined()
    {
        Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(alias))) => {
            Type::TypeAlias(alias)
        }
        _ => Type::any(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelInitBehavior {
    /// The model inherits Pydantic's ordinary `BaseModel` initializer.
    BaseModel,
    /// The first custom initializer in the MRO accepts arbitrary keyword arguments.
    CustomVariadic,
    /// The first custom initializer in the MRO has a fixed parameter list.
    CustomFixed,
    /// The model has a specialized Pydantic initializer or no recognized initializer.
    Other,
}

fn model_init_behavior(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> ModelInitBehavior {
    let db = ctx.db();
    for base in class
        .iter_mro(ctx, None)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.static_class_literal(db))
        .map(|(base, _)| base)
    {
        if base.is_known(db, KnownClass::PydanticBaseModel) {
            return ModelInitBehavior::BaseModel;
        }

        // These constructors use variadic keywords for specialized inputs, not arbitrary extras.
        if base.is_known(db, KnownClass::PydanticRootModel)
            || base.is_known(db, KnownClass::PydanticBaseSettings)
        {
            return ModelInitBehavior::Other;
        }

        let init = class_member(ctx, base.body_scope(db), "__init__");
        if !init.is_undefined() {
            return if init
                .ignore_possibly_undefined()
                .and_then(Type::as_function_literal)
                .is_some_and(|init| {
                    init.signature(ctx)
                        .iter()
                        .any(|signature| signature.parameters().keyword_variadic().is_some())
                }) {
                ModelInitBehavior::CustomVariadic
            } else {
                ModelInitBehavior::CustomFixed
            };
        }
    }

    ModelInitBehavior::Other
}

/// Return `true` if `class` should synthesize a field-derived constructor signature.
///
/// A fixed custom initializer on an intermediate base class controls the constructor accepted by
/// its subclasses. A variadic custom initializer still allows Pydantic to validate field values
/// passed via keyword arguments.
pub(in crate::types) fn synthesizes_constructor_signature_from_fields(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
) -> bool {
    model_init_behavior(ctx, class) != ModelInitBehavior::CustomFixed
}

/// Return `true` if `class` should accept extra keywords in its synthesized constructor.
pub(in crate::types) fn model_init_accepts_extra(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
    metadata: ModelMetadata<'_>,
) -> bool {
    let db = ctx.db();
    metadata.accepts_extra(db)
        && matches!(
            model_init_behavior(ctx, class),
            ModelInitBehavior::BaseModel | ModelInitBehavior::CustomVariadic
        )
}

/// Return `true` if extra keywords passed to `class` are silently discarded by Pydantic.
pub(in crate::types) fn model_init_discards_extra(
    ctx: &SemanticContext<'_>,
    class: StaticClassLiteral<'_>,
    metadata: ModelMetadata<'_>,
) -> bool {
    let db = ctx.db();
    metadata.discards_extra(db) && model_init_behavior(ctx, class) == ModelInitBehavior::BaseModel
}

/// Report keyword arguments that the Pydantic model constructor silently discards.
pub(in crate::types) fn report_discarded_extra_arguments<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    arguments: &Arguments,
    bindings: &Bindings<'db>,
) {
    if !context.is_lint_enabled(&PYDANTIC_DISCARDED_EXTRA_ARGUMENT) {
        return;
    }

    let db = context.db();
    let ctx = context.semantic_context();
    let Some((class, _)) = class.static_class_literal(db) else {
        return;
    };
    let Some(metadata) = CodeGeneratorKind::from_class(ctx, class.into())
        .and_then(CodeGeneratorKind::pydantic_metadata)
    else {
        return;
    };
    if !model_init_discards_extra(ctx, class, metadata) {
        return;
    }

    let extra_names: Vec<_> = arguments
        .iter_source_order()
        .enumerate()
        .filter_map(|(argument_index, argument)| {
            let ArgOrKeyword::Keyword(keyword) = argument else {
                return None;
            };
            let name = keyword.arg.as_ref()?;
            bindings
                .constructor_init_argument_matches_keyword_variadic(argument_index)
                .then_some(name)
        })
        .collect();

    if extra_names.is_empty() {
        return;
    }

    let Some(builder) = context.report_lint(&PYDANTIC_DISCARDED_EXTRA_ARGUMENT, arguments) else {
        return;
    };

    if let [name] = extra_names.as_slice() {
        builder.into_diagnostic(format_args!(
            "Extra argument `{name}` is discarded by Pydantic"
        ));
    } else {
        builder.into_diagnostic(format_args!(
            "Extra arguments {} are discarded by Pydantic",
            format_enumeration(extra_names.iter().map(|name| format!("`{name}`")))
        ));
    }
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
