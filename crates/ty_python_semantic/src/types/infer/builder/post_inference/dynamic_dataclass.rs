use ruff_python_ast as ast;

use crate::types::{
    ClassLiteral, ClassType, DataclassFlags, Type,
    class::{DynamicDataclassAnchor, DynamicDataclassLiteral, DynamicMetaclassConflict},
    class_base::ClassBase,
    context::InferContext,
    diagnostic::{
        INVALID_FROZEN_DATACLASS_SUBCLASS, IncompatibleBases,
        report_conflicting_metaclass_from_bases, report_instance_layout_conflict,
    },
    infer::builder::{
        make_dataclass_decorator_type_is_dataclass_like, report_dynamic_dataclass_mro_errors,
    },
    infer_definition_types,
};
use ty_python_core::definition::Definition;

/// Iterate over all dynamic dataclass definitions (created using `make_dataclass()` calls) to check
/// that the definition will not cause an exception to be raised at runtime. This needs to be done
/// after deferred inference completes, since bases may contain forward references.
pub(crate) fn check_dynamic_dataclass_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
) {
    let db = context.db();

    let Some(value) = definition.kind(db).value(context.module()) else {
        return;
    };

    let Some(call_expr) = value.as_call_expr() else {
        return;
    };

    let inference = infer_definition_types(db, definition);
    let dataclass =
        inference
            .functional_dataclass()
            .or_else(|| match inference.binding_type(definition) {
                Type::ClassLiteral(ClassLiteral::DynamicDataclass(dataclass)) => Some(dataclass),
                _ => None,
            });

    let Some(dataclass) = dataclass else {
        return;
    };

    // Only check dataclasses with Definition anchors (i.e., assigned `make_dataclass()` calls).
    // Dangling `make_dataclass()` calls are validated eagerly during inference.
    let DynamicDataclassAnchor::Definition(_) = dataclass.anchor(db) else {
        return;
    };

    // Find the `bases` keyword argument.
    let bases_kw = call_expr.arguments.find_keyword("bases");
    let bases_node = bases_kw.map(|kw| &kw.value);

    // Check for MRO errors.
    if report_dynamic_dataclass_mro_errors(context, dataclass, call_expr) {
        // MRO succeeded, check for instance-layout-conflict.
        let mut disjoint_bases = IncompatibleBases::default();
        let bases_tuple_elts =
            bases_node.and_then(|n| n.as_tuple_expr().map(|tuple| tuple.elts.as_slice()));

        for (idx, base) in dataclass.bases(db).iter().enumerate() {
            if let ClassBase::Class(class_type) = base {
                if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                    disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                }
            }
        }

        disjoint_bases.remove_redundant_entries(db);
        if disjoint_bases.len() > 1 {
            report_instance_layout_conflict(
                context,
                dataclass.header_range(db),
                bases_tuple_elts,
                &disjoint_bases,
            );
        }
    }

    // Check for metaclass conflicts.
    if let Err(DynamicMetaclassConflict {
        metaclass1,
        base1,
        metaclass2,
        base2,
    }) = dataclass.try_metaclass(db)
    {
        report_conflicting_metaclass_from_bases(
            context,
            call_expr.into(),
            dataclass.name(db),
            metaclass1,
            base1.display(db),
            metaclass2,
            base2.display(db),
        );
    }

    if make_dataclass_uses_dataclass_like_decorator(db, call_expr, inference) {
        let bases_tuple_elts =
            bases_node.and_then(|n| n.as_tuple_expr().map(|tuple| tuple.elts.as_slice()));
        check_dynamic_dataclass_frozen_base_inheritance(context, dataclass, bases_tuple_elts);
    }
}

