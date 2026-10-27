use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ty_module_resolver::KnownModule;
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    place_table, semantic_index, use_def_map,
};

use crate::place::{known_module_symbol, place_from_bindings};
use crate::types::infer::nearest_enclosing_class;
use crate::types::member::class_member;
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassBase, GenericContext, KnownClass, Parameter,
    Parameters, SelfTypeVarOrigin, Signature, Specialization, StaticClassLiteral, Type, UnionType,
    binding_type, definition_expression_type,
};
use crate::{Db, ProgramEnvironment};

/// Returns whether `class` is a subclass of Django's `Model`.
fn is_model<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::DjangoModel))
}

/// The type of a Django model field, and metadata extracted from the field construction.
///
/// For example, `isbn` is a `CharField` with `metadata.primary_key` set to `true`.
/// ```py
/// class Book(models.Model):
///     isbn = CharField(max_length=13, primary_key=True)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct Field<'db> {
    name: Name,
    ty: Type<'db>,
    metadata: FieldMetadata,
}

impl<'db> Field<'db> {
    /// Create a field by extracting the metadata from its definition.
    fn from_definition(
        db: &'db dyn Db,
        name: Name,
        ty: Type<'db>,
        definition: Definition<'db>,
    ) -> Self {
        let mut field = Field {
            name,
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

        if let Some(name) = call.arguments.find_keyword("name")
            && let Some(name) =
                definition_expression_type(db, definition, &name.value).as_string_literal()
            && !name.value(db).is_empty()
        {
            field.name = Name::new(name.value(db));
        }

        field.metadata.primary_key =
            call.arguments
                .find_keyword("primary_key")
                .is_some_and(|primary_key| {
                    definition_expression_type(db, definition, &primary_key.value)
                        == Type::bool_literal(true)
                });
        field.metadata.has_choices =
            call.arguments
                .find_keyword("choices")
                .is_some_and(|choices| {
                    !definition_expression_type(db, definition, &choices.value).is_none(db)
                });
        field.metadata.null = call.arguments.find_keyword("null").is_some_and(|null| {
            definition_expression_type(db, definition, &null.value) == Type::bool_literal(true)
        });
        field.metadata.to_field = call
            .arguments
            .find_keyword("to_field")
            .and_then(|to_field| {
                definition_expression_type(db, definition, &to_field.value)
                    .as_string_literal()
                    .map(|name| Name::new(name.value(db)))
            });
        field
    }
}

/// Metadata extracted from a Django field's construction.
#[derive(Debug, Clone, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct FieldMetadata {
    primary_key: bool,
    has_choices: bool,
    null: bool,
    to_field: Option<Name>,
}

/// All fields defined on this Django model (not including base class fields).
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn own_model_fields<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Box<[Field<'db>]> {
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    let table = place_table(db, class.body_scope(db));
    use_def_map(db, class.body_scope(db))
        .all_end_of_scope_symbol_bindings()
        .flat_map(|(symbol_id, bindings)| {
            let name = table.symbol(symbol_id).name();
            bindings.filter_map(move |binding| {
                binding
                    .binding
                    .definition()
                    .map(|definition| (name, definition))
            })
        })
        .filter_map(|(name, definition)| {
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
            Some(Field::from_definition(
                db,
                name.clone(),
                field_ty,
                definition,
            ))
        })
        .collect()
}

/// Metadata extracted from a Django model's `Meta` configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct ModelMetadata {
    is_abstract: bool,
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn model_metadata<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> ModelMetadata {
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    // Subclasses do not inherit `abstract = True`, so only inspect this model's own Meta.
    let Some(meta) = own_meta(db, class) else {
        return ModelMetadata::default();
    };

    for meta_base in meta.iter_mro(db, None).filter_map(ClassBase::into_class) {
        let Some((meta_base, _)) = meta_base.static_class_literal(db) else {
            return ModelMetadata::default();
        };
        // Django resets an abstract model's Meta.abstract to False during model creation.
        // A child can inherit other options from that Meta, but must set abstract=True itself.
        let enclosing_scope = meta_base.definition(db).scope(db);
        if let Some(owner) = nearest_enclosing_class(
            db,
            semantic_index(db, enclosing_scope.program_file(db)),
            enclosing_scope,
        ) && owner != class
            && is_model(db, owner)
            && own_meta(db, owner) == Some(meta_base)
        {
            return ModelMetadata::default();
        }

        let scope = meta_base.body_scope(db);
        let Some(symbol) = place_table(db, scope).symbol_id("abstract") else {
            continue;
        };
        // The public type is widened to `bool`, so read the bindings' literal values instead.
        let Some(abstract_flag) = place_from_bindings(
            db,
            &env,
            use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
        )
        .place
        .ignore_possibly_undefined() else {
            continue;
        };
        return ModelMetadata {
            is_abstract: abstract_flag == Type::bool_literal(true),
        };
    }
    ModelMetadata::default()
}

fn own_meta<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<StaticClassLiteral<'db>> {
    let meta = class_member(db, class.body_scope(db), "Meta")
        .ignore_possibly_undefined()?
        .as_class_literal()?
        .as_static()?;
    (meta.name(db).as_str() == "Meta" && meta.definition(db).scope(db) == class.body_scope(db))
        .then_some(meta)
}

