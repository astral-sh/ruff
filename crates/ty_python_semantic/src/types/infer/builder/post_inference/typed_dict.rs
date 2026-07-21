use crate::SemanticContext;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;

use crate::{
    Db,
    types::{
        ClassType, StaticClassLiteral, Type, TypedDictType,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_TYPED_DICT_FIELD, INVALID_TYPED_DICT_HEADER, INVALID_TYPED_DICT_STATEMENT,
        },
        typed_dict::{TypedDictField, TypedDictOpenness},
    },
};
use ty_python_core::definition::Definition;

pub(super) fn validate_typed_dict_class<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    direct_bases: &[ClassType<'db>],
) {
    validate_typed_dict_class_body(context, class_node);
    validate_typed_dict_field_overrides(context, class, direct_bases);
    validate_typed_dict_openness(context, class, class_node, direct_bases);
}

fn validate_typed_dict_class_body(context: &InferContext<'_, '_>, class_node: &ast::StmtClassDef) {
    // Check that a class-based `TypedDict` doesn't include any invalid statements:
    // https://typing.python.org/en/latest/spec/typeddict.html#class-based-syntax
    //
    //     The body of the class definition defines the items of the `TypedDict` type. It
    //     may also contain a docstring or pass statements (primarily to allow the creation
    //     of an empty `TypedDict`). No other statements are allowed, and type checkers
    //     should report an error if any are present.
    validate_typed_dict_class_body_statements(context, &class_node.body);
}

fn validate_typed_dict_class_body_statements(
    context: &InferContext<'_, '_>,
    statements: &[ast::Stmt],
) {
    for stmt in statements {
        match stmt {
            // Annotated assignments are allowed (that's the whole point), but they're
            // not allowed to have a value.
            ast::Stmt::AnnAssign(ann_assign) => {
                if let Some(value) = &ann_assign.value
                    && let Some(builder) =
                        context.report_lint(&INVALID_TYPED_DICT_STATEMENT, &**value)
                {
                    builder.into_diagnostic("TypedDict item cannot have a value");
                }

                continue;
            }
            // Pass statements are allowed.
            ast::Stmt::Pass(_) => continue,
            // If statements are allowed; the body statements must validate.
            ast::Stmt::If(if_stmt) => {
                validate_typed_dict_class_body_statements(context, &if_stmt.body);
                for elif_else_clause in &if_stmt.elif_else_clauses {
                    validate_typed_dict_class_body_statements(context, &elif_else_clause.body);
                }
                continue;
            }
            ast::Stmt::Expr(expr) => {
                // Docstrings are allowed.
                if matches!(*expr.value, ast::Expr::StringLiteral(_)) {
                    continue;
                }
                // As a non-standard but common extension, we also interpret `...` as
                // equivalent to `pass`.
                if matches!(*expr.value, ast::Expr::EllipsisLiteral(_)) {
                    continue;
                }
            }
            // Everything else is forbidden.
            _ => {}
        }
        if let Some(builder) = context.report_lint(&INVALID_TYPED_DICT_STATEMENT, stmt) {
            if matches!(stmt, ast::Stmt::FunctionDef(_)) {
                builder.into_diagnostic(format_args!("TypedDict class cannot have methods"));
            } else {
                let mut diagnostic = builder
                    .into_diagnostic(format_args!("invalid statement in TypedDict class body"));
                diagnostic.info("Only annotated declarations (`<name>: <type>`) are allowed.");
            }
        }
    }
}

fn validate_typed_dict_field_overrides<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    direct_bases: &[ClassType<'db>],
) {
    let db = context.db();
    let ctx = context.semantic_context();
    let child_fields = TypedDictType::new(class.identity_specialization(db)).items(db);
    let own_fields = class.own_fields(db, None, CodeGeneratorKind::TypedDict);
    let mut reported_fields = FxHashSet::default();

    for base in direct_bases {
        for (field_name, base_field) in TypedDictType::new(*base).items(db) {
            let Some(child_field) = child_fields.get(field_name.as_str()) else {
                continue;
            };

            let Some(reason) =
                TypedDictFieldOverrideReason::from_fields(&ctx, child_field, base_field)
            else {
                continue;
            };

            if !reported_fields.insert(field_name.clone()) {
                continue;
            }

            let own_field_definition = own_fields
                .get(field_name.as_str())
                .and_then(|field| field.first_declaration);
            let inherited_field_definition = own_field_definition
                .is_none()
                .then(|| child_field.first_declaration())
                .flatten();

            report_typed_dict_field_override(
                context,
                class,
                field_name.as_str(),
                reason,
                base.name(db),
                base_field.first_declaration(),
                own_field_definition,
                inherited_field_definition,
            );
        }
    }
}

