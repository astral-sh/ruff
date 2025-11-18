//! Checks relating to the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

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

    // Constructor methods are not checked for Liskov compliance
    if matches!(
        &*member.name,
        "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
    ) {
        return;
    }

    // Synthesized `__replace__` methods on dataclasses are not checked
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

    for superclass in class.iter_mro(db).skip(1).filter_map(ClassBase::into_class) {
        let Place::Defined(superclass_type, _, _) = Type::instance(db, superclass)
            .member(db, &member.name)
            .place
        else {
            // If not defined on any superclass, nothing to check
            break;
        };

        let superclass_symbol_table =
            place_table(db, superclass.class_literal(db).0.body_scope(db));

        // If the member is not defined on the class itself, skip it
        let Some(superclass_symbol) = superclass_symbol_table.symbol_by_name(&member.name) else {
            continue;
        };
        if !(superclass_symbol.is_bound() || superclass_symbol.is_declared()) {
            continue;
        }

        let Some(superclass_type_as_callable) = superclass_type.try_upcast_to_callable(db) else {
            continue;
        };

        if type_on_instance.is_assignable_to(db, superclass_type_as_callable) {
            continue;
        }

        report_invalid_method_override(
            context,
            &member.name,
            class,
            *definition,
            function,
            superclass,
            superclass_type,
        );

        // Only one diagnostic should be emitted per each invalid override,
        // even if it overrides multiple superclasses incorrectly!
        // It's possible `report_invalid_method_override` didn't emit a diagnostic because there's a
        // suppression comment, but that too should cause us to exit early here.
        break;
    }
}
