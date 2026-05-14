use ruff_db::{
    diagnostic::{Annotation, Span},
    parsed::parsed_module,
};
use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;

use crate::{
    Db,
    place::{DefinedPlace, Definedness, Place, place_from_bindings},
    types::{
        KnownClass, Type, binding_type,
        context::InferContext,
        diagnostic::INVALID_OVERLOAD,
        function::{FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral},
        signatures::{ParameterConsistency, ReturnTypeConsistency},
    },
};
use ty_python_core::{
    SemanticIndex, definition::Definition, place::ScopedPlaceId, scope::NodeWithScopeKind,
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

    let binding_decorator_inconsistencies =
        binding_decorator_inconsistencies(db, overloads, implementation.as_ref());

    if let Some(implementation) = implementation
        && binding_decorator_inconsistencies.is_empty()
    {
        check_non_generic_overload_implementation_consistency(context, overloads, implementation);
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

        if function.iter_overloads_and_implementation(db).all(|f| {
            index.is_in_type_checking_block(
                f.body_scope(db).file_scope_id(db),
                f.node(db, context.file(), context.module()).range(),
            )
        }) {
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

    for inconsistency in binding_decorator_inconsistencies {
        let function_node = function.node(db, context.file(), context.module());
        if let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Overloaded function `{}` does not use the `@{}` decorator \
                    consistently",
                &function_node.name, inconsistency.decorator_name
            ));
            for function in inconsistency.missing {
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

    for (known_function, decorator) in [
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
                    name = known_function.name()
                ));
                for known_function in [known_function, KnownFunction::Overload] {
                    if let Some(decorator) = overload.find_known_decorator_span(db, known_function)
                    {
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
                    name = known_function.name()
                ));
                if let Some(decorator) = overload.find_known_decorator_span(db, known_function) {
                    diagnostic.annotate(Annotation::secondary(decorator));
                }
                let file = function.file(db);
                let module = parsed_module(db, file).load(db);
                let node = first_overload.node(db, file, &module);
                let span = if node.body.len() == 1 {
                    Span::from(file).with_range(node.range())
                } else {
                    first_overload.spans(db).decorators_and_header
                };
                diagnostic.annotate(
                    Annotation::secondary(span)
                        .message(format_args!("First overload defined here")),
                );
            }
        }
    }
}

/// Check non-generic overload signatures against their implementation.
///
/// This is the first, deliberately narrow pass at overload implementation consistency. It reports
/// only when the overloads and implementation are all non-generic; generic signatures require
/// careful treatment of type-variable domains.
fn check_non_generic_overload_implementation_consistency<'db>(
    context: &InferContext<'db, '_>,
    overloads: &'db [OverloadLiteral<'db>],
    implementation: OverloadLiteral<'db>,
) {
    if !context.is_lint_enabled(&INVALID_OVERLOAD) {
        return;
    }

    let db = context.db();
    let implementation_signature = implementation.signature(db);

    // TODO: Remove this temporary non-generic restriction once overload implementation consistency
    // handles type-variable domains.
    if !implementation_signature.is_non_generic() {
        return;
    }

    let overload_signatures = overloads
        .iter()
        .map(|overload| (overload, overload.signature(db)));

    if overload_signatures
        .clone()
        .any(|(_, signature)| !signature.is_non_generic())
    {
        return;
    }

    for (overload, overload_signature) in overload_signatures {
        let function_node = overload.node(db, context.file(), context.module());
        let parameter_consistency = implementation_signature
            .non_generic_implementation_parameters_consistency_with(db, &overload_signature);
        let return_type_consistency = implementation_signature
            .non_generic_implementation_return_type_consistency_with(db, &overload_signature);

        let (parameter_error_context, return_type_error_context, message) =
            match (parameter_consistency, return_type_consistency) {
                (ParameterConsistency::Consistent, ReturnTypeConsistency::Consistent) => continue,
                (
                    ParameterConsistency::Inconsistent(error_context),
                    ReturnTypeConsistency::Consistent,
                ) => (
                    Some(error_context),
                    None,
                    "Implementation does not accept all arguments of this overload",
                ),
                (
                    ParameterConsistency::Consistent,
                    ReturnTypeConsistency::Inconsistent(error_context),
                ) => (
                    None,
                    Some(error_context),
                    "Overload return type is not assignable to implementation return type",
                ),
                (
                    ParameterConsistency::Inconsistent(parameter_error_context),
                    ReturnTypeConsistency::Inconsistent(return_type_error_context),
                ) => (
                    Some(parameter_error_context),
                    Some(return_type_error_context),
                    "Overload signature is not consistent with implementation",
                ),
            };

        let Some(builder) = context.report_lint(&INVALID_OVERLOAD, &function_node.name) else {
            continue;
        };
        let mut diagnostic = builder.into_diagnostic(format_args!("{message}"));
        if let Some(error_context) = parameter_error_context {
            diagnostic.info(format_args!(
                "Implementation signature `{}` is not assignable to overload signature `{}`",
                implementation_signature.display(db),
                overload_signature.display(db),
            ));
            error_context.attach_to(db, &mut diagnostic);
        }
        if let Some(error_context) = return_type_error_context {
            diagnostic.info(format_args!(
                "Overload returns `{}`, which is not assignable to implementation return type `{}`",
                overload_signature.return_ty.display(db),
                implementation_signature.return_ty.display(db),
            ));
            error_context.attach_to(db, &mut diagnostic);
        }
        diagnostic.annotate(
            context
                .secondary(implementation.focus_range(db, context.module()))
                .message(format_args!("Implementation defined here")),
        );
    }
}

/// A decorator that is applied inconsistently across an overload set.
struct BindingDecoratorInconsistency<'db, 'a> {
    /// The user-facing name of the decorator, without the leading `@`.
    decorator_name: &'static str,
    /// The overloads or implementation that are missing this decorator.
    missing: Vec<&'a OverloadLiteral<'db>>,
}

/// Finds binding-affecting decorator inconsistencies across an overload set.
///
/// `@classmethod` and `@staticmethod` affect the callable shape used for overload
/// implementation consistency checks. This returns a value for each decorator that appears on at
/// least one overload or implementation, but is missing from another.
///
/// For example, given:
///
/// ```py
/// @overload
/// @staticmethod
/// def f(x: int) -> int: ...
///
/// @overload
/// def f(x: str) -> str: ...
///
/// def f(x: int | str) -> int | str: ...
/// ```
///
/// this returns one `staticmethod` inconsistency whose `missing` entries are the second overload
/// and the implementation.
fn binding_decorator_inconsistencies<'db, 'a>(
    db: &dyn Db,
    overloads: &'a [OverloadLiteral<'db>],
    implementation: Option<&'a OverloadLiteral<'db>>,
) -> Vec<BindingDecoratorInconsistency<'db, 'a>> {
    const DECORATORS: [(FunctionDecorators, &str); 2] = [
        (FunctionDecorators::CLASSMETHOD, "classmethod"),
        (FunctionDecorators::STATICMETHOD, "staticmethod"),
    ];

    let mut inconsistencies = Vec::new();
    for (decorator, decorator_name) in DECORATORS {
        let mut decorator_present = false;
        let mut missing = vec![];

        for function in overloads.iter().chain(implementation) {
            if function.has_known_decorator(db, decorator) {
                decorator_present = true;
            } else {
                missing.push(function);
            }
        }

        if decorator_present && !missing.is_empty() {
            inconsistencies.push(BindingDecoratorInconsistency {
                decorator_name,
                missing,
            });
        }
    }
    inconsistencies
}