fn validate_typed_dict_openness<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    direct_bases: &[ClassType<'db>],
) {
    let db = context.db();
    let ctx = context.semantic_context();
    let child = TypedDictType::new(class.identity_specialization(db));
    let child_openness = child.openness(db);
    let child_items = child.items(db);

    if let Some(arguments) = class_node.arguments.as_deref()
        && arguments.find_keyword("closed").is_some()
        && arguments.find_keyword("extra_items").is_some()
    {
        report_invalid_typed_dict_openness(
            context,
            class,
            "`closed` and `extra_items` cannot both be specified",
        );
    }

    for base in direct_bases {
        let base_typed_dict = TypedDictType::new(*base);
        let base_openness = base_typed_dict.openness(db);
        let base_items = base_typed_dict.items(db);

        match base_openness {
            TypedDictOpenness::ImplicitlyOpen => {}
            TypedDictOpenness::Closed => {
                if !child_openness.is_closed() {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "TypedDict `{}` must remain closed because base `{}` is closed",
                            class.name(db),
                            base.name(db),
                        ),
                    );
                    continue;
                }

                if let Some((field_name, _)) = child_items
                    .iter()
                    .find(|(field_name, _)| !base_items.contains_key(*field_name))
                {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "Cannot add item `{field_name}` to closed TypedDict base `{}`",
                            base.name(db),
                        ),
                    );
                }
            }
            TypedDictOpenness::Extra(base_extra_items) if base_extra_items.is_read_only() => {
                match child_openness {
                    TypedDictOpenness::ImplicitlyOpen => {
                        report_invalid_typed_dict_openness(
                            context,
                            class,
                            format_args!(
                                "TypedDict `{}` cannot be open because base `{}` has extra items",
                                class.name(db),
                                base.name(db),
                            ),
                        );
                        continue;
                    }
                    TypedDictOpenness::Closed => {}
                    TypedDictOpenness::Extra(child_extra_items) => {
                        if !child_extra_items
                            .declared_ty
                            .is_assignable_to(&ctx, base_extra_items.declared_ty)
                        {
                            report_invalid_typed_dict_openness(
                                context,
                                class,
                                format_args!(
                                    "Extra items type `{}` is not assignable to `{}` from base `{}`",
                                    child_extra_items.declared_ty.display(&ctx),
                                    base_extra_items.declared_ty.display(&ctx),
                                    base.name(db),
                                ),
                            );
                            continue;
                        }
                    }
                }

                if let Some((field_name, field)) = child_items.iter().find(|(field_name, field)| {
                    !base_items.contains_key(*field_name)
                        && !field
                            .declared_ty
                            .is_assignable_to(&ctx, base_extra_items.declared_ty)
                }) {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "Item `{field_name}` of type `{}` is not assignable to extra items type `{}` from base `{}`",
                            field.declared_ty.display(&ctx),
                            base_extra_items.declared_ty.display(&ctx),
                            base.name(db),
                        ),
                    );
                }
            }
            TypedDictOpenness::Extra(base_extra_items) => {
                let Some(child_extra_items) = child_openness.explicit_extra_items() else {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "TypedDict `{}` must preserve mutable extra items from base `{}`",
                            class.name(db),
                            base.name(db),
                        ),
                    );
                    continue;
                };

                if child_extra_items.is_read_only()
                    || !child_extra_items
                        .declared_ty
                        .is_assignable_to(&ctx, base_extra_items.declared_ty)
                    || !base_extra_items
                        .declared_ty
                        .is_assignable_to(&ctx, child_extra_items.declared_ty)
                {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "TypedDict `{}` must preserve mutable extra items type `{}` from base `{}`",
                            class.name(db),
                            base_extra_items.declared_ty.display(&ctx),
                            base.name(db),
                        ),
                    );
                    continue;
                }

                if let Some((field_name, _)) = child_items.iter().find(|(field_name, field)| {
                    !base_items.contains_key(*field_name)
                        && (field.is_required()
                            || field.is_read_only()
                            || !field
                                .declared_ty
                                .is_assignable_to(&ctx, base_extra_items.declared_ty)
                            || !base_extra_items
                                .declared_ty
                                .is_assignable_to(&ctx, field.declared_ty))
                }) {
                    report_invalid_typed_dict_openness(
                        context,
                        class,
                        format_args!(
                            "Item `{field_name}` must be mutable, not required, and consistent with extra items type `{}` from base `{}`",
                            base_extra_items.declared_ty.display(&ctx),
                            base.name(db),
                        ),
                    );
                }
            }
        }
    }
}

fn report_invalid_typed_dict_openness(
    context: &InferContext<'_, '_>,
    class: StaticClassLiteral<'_>,
    message: impl std::fmt::Display,
) {
    if let Some(builder) =
        context.report_lint(&INVALID_TYPED_DICT_HEADER, class.header_range(context.db()))
    {
        builder.into_diagnostic(message);
    }
}

#[derive(Clone, Copy)]
enum TypedDictFieldOverrideReason<'db> {
    /// A required inherited field was relaxed to `NotRequired`.
    RequiredFieldMadeNotRequired,
    /// A mutable inherited field was redeclared as read-only.
    MutableFieldMadeReadOnly,
    /// A mutable inherited `NotRequired` field was made required.
    MutableNotRequiredFieldMadeRequired,
    /// A read-only inherited field's new type is not assignable to the base type.
    ReadOnlyTypeNotAssignable {
        ctx: SemanticContext<'db>,
        child_ty: Type<'db>,
        base_ty: Type<'db>,
    },
    /// A mutable inherited field's new type is not mutually assignable with the base type.
    MutableTypeIncompatible {
        ctx: SemanticContext<'db>,
        child_ty: Type<'db>,
        base_ty: Type<'db>,
    },
}

