use crate::{
    semantic_index::definition::{Definition, DefinitionKind},
    types::{
        ClassLiteral, Type, binding_type,
        class::{DynamicClassAnchor, DynamicMetaclassConflict, dynamic_class_bases_argument},
        context::InferContext,
        diagnostic::{
            IncompatibleBases, report_conflicting_metaclass_from_bases,
            report_instance_layout_conflict,
        },
        infer::builder::report_dynamic_mro_errors,
    },
};

/// Iterate over all dynamic class definitions (created using `type()` calls) to check that
/// the definition will not cause an exception to be raised at runtime. This needs to be done
/// after deferred inference completes, since bases may contain forward references.
pub(crate) fn check_dynamic_class_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
) {
    let db = context.db();

    let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
        return;
    };

    let ty = binding_type(db, definition);

    // Check if it's a dynamic class with a Definition anchor.
    let Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class)) = ty else {
        return;
    };

    // Only check classes with Definition anchors (i.e., assigned `type()` calls).
    // Dangling `type()` calls are validated eagerly during inference.
    let DynamicClassAnchor::Definition(_) = dynamic_class.anchor(db) else {
        return;
    };

    let value = assignment.value(context.module());
    let Some(call_expr) = value.as_call_expr() else {
        return;
    };

    let Some(bases) = dynamic_class_bases_argument(&call_expr.arguments) else {
        return;
    };

    // Check for MRO errors.
    if report_dynamic_mro_errors(context, dynamic_class, call_expr, bases) {
        // MRO succeeded, check for instance-layout-conflict.
        let mut disjoint_bases = IncompatibleBases::default();
        let bases_tuple_elts = bases.as_tuple_expr().map(|tuple| tuple.elts.as_slice());

        for (idx, base_type) in dynamic_class.explicit_bases(db).iter().enumerate() {
            // Convert to ClassType to access nearest_disjoint_base.
            if let Some(class_type) = base_type.to_class_type(db) {
                if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                    disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                }
            }
        }

        disjoint_bases.remove_redundant_entries(db);
        if disjoint_bases.len() > 1 {
            report_instance_layout_conflict(
                context,
                dynamic_class.header_range(db),
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
    }) = dynamic_class.try_metaclass(db)
    {
        report_conflicting_metaclass_from_bases(
            context,
            call_expr.into(),
            dynamic_class.name(db),
            metaclass1,
            base1.display(db),
            metaclass2,
            base2.display(db),
        );
    }
}
