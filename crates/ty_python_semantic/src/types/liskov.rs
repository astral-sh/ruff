//! Checks relating to the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

use ruff_db::diagnostic::Annotation;
use rustc_hash::FxHashSet;

use crate::{
    place::Place,
    semantic_index::{place_table, use_def_map},
    types::{
        ClassBase, ClassLiteral, ClassType, KnownClass, Type,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_EXPLICIT_OVERRIDE, report_invalid_method_override,
            report_overridden_final_method,
        },
        function::{FunctionDecorators, KnownFunction},
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

    let Some(definition) = definition else {
        return;
    };

    let (literal, specialization) = class.class_literal(db);
    let class_kind = CodeGeneratorKind::from_class(db, literal, specialization);

    let Place::Defined(type_on_subclass_instance, _, _) =
        Type::instance(db, class).member(db, &member.name).place
    else {
        return;
    };

    let mut subclass_overrides_superclass_declaration = false;
    let mut has_dynamic_superclass = false;
    let mut has_typeddict_in_mro = false;
    let mut liskov_diagnostic_emitted = false;
    let mut overridden_final_method = None;

    for class_base in class.iter_mro(db).skip(1) {
        let superclass = match class_base {
            ClassBase::Protocol | ClassBase::Generic => continue,
            ClassBase::Dynamic(_) => {
                has_dynamic_superclass = true;
                continue;
            }
            ClassBase::TypedDict => {
                has_typeddict_in_mro = true;
                continue;
            }
            ClassBase::Class(class) => class,
        };

        let (superclass_literal, superclass_specialization) = superclass.class_literal(db);
        let superclass_scope = superclass_literal.body_scope(db);
        let superclass_symbol_table = place_table(db, superclass_scope);
        let superclass_symbol_id = superclass_symbol_table.symbol_id(&member.name);

        let mut method_kind = MethodKind::default();

        // If the member is not defined on the class itself, skip it
        if let Some(id) = superclass_symbol_id {
            let superclass_symbol = superclass_symbol_table.symbol(id);
            if !(superclass_symbol.is_bound() || superclass_symbol.is_declared()) {
                continue;
            }
        } else {
            if superclass_literal
                .own_synthesized_member(db, superclass_specialization, None, &member.name)
                .is_none()
            {
                continue;
            }
            method_kind =
                CodeGeneratorKind::from_class(db, superclass_literal, superclass_specialization)
                    .map(MethodKind::Synthesized)
                    .unwrap_or_default();
        }

        subclass_overrides_superclass_declaration = true;

        let Place::Defined(superclass_type, _, _) = Type::instance(db, superclass)
            .member(db, &member.name)
            .place
        else {
            // If not defined on any superclass, no point in continuing to walk up the MRO
            break;
        };

        overridden_final_method = overridden_final_method.or_else(|| {
            let superclass_symbol_id = superclass_symbol_id?;

            // TODO: `@final` should be more like a type qualifier:
            // we should also recognise `@final`-decorated methods that don't end up
            // as being function- or property-types (because they're wrapped by other
            // decorators that transform the type into something else).
            let underlying_function = match superclass
                .own_class_member(db, None, &member.name)
                .ignore_possibly_undefined()?
            {
                Type::BoundMethod(method) => method.function(db),
                Type::FunctionLiteral(function) => function,
                Type::PropertyInstance(property) => property.getter(db)?.as_function_literal()?,
                _ => return None,
            };

            if underlying_function.has_known_decorator(db, FunctionDecorators::FINAL) {
                use_def_map(db, superclass_scope)
                    .end_of_scope_symbol_bindings(superclass_symbol_id)
                    .next()?
                    .binding
                    .definition()?
                    .kind(db)
                    .is_function_def()
                    .then_some((superclass, underlying_function))
            } else {
                None
            }
        });

        // **********************************************************
        // Everything below this point in the loop
        // is about Liskov Substitution Principle checks
        // **********************************************************

        // Only one Liskov diagnostic should be emitted per each invalid override,
        // even if it overrides multiple superclasses incorrectly!
        if liskov_diagnostic_emitted {
            continue;
        }

        // TODO: Check Liskov on non-methods too
        let Type::FunctionLiteral(subclass_function) = member.ty else {
            continue;
        };

        // Constructor methods are not checked for Liskov compliance
        if matches!(
            &*member.name,
            "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
        ) {
            continue;
        }

        // Synthesized `__replace__` methods on dataclasses are not checked
        if &member.name == "__replace__"
            && matches!(class_kind, Some(CodeGeneratorKind::DataclassLike(_)))
        {
            continue;
        }

        let Some(superclass_type_as_callable) = superclass_type
            .try_upcast_to_callable(db)
            .map(|callables| callables.into_type(db))
        else {
            continue;
        };

        if type_on_subclass_instance.is_assignable_to(db, superclass_type_as_callable) {
            continue;
        }

        report_invalid_method_override(
            context,
            &member.name,
            class,
            *definition,
            subclass_function,
            superclass,
            superclass_type,
            method_kind,
        );

        liskov_diagnostic_emitted = true;
    }

    if !subclass_overrides_superclass_declaration && !has_dynamic_superclass {
        if has_typeddict_in_mro {
            if !KnownClass::TypedDictFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        } else if class_kind == Some(CodeGeneratorKind::NamedTuple) {
            if !KnownClass::NamedTupleFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        }
    }

    if !subclass_overrides_superclass_declaration
        && !has_dynamic_superclass
        && definition.kind(db).is_function_def()
        && let Type::FunctionLiteral(function) = member.ty
        && function.has_known_decorator(db, FunctionDecorators::OVERRIDE)
    {
        let function_literal = if context.in_stub() {
            function.first_overload(db)
        } else {
            function.literal(db).last_definition(db)
        };

        if let Some(builder) = context.report_lint(
            &INVALID_EXPLICIT_OVERRIDE,
            function_literal.focus_range(db, context.module()),
        ) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Method `{}` is decorated with `@override` but does not override anything",
                member.name
            ));
            if let Some(decorator_span) =
                function_literal.find_known_decorator_span(db, KnownFunction::Override)
            {
                diagnostic.annotate(Annotation::secondary(decorator_span));
            }
            diagnostic.info(format_args!(
                "No `{member}` definitions were found on any superclasses of `{class}`",
                member = &member.name,
                class = class.name(db)
            ));
        }
    }

    if let Some((superclass, superclass_method)) = overridden_final_method {
        report_overridden_final_method(
            context,
            &member.name,
            *definition,
            superclass,
            class,
            superclass_method,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum MethodKind<'db> {
    Synthesized(CodeGeneratorKind<'db>),
    #[default]
    NotSynthesized,
}
