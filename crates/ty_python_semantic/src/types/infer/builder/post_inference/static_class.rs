use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
    source::source_text,
};
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;

use crate::attribute_assignments;
use crate::{
    Db, TypeQualifiers,
    diagnostic::format_enumeration,
    place::{place_from_bindings, place_from_declarations},
    types::{
        CallArguments, ClassBase, ClassLiteral, ClassType, GenericAlias, KnownInstanceType,
        MemberLookupPolicy, MetaclassCandidate, Parameters, Signature, SpecialFormType,
        StaticClassLiteral, Type,
        call::Argument,
        class::{
            AbstractMethod, CodeGeneratorKind, FieldKind, MetaclassErrorKind,
            expanded_fixed_length_starred_class_base_tuple,
        },
        context::InferContext,
        definition_expression_type,
        diagnostic::{
            ABSTRACT_METHOD_IN_FINAL_CLASS, CONFLICTING_METACLASS, CYCLIC_CLASS_DEFINITION,
            DATACLASS_FIELD_ORDER, DUPLICATE_KW_ONLY, FINAL_WITHOUT_VALUE, INCONSISTENT_MRO,
            INVALID_ARGUMENT_TYPE, INVALID_BASE, INVALID_DATACLASS, INVALID_GENERIC_CLASS,
            INVALID_GENERIC_ENUM, INVALID_METACLASS, INVALID_NAMED_TUPLE, INVALID_PROTOCOL,
            INVALID_TYPED_DICT_HEADER, IncompatibleBases, SUBCLASS_OF_FINAL_CLASS,
            UNKNOWN_ARGUMENT, report_bad_frozen_dataclass_inheritance,
            report_conflicting_metaclass_from_bases, report_duplicate_bases,
            report_instance_layout_conflict, report_invalid_or_unsupported_base,
            report_invalid_total_ordering, report_invalid_type_param_order,
            report_invalid_typevar_default_reference,
            report_named_tuple_field_with_leading_underscore,
            report_namedtuple_field_without_default_after_field_with_default,
            report_shadowed_type_variable,
            report_subclass_of_class_with_non_callable_init_subclass, report_unsupported_base,
        },
        enums::is_enum_class_by_inheritance,
        function::KnownFunction,
        generics::enclosing_generic_contexts,
        infer::builder::post_inference::typed_dict::validate_typed_dict_class,
        infer_definition_types,
        mro::StaticMroErrorKind,
        overrides,
        tuple::Tuple,
        typevar::TypeVarInstance,
        visitor::find_over_type,
    },
};
use ty_python_core::{SemanticIndex, definition::DefinitionKind, scope::ScopeId};

