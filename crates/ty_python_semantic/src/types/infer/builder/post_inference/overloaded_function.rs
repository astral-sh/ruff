use ruff_db::diagnostic::Annotation;
use rustc_hash::FxHashSet;

use crate::{
    place::{DefinedPlace, Definedness, Place, place_from_bindings},
    semantic_index::{
        SemanticIndex, definition::Definition, place::ScopedPlaceId, scope::NodeWithScopeKind,
    },
    types::{
        KnownClass, Type, binding_type,
        context::InferContext,
        diagnostic::INVALID_OVERLOAD,
        function::{FunctionDecorators, FunctionType, KnownFunction},
    },
};

/// Check the overloaded functions in this scope.
///
/// This only checks the overloaded functions that are:
/// 1. Visible publicly at the end of this scope
/// 2. Or, defined and called in this scope
///
/// For (1), this has the consequence of not checking an overloaded function that is being
/// shadowed by another function with the same name in this scope.
pub(crate) fn check_overloaded_function<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    definition: Definition<'db>,
    scope: &NodeWithScopeKind,
    index: &SemanticIndex<'db>,
    seen_overloaded_places: &mut FxHashSet<ScopedPlaceId>,
    seen_public_functions: &mut FxHashSet<FunctionType<'db>>,
) {
    // Collect all the unique overloaded function places in this scope. This requires a set
    // because an overloaded function uses the same place for each of the overloads and the
    // implementation.
    let Type::FunctionLiteral(function) = ty else {
        return;
    };

    let db = context.db();

    if function.file(db) != context.file() {
        // If the function is not in this file, we don't need to check it.
        // https://github.com/astral-sh/ruff/pull/17609#issuecomment-2839445740
        return;
    }

    if !function.has_known_decorator(db, FunctionDecorators::OVERLOAD) {
        return;
    }

    let place = definition.place(db);

    if !seen_overloaded_places.insert(place) {
        // We have already checked this overloaded function in this scope, so we can skip it.
        return;
    }

    let use_def = index.use_def_map(context.scope().file_scope_id(db));

    let Place::Defined(DefinedPlace {
        ty: Type::FunctionLiteral(function),
        definedness: Definedness::AlwaysDefined,
        ..
    }) = place_from_bindings(
        db,
        use_def.end_of_scope_symbol_bindings(place.as_symbol().unwrap()),
    )
    .place
    else {
        return;
    };

    if !seen_public_functions.insert(function) {
        // We have already checked this overloaded function as a public function, so we can skip it.
        return;
    }

    let (overloads, implementation) = function.overloads_and_implementation(db);
    if overloads.is_empty() {
        return;
    }

    // Check that the overloaded function has at least two overloads
    if let [single_overload] = overloads {
        let function_node = single_overload.node(db, context.file(), context.module());
        if let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Overloaded function `{}` requires at least two overloads",
                &function_node.name
            ));
            diagnostic.set_primary_message("Only one overload defined here");
            if let Some(decorator) =
                single_overload.find_known_decorator_span(db, KnownFunction::Overload)
            {
                diagnostic.annotate(Annotation::secondary(decorator));
            }
        }
    }

    // Check that the overloaded function has an implementation. Overload definitions
    // within stub files, protocols, and on abstract methods within abstract base classes
    // are exempt from this check.
    if implementation.is_none() && !context.in_stub() {
        let mut implementation_required = true;

        if function
            .iter_overloads_and_implementation(db)
            .all(|f| f.body_scope(db).scope(db).in_type_checking_block())
        {
            implementation_required = false;
        } else if let NodeWithScopeKind::Class(class_node_ref) = scope {
            let class = binding_type(
                db,
                index.expect_single_definition(class_node_ref.node(context.module())),
            )
            .expect_class_literal();

            if class.is_protocol(db)
                || (Type::ClassLiteral(class)
                    .is_subtype_of(db, KnownClass::ABCMeta.to_instance(db))
                    && overloads.iter().all(|overload| {
                        overload.has_known_decorator(db, FunctionDecorators::ABSTRACT_METHOD)
                    }))
            {
                implementation_required = false;
            }
        }

        if implementation_required {
            let function_node = overloads[0].node(db, context.file(), context.module());
            if let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Overloads for function `{}` must be followed by a \
                    non-`@overload`-decorated implementation function",
                    &function_node.name
                ));
                diagnostic.info(format_args!(
                    "Attempting to call `{}` will raise `TypeError` at runtime",
                    &function_node.name
                ));
                diagnostic.info("Overloaded functions without implementations are only permitted:");
                diagnostic.info(" - in stub files");
                diagnostic.info(" - in `if TYPE_CHECKING` blocks");
                diagnostic.info(" - as methods on protocol classes");
                diagnostic.info(" - or as `@abstractmethod`-decorated methods on abstract classes");
                diagnostic.info(
                    "See https://docs.python.org/3/library/typing.html#typing.overload \
                            for more details",
                );
            }
        }
    }

    for (decorator, name) in [
        (FunctionDecorators::CLASSMETHOD, "classmethod"),
        (FunctionDecorators::STATICMETHOD, "staticmethod"),
    ] {
        let mut decorator_present = false;
        let mut decorator_missing = vec![];

        for function in overloads.iter().chain(implementation.as_ref()) {
            if function.has_known_decorator(db, decorator) {
                decorator_present = true;
            } else {
                decorator_missing.push(function);
            }
        }

        if !decorator_present {
            // Both overloads and implementation does not have the decorator
            continue;
        }
        if decorator_missing.is_empty() {
            // All overloads and implementation have the decorator
            continue;
        }

        let function_node = function.node(db, context.file(), context.module());
        if let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Overloaded function `{}` does not use the `@{name}` decorator \
                    consistently",
                &function_node.name
            ));
            for function in decorator_missing {
                diagnostic.annotate(
                    context
                        .secondary(function.focus_range(db, context.module()))
                        .message(format_args!("Missing here")),
                );
                if let Some(decorator) =
                    function.find_known_decorator_span(db, KnownFunction::Overload)
                {
                    diagnostic.annotate(Annotation::secondary(decorator));
                }
            }
        }
    }

    for (function, decorator) in [
        (KnownFunction::Final, FunctionDecorators::FINAL),
        (KnownFunction::Override, FunctionDecorators::OVERRIDE),
    ] {
        if let Some(implementation) = implementation {
            for overload in overloads {
                if !overload.has_known_decorator(db, decorator) {
                    continue;
                }
                let function_node = overload.node(db, context.file(), context.module());
                let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name)
                else {
                    continue;
                };
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "`@{name}` decorator should be applied only to the \
                        overload implementation",
                    name = function.name()
                ));
                for function in [function, KnownFunction::Overload] {
                    if let Some(decorator) = overload.find_known_decorator_span(db, function) {
                        diagnostic.annotate(Annotation::secondary(decorator));
                    }
                }
                diagnostic.annotate(
                    context
                        .secondary(implementation.focus_range(db, context.module()))
                        .message(format_args!("Implementation defined here")),
                );
            }
        } else {
            let mut overloads = overloads.iter();
            let Some(first_overload) = overloads.next() else {
                continue;
            };
            for overload in overloads {
                if !overload.has_known_decorator(db, decorator) {
                    continue;
                }
                let function_node = overload.node(db, context.file(), context.module());
                let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name)
                else {
                    continue;
                };
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "`@{name}` decorator should be applied only to the \
                        first overload",
                    name = function.name()
                ));
                if let Some(decorator) = overload.find_known_decorator_span(db, function) {
                    diagnostic.annotate(Annotation::secondary(decorator));
                }
                diagnostic.annotate(
                    context
                        .secondary(first_overload.focus_range(db, context.module()))
                        .message(format_args!("First overload defined here")),
                );
                if let Some(decorator) =
                    first_overload.find_known_decorator_span(db, KnownFunction::Overload)
                {
                    diagnostic.annotate(Annotation::secondary(decorator));
                }
            }
        }
    }
}
