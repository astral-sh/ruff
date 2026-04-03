use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_module_resolver::KnownModule;

use crate::{
    Db, FxIndexMap,
    place::known_module_symbol,
    semantic_index::global_scope,
    types::{
        ClassLiteral, KnownClass, Type, UnionType, class::StaticClassLiteral, member::class_member,
    },
};

/// The category of a Django model field, used to determine the Python type
/// that accessing the field on a model instance should produce.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum DjangoFieldKind {
    // character / text
    Char,
    Binary,
    // numeric
    Integer,
    Float,
    Bool,
    Decimal,
    Auto,
    // date / time
    Date,
    DateTime,
    Time,
    // other scalars
    Uuid,
    Json,
    // TODO: `FileField`/`ImageField` return `FieldFile`/`ImageFieldFile` at runtime;
    // model these once we have stubs for `django.db.models.fields.files`.
    Opaque,
    // relational
    ForeignKey,
    OneToOne,
}

/// A single Django model field declaration with its resolved Python type information.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct DjangoFieldInfo<'db> {
    pub name: Name,
    pub kind: DjangoFieldKind,
    pub nullable: bool,
    pub primary_key: bool,
    /// For `ForeignKey` or `OneToOneField`, the resolved instance type of the target model.
    /// `None` for non-relational fields.
    pub related_model: Option<Type<'db>>,
}

/// Return an instance of the stdlib class `module.class_name`, falling back to `Unknown`
/// if the class cannot be resolved in typeshed.
fn resolve_stdlib_instance<'db>(
    db: &'db dyn Db,
    module: KnownModule,
    class_name: &str,
) -> Type<'db> {
    known_module_symbol(db, module, class_name)
        .place
        .ignore_possibly_undefined()
        .and_then(|ty| ty.to_instance(db))
        .unwrap_or_else(Type::unknown)
}

