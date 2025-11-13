use ruff_db::diagnostic::Annotation;
use rustc_hash::FxHashSet;

use crate::{
    place::Place,
    semantic_index::place_table,
    types::{
        ClassBase, ClassLiteral, ClassType, KnownClass, Type,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::INVALID_METHOD_OVERRIDE,
        ide_support::{Member, all_declarations_and_bindings},
    },
};

pub(super) fn check_class<'db>(context: &InferContext<'db, '_>, class: ClassLiteral<'db>) {
    let db = context.db();
    if class.is_known(db, KnownClass::Object) {
        return;
    }
    let class_specialized = class.identity_specialization(db);
    let own_class_members: FxHashSet<_> = all_declarations_and_bindings(db, class.body_scope(db))
        .map(|member| member.member)
        .collect();

    for member in own_class_members {
        check_class_declaration(context, class_specialized, &member);
    }
}

fn check_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    member: &Member<'db>,
) {
    let db = context.db();

    // TODO: Check Liskov on non-methods too
    let Type::FunctionLiteral(function) = member.ty else {
        return;
    };

    // TODO: classmethods and staticmethods
    if function.is_classmethod(db) || function.is_staticmethod(db) {
        return;
    }

    if matches!(
        &*member.name,
        "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
    ) {
        return;
    }

    if &member.name == "__replace__"
        && matches!(
            CodeGeneratorKind::from_class(db, class.class_literal(db).0, None),
            Some(CodeGeneratorKind::DataclassLike(_))
        )
    {
        return;
    }

    let Place::Defined(type_on_instance, _, _) =
        Type::instance(db, class).member(db, &member.name).place
    else {
        return;
    };

    for supercls in class.iter_mro(db).skip(1).filter_map(ClassBase::into_class) {
        let Place::Defined(type_on_supercls, _, _) =
            Type::instance(db, supercls).member(db, &member.name).place
        else {
            // If not defined on any superclass, nothing to check
            break;
        };

        let class_symbol_table = place_table(db, class.class_literal(db).0.body_scope(db));

        // If the member is not defined on the class itself, skip it
        if class_symbol_table.symbol_by_name(&member.name).is_none() {
            continue;
        }

        let Some(supercls_as_callable) = type_on_supercls.try_upcast_to_callable(db) else {
            continue;
        };

        if type_on_instance.is_assignable_to(db, supercls_as_callable) {
            continue;
        }

        let range = function
            .literal(db)
            .last_definition(db)
            .spans(db)
            .and_then(|spans| spans.signature.range())
            .unwrap_or(function.node(db, context.file(), context.module()).range);

        let Some(builder) = context.report_lint(&INVALID_METHOD_OVERRIDE, range) else {
            continue;
        };

        let mut diagnostic =
            builder.into_diagnostic(format_args!("Invalid override of method `{}`", member.name));

        diagnostic.set_primary_message(format_args!(
            "Definition is incompatible with `{}.{}`",
            supercls.name(db),
            member.name
        ));

        diagnostic.info("This violates the Liskov Substitution Principle");

        if let Type::BoundMethod(method_on_supercls) = type_on_supercls
            && let Some(spans) = method_on_supercls
                .function(db)
                .literal(db)
                .last_definition(db)
                .spans(db)
        {
            diagnostic.annotate(Annotation::secondary(spans.signature).message(format_args!(
                "`{}.{}` defined here",
                supercls.name(db),
                member.name
            )));
        }

        // Only one diagnostic should be emitted per each invalid override,
        // even if it overrides multiple superclasses incorrectly!
        break;
    }
}