/// Return whether the `make_dataclass` call should be validated with dataclass runtime rules.
///
/// On Python 3.14, a plain custom decorator receives the class before dataclass processing and may
/// return anything, so frozen-inheritance validation only applies when the decorator is omitted,
/// is `dataclass`, or is marked with `dataclass_transform`.
fn make_dataclass_uses_dataclass_like_decorator<'db>(
    db: &'db dyn crate::Db,
    call_expr: &ast::ExprCall,
    inference: &crate::types::infer::DefinitionInference<'db>,
) -> bool {
    call_expr
        .arguments
        .find_keyword("decorator")
        .is_none_or(|keyword| {
            make_dataclass_decorator_type_is_dataclass_like(
                db,
                inference.expression_type(&keyword.value),
            )
        })
}

/// Return the frozen status for static and dynamic dataclass classes.
///
/// Non-dataclass bases return `None` because dataclasses only enforce frozen inheritance
/// compatibility across dataclass bases.
fn class_frozen_dataclass_status<'db>(
    db: &'db dyn crate::Db,
    class: ClassType<'db>,
) -> Option<bool> {
    match class.class_literal(db) {
        ClassLiteral::Static(static_class) => static_class.is_frozen_dataclass(db),
        ClassLiteral::DynamicDataclass(dataclass) => Some(
            dataclass
                .dataclass_params(db)
                .flags(db)
                .contains(DataclassFlags::FROZEN),
        ),
        ClassLiteral::Dynamic(_)
        | ClassLiteral::DynamicNamedTuple(_)
        | ClassLiteral::DynamicTypedDict(_)
        | ClassLiteral::DynamicEnum(_) => None,
    }
}

/// Report a frozen/non-frozen inheritance mismatch for a dynamic dataclass.
///
/// CPython rejects both directions:
///
/// ```py
/// @dataclass(frozen=True)
/// class Frozen: ...
///
/// make_dataclass("C", [], bases=(Frozen,))
/// make_dataclass("D", [], bases=(NonFrozen,), frozen=True)
/// ```
fn report_bad_dynamic_dataclass_frozen_inheritance<'db>(
    context: &InferContext<'db, '_>,
    dataclass: DynamicDataclassLiteral<'db>,
    base_class: ClassType<'db>,
    base_node: Option<&ast::Expr>,
    base_is_frozen: bool,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(
        &INVALID_FROZEN_DATACLASS_SUBCLASS,
        dataclass.header_range(db),
    ) else {
        return;
    };

    let mut diagnostic = if base_is_frozen {
        let mut diagnostic =
            builder.into_diagnostic("Non-frozen dataclass cannot inherit from frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Non-frozen dataclass `{}` cannot inherit from frozen dataclass `{}`",
            dataclass.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is not frozen but base class `{}` is",
            dataclass.name(db),
            base_class.name(db)
        ));
        diagnostic
    } else {
        let mut diagnostic =
            builder.into_diagnostic("Frozen dataclass cannot inherit from non-frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Frozen dataclass `{}` cannot inherit from non-frozen dataclass `{}`",
            dataclass.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is frozen but base class `{}` is not",
            dataclass.name(db),
            base_class.name(db)
        ));
        diagnostic
    };

    if let Some(base_node) = base_node {
        diagnostic.annotate(context.secondary(base_node));
    }
    diagnostic.info("This causes the class creation to fail");
}

/// Validate frozen inheritance constraints for a functional dataclass and its explicit bases.
fn check_dynamic_dataclass_frozen_base_inheritance<'db>(
    context: &InferContext<'db, '_>,
    dataclass: DynamicDataclassLiteral<'db>,
    bases_tuple_elts: Option<&[ast::Expr]>,
) {
    let db = context.db();
    let class_is_frozen = dataclass
        .dataclass_params(db)
        .flags(db)
        .contains(DataclassFlags::FROZEN);

    for (idx, base) in dataclass.bases(db).iter().enumerate() {
        let ClassBase::Class(base_class) = base else {
            continue;
        };
        let Some(base_is_frozen) = class_frozen_dataclass_status(db, *base_class) else {
            continue;
        };
        if base_is_frozen == class_is_frozen {
            continue;
        }

        report_bad_dynamic_dataclass_frozen_inheritance(
            context,
            dataclass,
            *base_class,
            bases_tuple_elts.and_then(|elts| elts.get(idx)),
            base_is_frozen,
        );
    }
}