impl<'db> DjangoFieldInfo<'db> {
    /// Return the Python value type for this field when accessed on a model instance.
    pub(super) fn instance_type(&self, db: &'db dyn Db) -> Type<'db> {
        let base = match self.kind {
            DjangoFieldKind::Char => KnownClass::Str.to_instance(db),
            DjangoFieldKind::Integer | DjangoFieldKind::Auto => KnownClass::Int.to_instance(db),
            DjangoFieldKind::Float => KnownClass::Float.to_instance(db),
            DjangoFieldKind::Bool => KnownClass::Bool.to_instance(db),
            DjangoFieldKind::Date => resolve_stdlib_instance(db, KnownModule::Datetime, "date"),
            DjangoFieldKind::DateTime => {
                resolve_stdlib_instance(db, KnownModule::Datetime, "datetime")
            }
            DjangoFieldKind::Time => resolve_stdlib_instance(db, KnownModule::Datetime, "time"),
            DjangoFieldKind::Decimal => {
                resolve_stdlib_instance(db, KnownModule::Decimal, "Decimal")
            }
            DjangoFieldKind::Uuid => resolve_stdlib_instance(db, KnownModule::Uuid, "UUID"),
            DjangoFieldKind::Binary => KnownClass::Bytes.to_instance(db),
            DjangoFieldKind::Json | DjangoFieldKind::Opaque => Type::unknown(),
            DjangoFieldKind::ForeignKey | DjangoFieldKind::OneToOne => {
                self.related_model.unwrap_or_else(Type::unknown)
            }
        };

        if self.nullable {
            UnionType::from_two_elements(db, base, Type::none(db))
        } else {
            base
        }
    }
}

/// Map a Django field class name (e.g. `"CharField"`) to its [`DjangoFieldKind`].
fn field_class_to_kind(name: &str) -> Option<DjangoFieldKind> {
    Some(match name {
        "CharField"
        | "TextField"
        | "SlugField"
        | "URLField"
        | "EmailField"
        | "GenericIPAddressField"
        | "IPAddressField"
        | "FilePathField" => DjangoFieldKind::Char,

        "FileField" | "ImageField" => DjangoFieldKind::Opaque,

        "IntegerField"
        | "SmallIntegerField"
        | "BigIntegerField"
        | "PositiveIntegerField"
        | "PositiveSmallIntegerField"
        | "PositiveBigIntegerField" => DjangoFieldKind::Integer,

        "FloatField" => DjangoFieldKind::Float,
        "BooleanField" | "NullBooleanField" => DjangoFieldKind::Bool,
        "DateField" => DjangoFieldKind::Date,
        "DateTimeField" => DjangoFieldKind::DateTime,
        "TimeField" => DjangoFieldKind::Time,
        "DecimalField" => DjangoFieldKind::Decimal,
        "UUIDField" => DjangoFieldKind::Uuid,
        "JSONField" => DjangoFieldKind::Json,
        "BinaryField" => DjangoFieldKind::Binary,
        "AutoField" | "BigAutoField" | "SmallAutoField" => DjangoFieldKind::Auto,
        "ForeignKey" | "ForeignObject" => DjangoFieldKind::ForeignKey,
        "OneToOneField" => DjangoFieldKind::OneToOne,
        // TODO: `ManyToManyField` returns a manager at runtime, not a model instance;
        // synthesizing its type requires modelling `RelatedManager`.
        _ => return None,
    })
}

/// Walk the MRO of `class_name` to find a recognized Django field base class.
fn resolve_custom_field_kind(db: &dyn Db, file: File, class_name: &str) -> Option<DjangoFieldKind> {
    let scope = global_scope(db, file);
    let ty = class_member(db, scope, class_name).ignore_possibly_undefined()?;
    let Type::ClassLiteral(ClassLiteral::Static(lit)) = ty else {
        return None;
    };

    for base in lit.iter_mro(db, None) {
        if let Some(class_type) = base.into_class()
            && let Some((base_lit, _)) = class_type.static_class_literal(db)
            && let Some(kind) = field_class_to_kind(base_lit.name(db).as_str())
        {
            return Some(kind);
        }
    }

    None
}

/// Resolve the target model for a `ForeignKey` or `OneToOneField`.
///
/// Handles `ForeignKey(Author)`, `ForeignKey(to=Author)`, `ForeignKey("Author")`,
/// and `ForeignKey("self")`. Returns `Unknown` for targets that cannot be statically
/// resolved (dotted cross-app references, `settings.AUTH_USER_MODEL`).
fn resolve_related_model<'db>(
    db: &'db dyn Db,
    file: File,
    self_class: StaticClassLiteral<'db>,
    call_expr: &ast::ExprCall,
) -> Type<'db> {
    let scope = global_scope(db, file);
    let resolve_name = |name: &str| -> Type<'db> {
        class_member(db, scope, name)
            .ignore_possibly_undefined()
            .and_then(|ty| ty.to_instance(db))
            .unwrap_or_else(Type::unknown)
    };

    // The `to=` keyword takes precedence, matching Django's argument resolution order.
    let to_kwarg = call_expr
        .arguments
        .keywords
        .iter()
        .find_map(|kw| (kw.arg.as_deref() == Some("to")).then_some(&kw.value));
    let target_expr = to_kwarg.or_else(|| call_expr.arguments.args.first());

    match target_expr {
        Some(ast::Expr::Name(name_expr)) => resolve_name(name_expr.id.as_str()),
        Some(ast::Expr::StringLiteral(string_lit)) => {
            let value = string_lit.value.to_str();
            if value == "self" {
                Type::instance(db, self_class.apply_optional_specialization(db, None))
            } else if value.contains('.') {
                // Dotted cross-app references (e.g. "myapp.Author") cannot be
                // resolved without Django's app registry.
                Type::unknown()
            } else {
                resolve_name(value)
            }
        }
        _ => Type::unknown(),
    }
}

/// Collect all Django field declarations across the class hierarchy.
///
/// Iterates the MRO in ancestor-first order so that child fields with the same
/// name override parent fields.
fn collect_all_django_fields<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Vec<DjangoFieldInfo<'db>> {
    let mut fields: FxIndexMap<Name, DjangoFieldInfo<'db>> = FxIndexMap::default();

    for base in class.iter_mro(db, None).rev() {
        let Some(class_type) = base.into_class() else {
            continue;
        };
        let Some((base_lit, _)) = class_type.static_class_literal(db) else {
            continue;
        };
        if base_lit.is_known(db, KnownClass::DjangoModel) || !base_lit.is_django_model(db) {
            continue;
        }
        for field in base_lit.django_model_fields(db) {
            fields.insert(field.name.clone(), field.clone());
        }
    }

    fields.into_values().collect()
}

