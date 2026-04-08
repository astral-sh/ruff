use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;

use crate::{
    Db,
    semantic_index::definition::Definition,
    types::{
        ClassType, StaticClassLiteral, Type, TypedDictType,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{INVALID_TYPED_DICT_FIELD, INVALID_TYPED_DICT_STATEMENT},
        typed_dict::TypedDictField,
    },
};

pub(super) fn validate_typed_dict_class<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    direct_bases: &[ClassType<'db>],
) {
    validate_typed_dict_class_body(context, class_node);
    validate_typed_dict_field_overrides(context, class, direct_bases);
}

fn validate_typed_dict_class_body(context: &InferContext<'_, '_>, class_node: &ast::StmtClassDef) {
    // Check that a class-based `TypedDict` doesn't include any invalid statements:
    // https://typing.python.org/en/latest/spec/typeddict.html#class-based-syntax
    //
    //     The body of the class definition defines the items of the `TypedDict` type. It
    //     may also contain a docstring or pass statements (primarily to allow the creation
    //     of an empty `TypedDict`). No other statements are allowed, and type checkers
    //     should report an error if any are present.
    for stmt in &class_node.body {
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
    let child_fields = TypedDictType::new(class.identity_specialization(db)).items(db);
    let own_fields = class.own_fields(db, None, CodeGeneratorKind::TypedDict);
    let mut reported_fields = FxHashSet::default();

    for base in direct_bases {
        for (field_name, base_field) in TypedDictType::new(*base).items(db) {
            let Some(child_field) = child_fields.get(field_name.as_str()) else {
                continue;
            };

            let Some(reason) =
                TypedDictFieldOverrideReason::from_fields(db, child_field, base_field)
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
        db: &'db dyn Db,
        child_ty: Type<'db>,
        base_ty: Type<'db>,
    },
    /// A mutable inherited field's new type is not mutually assignable with the base type.
    MutableTypeIncompatible {
        db: &'db dyn Db,
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
                db,
                child_ty,
                base_ty,
            } => write!(
                f,
                "Inherited read-only field type `{}` is not assignable from `{}`",
                base_ty.display(*db),
                child_ty.display(*db),
            ),
            Self::MutableTypeIncompatible {
                db,
                child_ty,
                base_ty,
            } => write!(
                f,
                "Inherited mutable field type `{}` is incompatible with `{}`",
                base_ty.display(*db),
                child_ty.display(*db),
            ),
        }
    }
}

impl<'db> TypedDictFieldOverrideReason<'db> {
    fn from_fields(
        db: &'db dyn Db,
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
                .is_assignable_to(db, base_field.declared_ty)
        } else {
            child_field
                .declared_ty
                .is_assignable_to(db, base_field.declared_ty)
                && base_field
                    .declared_ty
                    .is_assignable_to(db, child_field.declared_ty)
        };

        if types_are_compatible {
            return None;
        }

        Some(if base_field.is_read_only() {
            Self::ReadOnlyTypeNotAssignable {
                db,
                child_ty: child_field.declared_ty,
                base_ty: base_field.declared_ty,
            }
        } else {
            Self::MutableTypeIncompatible {
                db,
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
    let module = parsed_module(db, file).load(db);
    let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Field declaration");
    sub.annotate(
        Annotation::secondary(
            Span::from(file).with_range(definition.full_range(db, &module).range()),
        )
        .message(message),
    );
    diagnostic.sub(sub);
}