impl std::fmt::Display for TypedDictFieldOverrideReason<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredFieldMadeNotRequired => {
                write!(
                    f,
                    "Required inherited fields cannot be redeclared as `NotRequired`"
                )
            }
            Self::MutableFieldMadeReadOnly => {
                write!(
                    f,
                    "Mutable inherited fields cannot be redeclared as read-only"
                )
            }
            Self::MutableNotRequiredFieldMadeRequired => {
                write!(
                    f,
                    "Mutable inherited `NotRequired` fields cannot be redeclared as required"
                )
            }
            Self::ReadOnlyTypeNotAssignable {
                ctx,
                child_ty,
                base_ty,
            } => write!(
                f,
                "Inherited read-only field type `{}` is not assignable from `{}`",
                base_ty.display(ctx),
                child_ty.display(ctx),
            ),
            Self::MutableTypeIncompatible {
                ctx,
                child_ty,
                base_ty,
            } => write!(
                f,
                "Inherited mutable field type `{}` is incompatible with `{}`",
                base_ty.display(ctx),
                child_ty.display(ctx),
            ),
        }
    }
}

impl<'db> TypedDictFieldOverrideReason<'db> {
    fn from_fields(
        ctx: &SemanticContext<'db>,
        child_field: &TypedDictField<'db>,
        base_field: &TypedDictField<'db>,
    ) -> Option<Self> {
        if base_field.is_required() && !child_field.is_required() {
            return Some(Self::RequiredFieldMadeNotRequired);
        }

        if !base_field.is_read_only() {
            if child_field.is_read_only() {
                return Some(Self::MutableFieldMadeReadOnly);
            }

            if !base_field.is_required() && child_field.is_required() {
                return Some(Self::MutableNotRequiredFieldMadeRequired);
            }
        }

        let types_are_compatible = if base_field.is_read_only() {
            child_field
                .declared_ty
                .is_assignable_to(ctx, base_field.declared_ty)
        } else {
            child_field
                .declared_ty
                .is_assignable_to(ctx, base_field.declared_ty)
                && base_field
                    .declared_ty
                    .is_assignable_to(ctx, child_field.declared_ty)
        };

        if types_are_compatible {
            return None;
        }

        Some(if base_field.is_read_only() {
            Self::ReadOnlyTypeNotAssignable {
                ctx: *ctx,
                child_ty: child_field.declared_ty,
                base_ty: base_field.declared_ty,
            }
        } else {
            Self::MutableTypeIncompatible {
                ctx: *ctx,
                child_ty: child_field.declared_ty,
                base_ty: base_field.declared_ty,
            }
        })
    }
}

#[expect(clippy::too_many_arguments)]
fn report_typed_dict_field_override<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    field_name: &str,
    reason: TypedDictFieldOverrideReason<'db>,
    base_name: &str,
    base_definition: Option<Definition<'db>>,
    own_field_definition: Option<Definition<'db>>,
    inherited_field_definition: Option<Definition<'db>>,
) {
    let db = context.db();
    let builder = if let Some(definition) = own_field_definition {
        context.report_lint(
            &INVALID_TYPED_DICT_FIELD,
            definition.full_range(db, context.module()),
        )
    } else {
        context.report_lint(&INVALID_TYPED_DICT_FIELD, class.header_range(db))
    };
    let Some(builder) = builder else {
        return;
    };

    let mut diagnostic = if own_field_definition.is_some() {
        builder.into_diagnostic(format_args!(
            "Cannot overwrite TypedDict field `{field_name}`"
        ))
    } else {
        builder.into_diagnostic(format_args!(
            "Cannot overwrite TypedDict field `{field_name}` while merging base classes"
        ))
    };

    diagnostic.set_primary_message(format_args!("{reason}"));

    if own_field_definition.is_none() {
        add_definition_subdiagnostic(
            db,
            &mut diagnostic,
            inherited_field_definition,
            format_args!("Field `{field_name}` already inherited from another base here"),
        );
    }

    add_definition_subdiagnostic(
        db,
        &mut diagnostic,
        base_definition,
        format_args!("Inherited field `{field_name}` declared here on base `{base_name}`"),
    );
}

fn add_definition_subdiagnostic<'db>(
    db: &'db dyn Db,
    diagnostic: &mut Diagnostic,
    definition: Option<Definition<'db>>,
    message: impl std::fmt::Display,
) {
    let Some(definition) = definition else {
        return;
    };

    let file = definition.file(db);
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Field declaration");
    sub.annotate(
        Annotation::secondary(
            Span::from(file).with_range(definition.full_range(db, &module).range()),
        )
        .message(message),
    );
    diagnostic.sub(sub);
}