/// Return the type of `pk`: the field with `primary_key=True`, falling back to
/// any `AutoField`, falling back to `int`. Nullability is always stripped.
fn resolve_pk_type<'db>(db: &'db dyn Db, fields: &[DjangoFieldInfo<'db>]) -> Type<'db> {
    fields
        .iter()
        .find(|f| f.primary_key)
        .or_else(|| {
            fields
                .iter()
                .find(|f| matches!(f.kind, DjangoFieldKind::Auto))
        })
        .map(|f| {
            DjangoFieldInfo {
                nullable: false,
                ..f.clone()
            }
            .instance_type(db)
        })
        .unwrap_or_else(|| KnownClass::Int.to_instance(db))
}

/// Extract the target name and call expression from a field assignment in a model
/// class body, returning `None` for statements that are not field assignments.
fn extract_field_assignment(stmt: &ast::Stmt) -> Option<(Name, &ast::ExprCall)> {
    match stmt {
        ast::Stmt::Assign(assign) => {
            let [target] = assign.targets.as_slice() else {
                return None;
            };
            let ast::Expr::Name(name) = target else {
                return None;
            };
            let ast::Expr::Call(call) = assign.value.as_ref() else {
                return None;
            };
            Some((name.id.clone(), call))
        }
        ast::Stmt::AnnAssign(ann_assign) => {
            let ast::Expr::Name(name) = ann_assign.target.as_ref() else {
                return None;
            };
            let ast::Expr::Call(call) = ann_assign.value.as_deref()? else {
                return None;
            };
            Some((name.id.clone(), call))
        }
        _ => None,
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Return the Django field declarations in this class's own body, excluding
    /// inherited fields.
    ///
    /// Use [`collect_all_django_fields`] to include inherited fields.
    #[salsa::tracked(returns(deref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn django_model_fields(self, db: &'db dyn Db) -> Box<[DjangoFieldInfo<'db>]> {
        let file = self.file(db);
        let module = parsed_module(db, file).load(db);
        let class_stmt = self.node(db, &module);

        let mut fields = Vec::new();

        for stmt in &class_stmt.body {
            let Some((target_name, call_expr)) = extract_field_assignment(stmt) else {
                continue;
            };

            // We can only match the trailing identifier (e.g. `CharField` from
            // `models.CharField(...)`) because full attribute resolution is not
            // available inside a `#[salsa::tracked]` method.
            let field_class_name = match call_expr.func.as_ref() {
                ast::Expr::Name(name) => name.id.to_string(),
                ast::Expr::Attribute(attr) => attr.attr.to_string(),
                _ => continue,
            };

            let Some(kind) = field_class_to_kind(&field_class_name)
                .or_else(|| resolve_custom_field_kind(db, file, &field_class_name))
            else {
                continue;
            };

            let mut nullable = false;
            let mut primary_key = false;
            for keyword in &call_expr.arguments.keywords {
                let Some(arg_name) = keyword.arg.as_deref() else {
                    continue;
                };
                let is_true = matches!(
                    keyword.value,
                    ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, .. })
                );
                match arg_name {
                    "null" if is_true => nullable = true,
                    "primary_key" if is_true => primary_key = true,
                    _ => {}
                }
            }

            // `NullBooleanField` is intrinsically nullable regardless of whether
            // `null=True` was explicitly passed. Deprecated since Django 3.1.
            if field_class_name == "NullBooleanField" {
                nullable = true;
            }

            let related_model = if matches!(
                kind,
                DjangoFieldKind::ForeignKey | DjangoFieldKind::OneToOne
            ) {
                Some(resolve_related_model(db, file, self, call_expr))
            } else {
                None
            };

            fields.push(DjangoFieldInfo {
                name: target_name,
                kind,
                nullable,
                primary_key,
                related_model,
            });
        }

        fields.into_boxed_slice()
    }
}

/// Return the synthesized instance-member type for a Django model field, or `None`
/// if `name` is not a recognized field or `class` is not a Django model.
pub(super) fn synthesize_django_instance_member<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<Type<'db>> {
    if !class.is_django_model(db) || class.is_known(db, KnownClass::DjangoModel) {
        return None;
    }

    match name {
        "pk" => {
            let all_fields = collect_all_django_fields(db, class);
            Some(resolve_pk_type(db, &all_fields))
        }
        "id" => {
            let all_fields = collect_all_django_fields(db, class);
            if let Some(field) = all_fields.iter().find(|f| f.name.as_str() == "id") {
                Some(field.instance_type(db))
            } else if all_fields.iter().any(|f| f.primary_key) {
                // Django only synthesizes an implicit `id` field when no field in the
                // hierarchy has `primary_key=True`.
                None
            } else {
                Some(KnownClass::Int.to_instance(db))
            }
        }
        // Regular fields only search this class's own body; inherited fields are
        // found by the caller's MRO walk via `own_instance_member` on each ancestor.
        // `pk` and `id` above must inspect the full hierarchy because determining
        // the primary key requires cross-class knowledge.
        _ => class
            .django_model_fields(db)
            .iter()
            .find(|f| f.name.as_str() == name)
            .map(|f| f.instance_type(db)),
    }
}
