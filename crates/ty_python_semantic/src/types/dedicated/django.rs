use ruff_db::parsed::parsed_module;
use ty_module_resolver::KnownModule;
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    use_def_map,
};

use crate::place::known_module_symbol;
use crate::types::member::class_member;
use crate::types::{
    ClassBase, KnownClass, Specialization, StaticClassLiteral, Type, binding_type,
    definition_expression_type,
};
use crate::{Db, ProgramEnvironment};

/// Returns whether `class` is a subclass of Django's `Model`.
pub(in crate::types) fn is_model<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::DjangoModel))
}

/// The type of a Django model field, and metadata extracted from the field construction.
///
/// For example, the `isbn` field has type `CharField[str]` and `metadata.primary_key: true`:
/// ```py
/// class Book(models.Model):
///     isbn = CharField(max_length=13, primary_key=True)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct Field<'db> {
    ty: Type<'db>,
    metadata: FieldMetadata,
}

impl<'db> Field<'db> {
    /// Create a field by extracting the metadata from its definition.
    fn from_definition(db: &'db dyn Db, ty: Type<'db>, definition: Definition<'db>) -> Self {
        let mut field = Field {
            ty,
            metadata: FieldMetadata::default(),
        };
        let module = parsed_module(db, definition.python_file(db)).load(db);
        let kind = definition.kind(db);
        let value = match &kind {
            DefinitionKind::Assignment(assignment) => assignment.value(&module),
            DefinitionKind::AnnotatedAssignment(assignment) => {
                let Some(value) = assignment.value(&module) else {
                    return field;
                };
                value
            }
            _ => return field,
        };
        let Some(call) = value.as_call_expr() else {
            return field;
        };

        field.metadata.primary_key =
            call.arguments
                .find_keyword("primary_key")
                .is_some_and(|primary_key| {
                    definition_expression_type(db, definition, &primary_key.value)
                        == Type::bool_literal(true)
                });
        field
    }
}

/// Metadata extracted from a Django field's construction.
#[derive(Debug, Clone, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct FieldMetadata {
    primary_key: bool,
}

/// All fields defined on this Django model (not including base class fields).
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn own_model_fields<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Box<[Field<'db>]> {
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    use_def_map(db, class.body_scope(db))
        .all_end_of_scope_symbol_bindings()
        .flat_map(|(_, bindings)| bindings.filter_map(|binding| binding.binding.definition()))
        .filter_map(|definition| {
            let field_ty = binding_type(db, definition);
            let is_field = field_ty.nominal_class(db, &env).is_some_and(|class| {
                class
                    .iter_mro(db)
                    .filter_map(ClassBase::into_class)
                    .any(|base| base.is_known(db, KnownClass::DjangoField))
            });
            if !is_field {
                return None;
            }
            Some(Field::from_definition(db, field_ty, definition))
        })
        .collect()
}

/// Synthesize `id` and `pk` attributes for Django models.
///
/// Without an explicit primary key, both `id` and `pk` have type `int`:
/// ```py
/// class Book(models.Model):
///     title = models.CharField(max_length=200)
///
/// # Book().id and Book().pk have type int.
/// ```
///
/// With an explicit primary key, `pk` has that field's value type and no automatic `id` is added:
/// ```py
/// class Book(models.Model):
///     isbn = models.CharField(max_length=13, primary_key=True)
///
/// # Book().id does not exist; Book().pk has type str.
/// ```
pub(in crate::types) fn primary_key_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
    if !matches!(name, "id" | "pk") || !is_model(db, class) {
        return None;
    }

    // Explicit `id` or `pk` attributes on base classes take precedence over a synthesized
    // attribute on this model.
    if class
        .iter_mro(db, specialization)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.static_class_literal(db))
        .filter(|(base, _)| !base.is_known(db, KnownClass::DjangoModel))
        .any(|(base, _)| !class_member(db, base.body_scope(db), name).is_undefined())
    {
        return None;
    }

    for (base, specialization) in class
        .iter_mro(db, specialization)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.static_class_literal(db))
    {
        if base.is_known(db, KnownClass::DjangoModel) {
            break;
        }
        if let Some(field) = own_primary_key(db, base) {
            return (name == "pk").then(|| field.apply_optional_specialization(db, specialization));
        }
    }

    known_module_symbol(db, env, KnownModule::DjangoDbModelsFields, "AutoField")
        .place
        .ignore_possibly_undefined()?
        .to_instance_approximation(db, env)
}

/// Find a field constructed with `primary_key=True` in this class body.
fn own_primary_key<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Option<Type<'db>> {
    own_model_fields(db, class)
        .iter()
        .find(|field| field.metadata.primary_key)
        .map(|field| field.ty)
}
