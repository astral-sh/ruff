//! Checks relating to the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;

use crate::{
    place::Place,
    semantic_index::place_table,
    types::{
        ClassBase, ClassLiteral, ClassType, KnownClass, Type,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::report_invalid_method_override,
        ide_support::{MemberWithDefinition, all_declarations_and_bindings},
    },
};

pub(super) fn check_class<'db>(context: &InferContext<'db, '_>, class: ClassLiteral<'db>) {
    let db = context.db();
    if class.is_known(db, KnownClass::Object) {
        return;
    }

    let class_specialized = class.identity_specialization(db);
    let own_class_members: FxHashSet<_> =
        all_declarations_and_bindings(db, class.body_scope(db)).collect();

    for member in own_class_members {
        check_class_declaration(context, class_specialized, &member);
    }
}

fn check_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    member: &MemberWithDefinition<'db>,
) {
    let db = context.db();

    let MemberWithDefinition { member, definition } = member;

    // TODO: Check Liskov on non-methods too
    let Type::FunctionLiteral(function) = member.ty else {
        return;
    };

    let Some(definition) = definition else {
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

        let class_symbol_table = place_table(db, supercls.class_literal(db).0.body_scope(db));

        // If the member is not defined on the class itself, skip it
        let Some(symbol) = class_symbol_table.symbol_by_name(&member.name) else {
            continue;
        };
        if !(symbol.is_bound() || symbol.is_declared()) {
            continue;
        }

        let Some(supercls_as_callable) = type_on_supercls.try_upcast_to_callable(db) else {
            continue;
        };

        if type_on_instance.is_assignable_to(db, supercls_as_callable) {
            continue;
        }

        // If the function was originally defined elsewhere and simply assigned
        // in the body of the class here, we cannot use the range associated with the `FunctionType`
        let range = if definition.kind(db).is_function_def() {
            function
                .literal(db)
                .last_definition(db)
                .spans(db)
                .and_then(|spans| spans.signature.range())
                .unwrap_or_else(|| function.node(db, context.file(), context.module()).range)
        } else {
            definition.full_range(db, context.module()).range()
        };

        report_invalid_method_override(context, range, member, class, supercls, type_on_supercls);

        // Only one diagnostic should be emitted per each invalid override,
        // even if it overrides multiple superclasses incorrectly!
        // It's possible `report_invalid_method_override` didn't emit a diagnostic because there's a
        // suppression comment, but that too should cause us to exit early here.
        break;
    }
}