/// Provides synthesized attributes and methods for Django models.
pub(in crate::types) fn synthesized_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
    // A quick filter on the name, so that we can return quickly for almost all attribute names.
    if !(matches!(name, "id" | "pk")
        || name.ends_with("_id")
        || name.starts_with("get_") && name.ends_with("_display")
        || name.starts_with("get_next_by_")
        || name.starts_with("get_previous_by_"))
    {
        return None;
    }
    if !is_model(db, class) {
        return None;
    }
    match name {
        "id" | "pk" => primary_key_member(db, env, class, specialization, name),
        _ => display_method(db, env, class, specialization, name)
            .or_else(|| date_navigation_method(db, env, class, specialization, name))
            .or_else(|| foreign_key_id_member(db, env, class, specialization, name)),
    }
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
fn primary_key_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
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

    // Abstract models do not have an automatic primary key.
    if model_metadata(db, class).is_abstract {
        return None;
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

/// Synthesize `<field>_id` attributes for foreign keys using the referenced field's type.
///
/// For example, `Book().author_id` has type `int`, matching `Author`'s automatic primary key:
/// ```py
/// class Author(models.Model):
///     pass
///
/// class Book(models.Model):
///     author = models.ForeignKey(Author, on_delete=models.CASCADE)
/// ```
fn foreign_key_id_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
    let field_name = name.strip_suffix("_id")?;
    let field = own_model_fields(db, class)
        .iter()
        .find(|field| field.name == field_name)?;
    let field_ty = field.ty.apply_optional_specialization(db, specialization);
    let foreign_key = KnownClass::DjangoForeignKey.try_to_class_literal(db, env)?;
    if !field_ty
        .nominal_class(db, env)?
        .is_subtype_of_class_literal(db, foreign_key.into())
    {
        return None;
    }

    // django-types represents a foreign key declared with `null=True` as
    // `ForeignKey[Author | None]`, so its `related_model` attribute has type
    // `type[Author | None]`. Remove `None` from the instance type before looking up `pk`
    // (or `to_field`) on `Author`. If `null=True`, add `None` to the
    // resulting ID type: for example, an integer primary key becomes `int | None`.
    let related_model = field_ty
        .member(db, env, "related_model")
        .place
        .ignore_possibly_undefined()?
        .to_instance_approximation(db, env)?
        .filter_union(db, env, |ty| !ty.is_none(db));
    let target_name = field.metadata.to_field.as_deref().unwrap_or("pk");
    let id_ty = related_model
        .member(db, env, target_name)
        .place
        .ignore_possibly_undefined()?;
    Some(if field.metadata.null {
        UnionType::from_elements(db, env, [id_ty, Type::none(db, env)])
    } else {
        id_ty
    })
}

/// Synthesize `get_<field>_display()` for fields with `choices` defined.
fn display_method<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
    let field_name = name.strip_prefix("get_")?.strip_suffix("_display")?;
    let field = own_model_fields(db, class)
        .iter()
        .find(|field| field.name == field_name)?;
    if !field.metadata.has_choices {
        return None;
    }

    // Construct `def get_<field>_display(self: MyModel) -> str`.
    let self_ty = Type::instance(
        db,
        env,
        class.apply_optional_specialization(db, specialization),
    );
    let signature = Signature::new(
        Parameters::standard([
            Parameter::positional_or_keyword(Name::new_static("self")).with_annotated_type(self_ty)
        ]),
        KnownClass::Str.to_instance(db, env),
    );
    Some(Type::function_like_callable(db, signature))
}

/// Synthesize `get_next_by_<field>()` and `get_previous_by_<field>()` for non-nullable date fields.
fn date_navigation_method<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &str,
) -> Option<Type<'db>> {
    let field_name = name
        .strip_prefix("get_next_by_")
        .or_else(|| name.strip_prefix("get_previous_by_"))?;
    let field = own_model_fields(db, class)
        .iter()
        .find(|field| field.name == field_name)?;
    if field.metadata.null {
        return None;
    }
    // We add these methods for all field types that inherit from `DateField`, like `DateTimeField`.
    let date_field = KnownClass::DjangoDateField.try_to_class_literal(db, env)?;
    let field_class = field.ty.nominal_class(db, env)?;
    if !field_class.is_subtype_of_class_literal(db, date_field.into()) {
        return None;
    }

    // Construct `def get_{next,previous}_by_<field>(self: Self, /, **kwargs: Any) -> Self`.
    let self_typevar = BoundTypeVarInstance::synthetic_self(
        db,
        SelfTypeVarOrigin::Class(class.into()),
        Type::instance(
            db,
            env,
            class.apply_optional_specialization(db, specialization),
        ),
        BindingContext::Synthetic(env.program(db)),
    );
    let self_ty = Type::TypeVar(self_typevar);
    let generic_context = GenericContext::from_typevar_instances(db, env, [self_typevar]);
    let signature = Signature::new_generic(
        Some(generic_context),
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(self_ty),
            Parameter::keyword_variadic(Name::new_static("kwargs"))
                .with_annotated_type(Type::any()),
        ]),
        self_ty,
    );
    Some(Type::function_like_callable(db, signature))
}
