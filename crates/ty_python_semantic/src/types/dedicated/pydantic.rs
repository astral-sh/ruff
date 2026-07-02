use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::member::class_member;
use crate::types::{ClassBase, KnownClass, Parameter, StaticClassLiteral, Type};

/// Return `true` if `class` should accept extra keywords in its synthesized constructor.
///
/// Pydantic uses `BaseModel` as the public marker for models. A class before `BaseModel` in the MRO
/// can override `__init__`; Pydantic preserves that custom constructor, so it must continue to
/// control which keywords are accepted. This also avoids widening specialized model bases such as
/// `RootModel` and `BaseSettings`, which define their own constructors.
pub(in crate::types) fn uses_base_model_init(db: &dyn Db, class: StaticClassLiteral<'_>) -> bool {
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