/// Iterate over all static class definitions (created using `class` statements) to check that
/// the definition is semantically valid and will not cause an exception to be raised at runtime.
/// This needs to be done after most other types in the scope have been inferred, due to the fact
/// that base classes can be deferred. If it looks like a class definition is invalid in some way,
/// issue a diagnostic.
///
/// Note: Dynamic classes created via `type()` calls are checked separately during type
/// inference of the call expression.
///
/// Among the things we check for in this method are whether Python will be able to determine a
/// consistent "[method resolution order]" and [metaclass] for each class.
///
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
/// [metaclass]: https://docs.python.org/3/reference/datamodel.html#metaclasses
pub(crate) fn check_static_class_definitions<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    class_node: &ast::StmtClassDef,
    index: &SemanticIndex<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Type::ClassLiteral(ClassLiteral::Static(class)) = ty else {
        return;
    };

    // Check that the class does not have a cyclic definition
    if let Some(inheritance_cycle) = class.inheritance_cycle(db) {
        if inheritance_cycle.is_participant()
            && let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, class_node)
        {
            builder.into_diagnostic(format_args!(
                "Cyclic definition of `{}` (class cannot inherit from itself)",
                class.name(db)
            ));
        }

        // If a class is cyclically defined, that's a sufficient error to report; the
        // following checks (which are all inheritance-based) aren't even relevant.
        return;
    }

    // Check that the class is not an enum and generic
    if is_enum_class_by_inheritance(db, class) && class.generic_context(db).is_some() {
        if let Some(builder) = context.report_lint(&INVALID_GENERIC_ENUM, class_node) {
            builder.into_diagnostic(format_args!(
                "Enum class `{}` cannot be generic",
                class.name(db)
            ));
        }
    }

    let class_kind = CodeGeneratorKind::from_class(db, class.into(), None);

    // If it's a `NamedTuple` class, check that no field without a default value
    // appears after a field with a default value.
    if class_kind == Some(CodeGeneratorKind::NamedTuple) {
        let mut field_with_default_encountered = None;

        for (field_name, field) in class.own_fields(db, None, CodeGeneratorKind::NamedTuple) {
            if field_name.starts_with('_') {
                report_named_tuple_field_with_leading_underscore(
                    context,
                    class,
                    field_name,
                    field.first_declaration,
                );
            }
            if matches!(
                field.kind,
                FieldKind::NamedTuple {
                    default_ty: Some(_)
                }
            ) {
                field_with_default_encountered =
                    Some((field_name.clone(), field.first_declaration));
            } else if let Some(field_with_default) = field_with_default_encountered.as_ref() {
                report_namedtuple_field_without_default_after_field_with_default(
                    context,
                    class,
                    (field_name, field.first_declaration),
                    field_with_default,
                );
            }
        }
    }

    let is_protocol = class.is_protocol(db);

    if let Some(disjoint_base_decorator) = class_node.decorator_list.iter().find(|decorator| {
        file_expression_type(&decorator.expression)
            .as_function_literal()
            .is_some_and(|function| function.is_known(db, KnownFunction::DisjointBase))
    }) {
        if class_kind == Some(CodeGeneratorKind::TypedDict) {
            if let Some(builder) =
                context.report_lint(&INVALID_TYPED_DICT_HEADER, disjoint_base_decorator)
            {
                builder.into_diagnostic(format_args!(
                    "`@disjoint_base` cannot be used with `TypedDict` class `{}`",
                    class.name(db),
                ));
            }
        } else if is_protocol
            && let Some(builder) = context.report_lint(&INVALID_PROTOCOL, disjoint_base_decorator)
        {
            builder.into_diagnostic(format_args!(
                "`@disjoint_base` cannot be used with protocol class `{}`",
                class.name(db),
            ));
        }
    }

    // Check for invalid `@dataclass` applications.
    if class.dataclass_params(db).is_some() {
        if class.has_named_tuple_class_in_mro(db) {
            if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "`NamedTuple` class `{}` cannot be decorated with `@dataclass`",
                    class.name(db),
                ));
                diagnostic
                    .info("An exception will be raised when instantiating the class at runtime");
            }
        } else if class.is_typed_dict(db) {
            if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "`TypedDict` class `{}` cannot be decorated with `@dataclass`",
                    class.name(db),
                ));
                diagnostic.info(
                    "An exception will often be raised when instantiating the class at runtime",
                );
            }
        } else if is_enum_class_by_inheritance(db, class) {
            if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Enum class `{}` cannot be decorated with `@dataclass`",
                    class.name(db),
                ));
                diagnostic.info("Applying `@dataclass` to an enum is not supported at runtime");
            }
        } else if is_protocol {
            if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Protocol class `{}` cannot be decorated with `@dataclass`",
                    class.name(db),
                ));
                diagnostic.info("Protocols define abstract interfaces and cannot be instantiated");
            }
        }
    }

    let mut disjoint_bases = IncompatibleBases::default();
    let mut protocol_base_with_generic_context = None;
    let mut direct_typed_dict_bases = vec![];

    // Iterate through the class's explicit bases to check for various possible errors:
    //     - Check for inheritance from plain `Generic`,
    //     - Check for inheritance from a `@final` classes
    //     - If the class is a protocol class: check for inheritance from a non-protocol class
    //     - If the class is a NamedTuple class: check for multiple inheritance that isn't `Generic[]`
    for (i, base_class) in class.explicit_bases(db).iter().enumerate() {
        if class_kind == Some(CodeGeneratorKind::NamedTuple)
            && !matches!(
                base_class,
                Type::SpecialForm(SpecialFormType::NamedTuple)
                    | Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_))
            )
        {
            if let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, &class_node.bases()[i])
            {
                builder.into_diagnostic(format_args!(
                    "NamedTuple class `{}` cannot use multiple inheritance except with `Generic[]`",
                    class.name(db),
                ));
            }
        }

        let base_class = match base_class {
            Type::SpecialForm(SpecialFormType::Generic) => {
                if let Some(builder) = context.report_lint(&INVALID_BASE, &class_node.bases()[i]) {
                    // Unsubscripted `Generic` can appear in the MRO of many classes,
                    // but it is never valid as an explicit base class in user code.
                    builder.into_diagnostic("Cannot inherit from plain `Generic`");
                }
                continue;
            }
            Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(new_context)) => {
                let Some((previous_index, previous_context)) = protocol_base_with_generic_context
                else {
                    continue;
                };
                let prior_node = &class_node.bases()[previous_index];
                let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, prior_node) else {
                    continue;
                };
                let mut diagnostic = builder.into_diagnostic(
                    "Cannot both inherit from subscripted `Protocol` \
                                and subscripted `Generic`",
                );
                if let ast::Expr::Subscript(prior_node) = prior_node
                    && new_context == previous_context
                {
                    diagnostic.help("Remove the type parameters from the `Protocol` base");
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                        prior_node.value.end(),
                        prior_node.end(),
                    ))));
                }
                continue;
            }
            // Note that unlike several of the other errors caught in this function,
            // this does not lead to the class creation failing at runtime,
            // but it is semantically invalid.
            Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(generic_context)) => {
                if let Some(type_params) = class_node.type_params.as_deref() {
                    let Some(builder) =
                        context.report_lint(&INVALID_GENERIC_CLASS, &class_node.bases()[i])
                    else {
                        continue;
                    };
                    let mut diagnostic = builder.into_diagnostic(
                        "Cannot both inherit from subscripted `Protocol` \
                            and use PEP 695 type variables",
                    );
                    if let ast::Expr::Subscript(node) = &class_node.bases()[i] {
                        let source = source_text(db, context.file());
                        let type_params_range = TextRange::new(
                            type_params.start().saturating_add(TextSize::new(1)),
                            type_params.end().saturating_sub(TextSize::new(1)),
                        );
                        if source[node.slice.range()] == source[type_params_range] {
                            diagnostic.help("Remove the type parameters from the `Protocol` base");
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(
                                TextRange::new(node.value.end(), node.end()),
                            )));
                        }
                    }
                } else if protocol_base_with_generic_context.is_none() {
                    protocol_base_with_generic_context = Some((i, generic_context));
                }
                continue;
            }
            Type::ClassLiteral(class) => ClassType::NonGeneric(*class),
            Type::GenericAlias(class) => ClassType::Generic(*class),
            _ => continue,
        };

        if let Some(disjoint_base) = base_class.nearest_disjoint_base(db) {
            disjoint_bases.insert(disjoint_base, i, base_class.class_literal(db));
        }

        if is_protocol {
            if !base_class.is_protocol(db)
                && !base_class.is_object(db)
                && let Some(builder) =
                    context.report_lint(&INVALID_PROTOCOL, &class_node.bases()[i])
            {
                builder.into_diagnostic(format_args!(
                    "Protocol class `{}` cannot inherit from non-protocol class `{}`",
                    class.name(db),
                    base_class.name(db),
                ));
            }
        } else if class_kind == Some(CodeGeneratorKind::TypedDict) {
            if !base_class.class_literal(db).is_typed_dict(db)
                && let Some(builder) =
                    context.report_lint(&INVALID_TYPED_DICT_HEADER, &class_node.bases()[i])
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "TypedDict class `{}` can only inherit from TypedDict classes",
                    class.name(db),
                ));
                diagnostic.set_primary_message(format_args!(
                    "`{}` is not a `TypedDict` class",
                    base_class.name(db)
                ));
                diagnostic.annotate(
                    Annotation::secondary(base_class.class_literal(db).header_span(db))
                        .message(format_args!("`{}` defined here", base_class.name(db))),
                );
            }
            if base_class.class_literal(db).is_typed_dict(db) {
                direct_typed_dict_bases.push(base_class);
            }
        }

        if base_class.is_final(db) {
            if let Some(builder) =
                context.report_lint(&SUBCLASS_OF_FINAL_CLASS, &class_node.bases()[i])
            {
                builder.into_diagnostic(format_args!(
                    "Class `{}` cannot inherit from final class `{}`",
                    class.name(db),
                    base_class.name(db),
                ));
            }
        }

        if let Some((base_class_literal, _)) = base_class.static_class_literal(db)
            && let (Some(base_is_frozen), Some(class_is_frozen)) = (
                base_class_literal.is_frozen_dataclass(db),
                class.is_frozen_dataclass(db),
            )
            && base_is_frozen != class_is_frozen
        {
            report_bad_frozen_dataclass_inheritance(
                context,
                class,
                class_node,
                base_class_literal,
                &class_node.bases()[i],
                base_is_frozen,
            );
        }
    }

    // Check for starred variable-length tuples that cannot be unpacked
    let class_definition = index.expect_single_definition(class_node);
    for base in class_node.bases() {
        if let ast::Expr::Starred(starred) = base
            && let starred_ty = definition_expression_type(db, class_definition, &starred.value)
            && let Some(tuple_spec) = starred_ty.tuple_instance_spec(db)
            && !matches!(tuple_spec.as_ref(), Tuple::Fixed(_))
        {
            report_unsupported_base(context, base, starred_ty, class);
        }
    }

    // Check that the class's MRO is resolvable
    let expanded_base_nodes = expanded_class_base_nodes(db, class_node, index);
    match class.try_mro(db, None) {
        Err(mro_error) => match mro_error.reason() {
            StaticMroErrorKind::DuplicateBases(duplicates) => {
                for duplicate in duplicates {
                    report_duplicate_bases(context, class, duplicate, &expanded_base_nodes);
                }
            }
            StaticMroErrorKind::InvalidBases(bases) => {
                for (index, base_ty) in bases {
                    let Some(base_node) = expanded_base_nodes.get(*index) else {
                        continue;
                    };
                    report_invalid_or_unsupported_base(context, base_node, *base_ty, class);
                }
            }
            StaticMroErrorKind::UnresolvableMro {
                bases_list,
                generic_index,
            } => {
                if let Some(builder) =
                    context.report_lint(&INCONSISTENT_MRO, class.header_range(db))
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot create a consistent method resolution order (MRO) \
                                    for class `{}` with bases list `[{}]`",
                        class.name(db),
                        bases_list.iter().map(|base| base.display(db)).join(", ")
                    ));
                    let can_rewrite_bases = bases_list.len() == class_node.bases().len()
                        && !class_node.bases().iter().any(ast::Expr::is_starred_expr);
                    if can_rewrite_bases
                        && let Some(index) = *generic_index
                        && let [first_base, .., last_base] = class_node.bases()
                    {
                        let source = source_text(db, context.file());
                        let generic_base = &source[class_node.bases()[index].range()];
                        diagnostic.help(format_args!(
                            "Move `{generic_base}` to the end of the bases list"
                        ));
                        let reordered_bases = class_node
                            .bases()
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != index)
                            .map(|(_, base)| &source[base.range()])
                            .chain(std::iter::once(generic_base))
                            .join(", ");
                        let fix = Fix::unsafe_edit(Edit::range_replacement(
                            reordered_bases,
                            TextRange::new(first_base.start(), last_base.end()),
                        ));
                        diagnostic.set_fix(fix);
                    }
                }
            }
            StaticMroErrorKind::Pep695ClassWithGenericInheritance => {
                if let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class_node) {
                    builder.into_diagnostic(
                        "Cannot both inherit from `typing.Generic` \
                            and use PEP 695 type variables",
                    );
                }
            }
            StaticMroErrorKind::InheritanceCycle => {
                if let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, class_node) {
                    builder.into_diagnostic(format_args!(
                        "Cyclic definition of `{}` (class cannot inherit from itself)",
                        class.name(db)
                    ));
                }
            }
        },
        Ok(_) => {
            disjoint_bases.remove_redundant_entries(db);

            if disjoint_bases.len() > 1 {
                report_instance_layout_conflict(
                    context,
                    class.header_range(db),
                    Some(class_node.bases()),
                    &disjoint_bases,
                );
            }

            // Check for inconsistent specializations of the same generic
            // base class. This detects when different explicit bases
            // contribute conflicting specializations of a common generic
            // ancestor to the MRO. For example:
            //
            //   class Grandparent(Generic[T1, T2]): ...
            //   class Parent(Grandparent[T1, T2]): ...
            //   class BadChild(Parent[T1, T2], Grandparent[T2, T1]): ...  # Error
            let explicit_bases = class.explicit_bases(db);
            let can_annotate_bases = || {
                class_node.bases().len() == explicit_bases.len()
                    && !class_node.bases().iter().any(ast::Expr::is_starred_expr)
            };

            // Maps each generic ancestor's class literal to the first
            // specialization seen and the index of the explicit base it
            // came from.
            let mut ancestor_specs =
                FxHashMap::<StaticClassLiteral<'db>, (GenericAlias<'db>, usize)>::default();

            'outer: for (i, base) in explicit_bases.iter().enumerate() {
                let base_class = match base {
                    Type::GenericAlias(c) => ClassType::Generic(*c),
                    Type::ClassLiteral(c) if c.generic_context(db).is_none() => {
                        ClassType::NonGeneric(*c)
                    }
                    _ => continue,
                };

                for supercls in base_class.iter_mro(db) {
                    let ClassBase::Class(ClassType::Generic(supercls_alias)) = supercls else {
                        continue;
                    };
                    let origin = supercls_alias.origin(db);

                    if let Some(&(earlier_alias, earlier_idx)) = ancestor_specs.get(&origin) {
                        if earlier_idx != i
                            && earlier_alias
                                .specialization(db)
                                .types(db)
                                .iter()
                                .zip(supercls_alias.specialization(db).types(db))
                                .any(|(t1, t2)| !t1.is_dynamic() && !t2.is_dynamic() && t1 != t2)
                        {
                            let Some(builder) =
                                context.report_lint(&INVALID_GENERIC_CLASS, class.header_range(db))
                            else {
                                break 'outer;
                            };
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Inconsistent type arguments for `{}` among class bases",
                                origin.name(db)
                            ));

                            let later_is_direct = matches!(
                                base,
                                Type::GenericAlias(a)
                                    if a.origin(db) == origin
                            );

                            if can_annotate_bases() {
                                diagnostic.annotate(
                                    context.secondary(&class_node.bases()[earlier_idx]).message(
                                        format_args!(
                                            "Earlier class base inherits from `{}`",
                                            earlier_alias.display(db)
                                        ),
                                    ),
                                );
                                let later_annotation = context.secondary(&class_node.bases()[i]);
                                diagnostic.annotate(if later_is_direct {
                                    later_annotation.message(format_args!(
                                        "Later class base is `{}`",
                                        supercls_alias.display(db)
                                    ))
                                } else {
                                    later_annotation.message(format_args!(
                                        "Later class base inherits from `{}`",
                                        supercls_alias.display(db)
                                    ))
                                });
                            } else {
                                diagnostic.info(format_args!(
                                    "Earlier class base inherits from `{}`",
                                    earlier_alias.display(db)
                                ));
                                if later_is_direct {
                                    diagnostic.info(format_args!(
                                        "Later class base is `{}`",
                                        supercls_alias.display(db)
                                    ));
                                } else {
                                    diagnostic.info(format_args!(
                                        "Later class base inherits from `{}`",
                                        supercls_alias.display(db)
                                    ));
                                }
                            }
                            diagnostic.set_concise_message(format_args!(
                                "Inconsistent type arguments: class cannot \
                                        inherit from both `{}` and `{}`",
                                supercls_alias.display(db),
                                earlier_alias.display(db)
                            ));
                            break 'outer;
                        }
                    } else if !supercls_alias
                        .specialization(db)
                        .types(db)
                        .iter()
                        .all(Type::is_dynamic)
                    {
                        ancestor_specs.insert(origin, (supercls_alias, i));
                    }
                }
            }
        }
    }

    // Check that `@total_ordering` has a valid ordering method in the MRO
    if class.total_ordering(db) && !class.has_ordering_method_in_mro(db, None) {
        // Find the `@total_ordering` decorator to report the diagnostic at its location
        if let Some(decorator) = class_node.decorator_list.iter().find(|decorator| {
            file_expression_type(&decorator.expression)
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::TotalOrdering))
        }) {
            report_invalid_total_ordering(context, ClassLiteral::Static(class), decorator);
        }
    }

    // Check that the class's metaclass can be determined without error.
    if let Err(metaclass_error) = class.try_metaclass(db) {
        let invalid_metaclass_range = class_node
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.find_keyword("metaclass"))
            .map(Ranged::range)
            .unwrap_or_else(|| class.header_range(db));
        match metaclass_error.reason() {
            MetaclassErrorKind::Cycle => {
                if let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, class_node) {
                    builder
                        .into_diagnostic(format_args!("Cyclic definition of `{}`", class.name(db)));
                }
            }
            MetaclassErrorKind::GenericMetaclass => {
                if let Some(builder) =
                    context.report_lint(&INVALID_METACLASS, invalid_metaclass_range)
                {
                    builder.into_diagnostic("Generic metaclasses are not supported");
                }
            }
            MetaclassErrorKind::NotCallable(ty) => {
                if let Some(builder) =
                    context.report_lint(&INVALID_METACLASS, invalid_metaclass_range)
                {
                    builder.into_diagnostic(format_args!(
                        "Metaclass type `{}` is not callable",
                        ty.display(db)
                    ));
                }
            }
            MetaclassErrorKind::PartlyNotCallable(ty) => {
                if let Some(builder) =
                    context.report_lint(&INVALID_METACLASS, invalid_metaclass_range)
                {
                    builder.into_diagnostic(format_args!(
                        "Metaclass type `{}` is partly not callable",
                        ty.display(db)
                    ));
                }
            }
            MetaclassErrorKind::Conflict {
                candidate1:
                    MetaclassCandidate {
                        metaclass: metaclass1,
                        explicit_metaclass_of: class1,
                    },
                candidate2:
                    MetaclassCandidate {
                        metaclass: metaclass2,
                        explicit_metaclass_of: class2,
                    },
                candidate1_is_base_class,
            } => {
                if *candidate1_is_base_class {
                    report_conflicting_metaclass_from_bases(
                        context,
                        class_node.into(),
                        class.name(db),
                        *metaclass1,
                        class1.name(db),
                        *metaclass2,
                        class2.name(db),
                    );
                } else if let Some(builder) =
                    context.report_lint(&CONFLICTING_METACLASS, class_node)
                {
                    builder.into_diagnostic(format_args!(
                        "The metaclass of a derived class (`{class}`) \
                            must be a subclass of the metaclasses of all its bases, \
                            but `{metaclass_of_class}` (metaclass of `{class}`) \
                            and `{metaclass_of_base}` (metaclass of base class `{base}`) \
                            have no subclass relationship",
                        class = class.name(db),
                        metaclass_of_class = metaclass1.name(db),
                        metaclass_of_base = metaclass2.name(db),
                        base = class2.name(db),
                    ));
                }
            }
        }
    }

    // Check that the class arguments matches the arguments of the
    // base class `__init_subclass__` method.
    if let Some(args) = class_node.arguments.as_deref() {
        if class_kind == Some(CodeGeneratorKind::TypedDict) {
            for keyword in &args.keywords {
                match keyword.arg.as_deref() {
                    Some(arg_name @ ("total" | "closed")) => {
                        let passed_type = file_expression_type(&keyword.value);
                        if passed_type
                            .as_literal_value()
                            .is_none_or(|literal| !literal.is_bool())
                            && let Some(builder) =
                                context.report_lint(&INVALID_ARGUMENT_TYPE, keyword)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Invalid argument to parameter `{arg_name}` \
                                    in `TypedDict` definition",
                            ));
                            diagnostic.set_primary_message(format_args!(
                                "Expected either `True` or `False`, got object of type `{}`",
                                passed_type.display(db)
                            ));
                        }
                    }
                    Some("extra_items") => {
                        // TODO: validate that passed arguments here are annotation expressions
                    }
                    Some("metaclass") => {
                        if let Some(builder) =
                            context.report_lint(&INVALID_TYPED_DICT_HEADER, keyword)
                        {
                            builder.into_diagnostic(format_args!(
                                "Custom metaclasses are not supported in `TypedDict` definitions",
                            ));
                        }
                    }
                    Some(other) => {
                        if let Some(builder) = context.report_lint(&UNKNOWN_ARGUMENT, keyword) {
                            builder.into_diagnostic(format_args!(
                                "Unknown keyword argument `{other}` \
                                        in `TypedDict` definition",
                            ));
                        }
                    }
                    None => {
                        if let Some(builder) =
                            context.report_lint(&INVALID_TYPED_DICT_HEADER, keyword)
                        {
                            builder.into_diagnostic(format_args!(
                                "Keyword-variadic arguments are not supported \
                                in `TypedDict` definitions",
                            ));
                        }
                    }
                }
            }
        } else {
            let call_args: CallArguments = args
                .keywords
                .iter()
                .filter_map(|keyword| match keyword.arg.as_ref() {
                    // We mimic the runtime behaviour and discard the metaclass argument
                    Some(name) if name.id.as_str() == "metaclass" => None,
                    Some(name) => {
                        let ty = file_expression_type(&keyword.value);
                        Some((Argument::Keyword(name.id.as_str()), Some(ty)))
                    }
                    None => {
                        let ty = file_expression_type(&keyword.value);
                        Some((Argument::Keywords, Some(ty)))
                    }
                })
                .collect();

            let init_subclass_type = class
                .class_member_from_mro(
                    db,
                    "__init_subclass__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    // skip(1) to skip the current class and only consider base classes.
                    class.iter_mro(db, None).skip(1),
                )
                .ignore_possibly_undefined();

            if let Some(init_subclass) = init_subclass_type {
                let call_args = call_args.with_self(Some(Type::from(class)));
                if let Err(call_error) = init_subclass.try_call(db, &call_args) {
                    report_subclass_of_class_with_non_callable_init_subclass(
                        context, call_error, class, class_node,
                    );
                }
            }
        }
    }

    // If the class is generic, verify that its generic context does not violate any of
    // the typevar scoping rules.
    if let (Some(legacy), Some(inherited)) = (
        class.legacy_generic_context(db),
        class.inherited_legacy_generic_context(db),
    ) {
        if !inherited.is_subset_of(db, legacy)
            && let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class_node)
        {
            builder.into_diagnostic(
                "`Generic` base class must include all type \
                    variables used in other base classes",
            );
        }
    }

    // Check that no type parameter with a default follows a TypeVarTuple.
    // This is prohibited by the typing spec because a TypeVarTuple consumes
    // all remaining positional type arguments.
    if let Some(type_params) = class_node.type_params.as_deref() {
        super::type_param_validation::check_no_default_after_typevar_tuple_pep695(
            context,
            type_params,
        );
    }

    if context.is_lint_enabled(&INVALID_GENERIC_CLASS) {
        if !class.has_pep_695_type_params(db)
            && let Some(generic_context) = class.legacy_generic_context(db)
        {
            struct State<'db> {
                typevar_with_default: TypeVarInstance<'db>,
                invalid_later_tvars: Vec<TypeVarInstance<'db>>,
            }

            let mut state: Option<State<'db>> = None;

            for bound_typevar in generic_context.variables(db) {
                let typevar = bound_typevar.typevar(db);
                let has_default = typevar.default_type(db).is_some();

                if let Some(state) = state.as_mut() {
                    if !has_default {
                        state.invalid_later_tvars.push(typevar);
                    }
                } else if has_default {
                    state = Some(State {
                        typevar_with_default: typevar,
                        invalid_later_tvars: vec![],
                    });
                }
            }

            if let Some(state) = state
                && !state.invalid_later_tvars.is_empty()
            {
                report_invalid_type_param_order(
                    context,
                    class,
                    class_node,
                    state.typevar_with_default,
                    &state.invalid_later_tvars,
                );
            }
        }

        // Check that type variable defaults only reference type variables
        // that precede them in the type parameter list.
        if let Some(generic_context) = class
            .pep695_generic_context(db)
            .or(class.legacy_generic_context(db))
        {
            let typevars = generic_context.variables(db).map(|btv| btv.typevar(db));

            // `variables` should be fairly cheap to clone; it's just several cheap wrappers around
            // a `std::slice::Iter` under the hood.
            for (i, typevar) in typevars.clone().enumerate() {
                let Some(default_ty) = typevar.default_type(db) else {
                    continue;
                };

                let first_bad_tvar = find_over_type(db, default_ty, false, |t| {
                    let tvar = match t {
                        Type::TypeVar(tvar) => tvar.typevar(db),
                        Type::KnownInstance(KnownInstanceType::TypeVar(tvar)) => tvar,
                        _ => return None,
                    };
                    if !typevars.clone().take(i).contains(&tvar) {
                        Some(tvar)
                    } else {
                        None
                    }
                });
                if let Some(bad_typevar) = first_bad_tvar {
                    let is_later_in_list = typevars.clone().skip(i).contains(&bad_typevar);
                    report_invalid_typevar_default_reference(
                        context,
                        class,
                        typevar,
                        bad_typevar,
                        is_later_in_list,
                    );
                }
            }
        }

        let scope = class.body_scope(db).scope(db);
        if let Some(parent) = scope.parent() {
            // Check that the class's own type parameters don't shadow
            // type variables from enclosing scopes (by name).
            if let Some(generic_context) = class.generic_context(db) {
                for self_typevar in generic_context.variables(db) {
                    let name = self_typevar.typevar(db).name(db);
                    for enclosing in enclosing_generic_contexts(db, index, parent) {
                        if let Some(other_typevar) = enclosing.binds_named_typevar(db, name) {
                            report_shadowed_type_variable(
                                context,
                                name,
                                "class",
                                &class_node.name.id,
                                class.header_range(db),
                                other_typevar,
                            );
                        }
                    }
                }
            }

            // Check that the class's base classes don't reference type
            // variables from enclosing scopes (by identity).
            for base_typevar in class.typevars_referenced_in_bases(db) {
                let typevar = base_typevar.typevar(db);
                for enclosing in enclosing_generic_contexts(db, index, parent) {
                    if let Some(other_typevar) = enclosing.binds_typevar(db, typevar) {
                        report_shadowed_type_variable(
                            context,
                            typevar.name(db),
                            "class",
                            &class_node.name.id,
                            class.header_range(db),
                            other_typevar,
                        );
                    }
                }
            }
        }
    }

    // Check that a dataclass does not have more than one `KW_ONLY`
    // and that required fields are defined before default fields.
    if let Some(field_policy @ CodeGeneratorKind::DataclassLike(_)) =
        CodeGeneratorKind::from_class(db, class.into(), None)
    {
        let specialization = None;

        let mut kw_only_sentinel_fields = vec![];
        let mut required_after_default_field_names = vec![];
        let mut has_seen_default_field = false;

        for (name, field) in class.own_fields(db, specialization, field_policy) {
            if field.is_kw_only_sentinel(db) {
                kw_only_sentinel_fields.push(name);
                continue;
            }

            // Extract dataclass field properties
            let FieldKind::Dataclass {
                default_ty,
                init,
                kw_only,
                ..
            } = &field.kind
            else {
                continue;
            };

            // Fields with init=False or kw_only=true don't participate in ordering check
            if !init || *kw_only == Some(true) {
                continue;
            }

            if default_ty.is_some() {
                has_seen_default_field = true;
            } else if has_seen_default_field {
                required_after_default_field_names.push(name);
            }
        }

        if kw_only_sentinel_fields.len() > 1 {
            // TODO: The fields should be displayed in a subdiagnostic.
            if let Some(builder) = context.report_lint(&DUPLICATE_KW_ONLY, &class_node.name) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Dataclass has more than one field annotated with `KW_ONLY`"
                ));

                diagnostic.info(format_args!(
                    "`KW_ONLY` fields: {}",
                    kw_only_sentinel_fields
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .join(", ")
                ));
            }
        }

        if !required_after_default_field_names.is_empty() {
            // Report field ordering violations
            let body_scope = class.body_scope(db).file_scope_id(db);
            let use_def_map = index.use_def_map(body_scope);
            let place_table = index.place_table(body_scope);

            for name in required_after_default_field_names {
                let Some(symbol_id) = place_table.symbol_id(name.as_str()) else {
                    continue;
                };
                for decl_with_constraints in use_def_map.end_of_scope_symbol_declarations(symbol_id)
                {
                    let Some(definition) = decl_with_constraints.declaration.definition() else {
                        continue;
                    };
                    let DefinitionKind::AnnotatedAssignment(ann_assign) = definition.kind(db)
                    else {
                        continue;
                    };
                    let Some(builder) = context
                        .report_lint(&DATACLASS_FIELD_ORDER, ann_assign.target(context.module()))
                    else {
                        continue;
                    };
                    builder.into_diagnostic(format_args!(
                        "Required field `{name}` cannot be defined \
                                after fields with default values",
                    ));

                    break;
                }
            }
        }
    }

    // (13) Check for violations of the Liskov Substitution Principle,
    // and for violations of other rules relating to invalid overrides of some sort.
    overrides::check_class(context, class);

    // (14) Check for unimplemented abstract methods on final classes.
    check_final_class_abstract_methods(context, class, class_node);

    // (15) Check for Final-qualified declarations without a value.
    check_class_final_without_value(context, class, index);

    if let Some(protocol) = class.into_protocol_class(db) {
        protocol.validate_members(context);
    }

    if class.is_typed_dict(db) {
        validate_typed_dict_class(context, class, class_node, &direct_typed_dict_bases);
    }

    class.validate_members(context);
}

