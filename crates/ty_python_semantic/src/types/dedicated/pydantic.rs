use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ty_python_core::definition::DefinitionKind;

use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place, Provenance};
use crate::types::member::class_member;
use crate::types::{
    ClassBase, DataclassTransformerParams, KnownClass, Parameter, StaticClassLiteral, Type,
    definition_expression_type,
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ModelConfig {
    /// The `extra` configuration controls whether the synthesized constructor accepts keyword
    /// arguments that do not correspond to declared model fields.
    extra: Option<ExtraBehavior>,
}

impl ModelConfig {
    const fn unknown() -> Self {
        Self {
            extra: Some(ExtraBehavior::Unknown),
        }
    }

    /// Merge `other` into this config, with values from `other` taking precedence.
    fn merge(&mut self, other: Self) {
        self.extra = other.extra.or(self.extra);
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
    !class
        .iter_mro(db, None)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::PydanticRootModel))
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
    if let Some(class_keyword_config) = class_keyword_config(db, class) {
        config.merge(class_keyword_config);
    }
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
            Some(ModelConfig {
                extra: Some(ExtraBehavior::Unknown),
            })
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
            return Some(ModelConfig {
                extra: Some(ExtraBehavior::Unknown),
            });
        }
    };

    let Some(call) = value.as_call_expr() else {
        return Some(ModelConfig {
            extra: Some(ExtraBehavior::Unknown),
        });
    };
    let callee = definition_expression_type(db, definition, &call.func);
    if !callee
        .as_class_literal()
        .is_some_and(|class| class.is_known(db, KnownClass::PydanticConfigDict))
    {
        return Some(ModelConfig {
            extra: Some(ExtraBehavior::Unknown),
        });
    }

    if call
        .arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return Some(ModelConfig {
            extra: Some(ExtraBehavior::Unknown),
        });
    }

    let Some(extra) = call.arguments.find_keyword("extra") else {
        return Some(ModelConfig::default());
    };
    let extra = definition_expression_type(db, definition, &extra.value)
        .as_string_literal()
        .map(|literal| literal.value(db));

    Some(ModelConfig {
        extra: Some(ExtraBehavior::from_value(extra)),
    })
}

fn class_keyword_config(db: &dyn Db, class: StaticClassLiteral<'_>) -> Option<ModelConfig> {
    let definition = class.definition(db);
    let module = parsed_module(db, class.file(db)).load(db);
    let kind = definition.kind(db);
    let class_node = kind.as_class()?.node(&module);
    let arguments = class_node.arguments.as_ref()?;
    if arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return Some(ModelConfig::unknown());
    }
    let extra = arguments.find_keyword("extra")?;
    let extra = definition_expression_type(db, definition, &extra.value)
        .as_string_literal()
        .map(|literal| literal.value(db));

    Some(ModelConfig {
        extra: Some(ExtraBehavior::from_value(extra)),
    })
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