/// Check that a `@final` class does not have unimplemented abstract methods.
///
/// A final class cannot be subclassed, so if it inherits abstract methods without
/// implementing them, those methods can never be implemented, making the class
/// effectively broken.
fn check_final_class_abstract_methods<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) {
    let db = context.db();

    // Only check if the class is final.
    if !class.is_final(db) {
        return;
    }

    // Exclude `Protocol` classes. It is possible to subtype a `Protocol` class
    // without subclassing it, so an `@final` `Protocol` class with unimplemented abstract
    // methods is not inherently broken in the same way as a non-`Protocol` final class
    // with unimplemented abstract methods.
    if class.is_protocol(db) {
        return;
    }

    let class_type = class.identity_specialization(db);
    let abstract_methods = class_type.abstract_methods(db);

    // If there are no abstract methods, we're done.
    let Some((first_method_name, abstract_method)) = abstract_methods.iter().next() else {
        return;
    };

    let Some(builder) = context.report_lint(&ABSTRACT_METHOD_IN_FINAL_CLASS, &class_node.name)
    else {
        return;
    };

    let class_name = class.name(db);

    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Final class `{class_name}` has unimplemented abstract methods",
    ));

    let num_abstract_methods = abstract_methods.len();

    if num_abstract_methods == 1 {
        diagnostic.set_concise_message(format_args!(
            "Final class `{class_name}` has unimplemented abstract method \
                `{first_method_name}`",
        ));
        diagnostic.set_primary_message(format_args!("`{first_method_name}` is unimplemented"));
    } else {
        let verbose = db.verbose();
        let max_abstract_methods_to_print = if verbose { num_abstract_methods } else { 3 };
        let formatted_methods =
            format_enumeration(abstract_methods.keys().take(max_abstract_methods_to_print));

        if num_abstract_methods > max_abstract_methods_to_print {
            diagnostic.set_primary_message(format_args!(
                "{num_abstract_methods} abstract methods are unimplemented, \
                        including {formatted_methods}",
            ));
            diagnostic.set_concise_message(format_args!(
                "Final class `{class_name}` has {num_abstract_methods} unimplemented \
                    abstract methods, including {formatted_methods}",
            ));
            diagnostic.info(format_args!(
                "Use `--verbose` to see all {num_abstract_methods} \
                    unimplemented abstract methods",
            ));
        } else {
            diagnostic.set_concise_message(format_args!(
                "Final class `{class_name}` has unimplemented \
                    abstract methods {formatted_methods}",
            ));
            diagnostic.set_primary_message(format_args!(
                "Abstract methods {formatted_methods} are unimplemented"
            ));
        }
    }

    let AbstractMethod {
        defining_class,
        definition,
        kind,
    } = abstract_method;

    let module = parsed_module(db, definition.file(db)).load(db);
    let span = Span::from(definition.focus_range(db, &module));
    let defining_class_name = defining_class.name(db);

    let mut secondary_annotation = Annotation::secondary(span);
    secondary_annotation = if defining_class.class_literal(db) == ClassLiteral::Static(class) {
        secondary_annotation.message(format_args!("`{first_method_name}` declared as abstract"))
    } else {
        secondary_annotation.message(format_args!(
            "`{first_method_name}` declared as abstract on superclass `{defining_class_name}`",
        ))
    };
    diagnostic.annotate(secondary_annotation);

    if !kind.is_explicit() {
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "`{defining_class_name}.{first_method_name}` is implicitly abstract \
                    because `{defining_class_name}` is a `Protocol` class \
                    and `{first_method_name}` lacks an implementation",
            ),
        );
        sub.annotate(
            Annotation::secondary(defining_class.definition_span(db))
                .message(format_args!("`{defining_class_name}` declared here")),
        );
        diagnostic.sub(sub);

        // If the implicitly abstract method is defined in first-party code
        // and the return type is assignable to `None`, they may not have intended
        // for it to be implicitly abstract; add a clarificatory note:
        if kind.is_implicit_due_to_stub_body() && db.should_check_file(definition.file(db)) {
            let function_type_as_callable = infer_definition_types(db, *definition)
                .binding_type(*definition)
                .try_upcast_to_callable(db);

            if let Some(callables) = function_type_as_callable
                && Type::function_like_callable(
                    db,
                    Signature::new(Parameters::gradual_form(), Type::none(db)),
                )
                .is_assignable_to(db, callables.into_type(db))
            {
                diagnostic.help(format_args!(
                    "Change the body of `{first_method_name}` to `return` \
                        or `return None` if it was not intended to be abstract"
                ));
            }
        }
    }
}

/// Returns the raw base-expression nodes expanded to match the indexing of
/// [`StaticClassLiteral::explicit_bases`].
///
/// For a starred base that unpacks a fixed-length tuple, this repeats the original starred
/// expression once per unpacked element so MRO diagnostics can map semantic base indices back to
/// source spans.
fn expanded_class_base_nodes<'a, 'db>(
    db: &'db dyn Db,
    class_node: &'a ast::StmtClassDef,
    index: &SemanticIndex<'db>,
) -> Vec<&'a ast::Expr> {
    let class_definition = index.expect_single_definition(class_node);
    let mut expanded_base_nodes = Vec::with_capacity(class_node.bases().len());

    for base_node in class_node.bases() {
        if let Some(tuple) =
            expanded_fixed_length_starred_class_base_tuple(db, class_definition, base_node)
        {
            for _ in 0..tuple.len() {
                expanded_base_nodes.push(base_node);
            }
            continue;
        }

        expanded_base_nodes.push(base_node);
    }

    expanded_base_nodes
}

/// Check for `Final`-qualified declarations in a class body scope that are never
/// assigned a value.
fn check_class_final_without_value<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    index: &SemanticIndex<'db>,
) {
    // In stub files, bare declarations without values are normal.
    if context.in_stub() {
        return;
    }

    let db = context.db();
    let body_scope = class.body_scope(db);
    let body_scope_id = body_scope.file_scope_id(db);
    let use_def = index.use_def_map(body_scope_id);
    let place_table = index.place_table(body_scope_id);

    // In dataclasses (and similar code-generated classes), Final fields without
    // defaults are initialized by the synthesized __init__, so they are valid.
    if CodeGeneratorKind::from_class(db, class.into(), None).is_some() {
        return;
    }

    for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
        let result = place_from_declarations(db, declarations);
        let first_declaration = result.first_declaration;
        let (place_and_quals, _) = result.into_place_and_conflicting_declarations();

        if !place_and_quals.qualifiers.contains(TypeQualifiers::FINAL) {
            continue;
        }

        // Check if the symbol has any bindings at class level.
        let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
        let binding_place = place_from_bindings(db, bindings);

        if !binding_place.place.is_undefined() {
            continue;
        }

        // Per the typing spec, a `Final` attribute declared in a class body without a
        // value must be initialized in `__init__`. Assignments in other methods don't count.
        let symbol = place_table.symbol(symbol_id);
        if has_binding_in_init(context, body_scope, index, symbol.name().as_str()) {
            continue;
        }

        let place = place_table.place(symbol_id);
        if let Some(first_decl) = first_declaration
            && let Some(builder) = context.report_lint(
                &FINAL_WITHOUT_VALUE,
                first_decl.full_range(db, context.module()),
            )
        {
            builder.into_diagnostic(format_args!(
                "`Final` symbol `{place}` is not assigned a value"
            ));
        }
    }
}

/// Returns `true` if `name` has any attribute assignment (`self.<name> = ...`) in an
/// `__init__` method of the class whose body scope is `class_body_scope`.
fn has_binding_in_init<'db>(
    context: &InferContext<'db, '_>,
    class_body_scope: ScopeId<'db>,
    index: &SemanticIndex<'db>,
    name: &str,
) -> bool {
    let db = context.db();
    attribute_assignments(db, class_body_scope, name).any(|(bindings, scope_id)| {
        let is_init = index
            .scope(scope_id)
            .node()
            .as_function()
            .is_some_and(|f| f.node(context.module()).name.id == "__init__");
        is_init
            && bindings
                .into_iter()
                .any(|b| b.binding.definition().is_some())
    })
}
