use crate::types::{
    ClassLiteral, Type, binding_type,
    class::{DynamicDataclassAnchor, DynamicMetaclassConflict},
    class_base::ClassBase,
    context::InferContext,
    diagnostic::{
        IncompatibleBases, report_conflicting_metaclass_from_bases, report_instance_layout_conflict,
    },
    infer::builder::report_dynamic_dataclass_mro_errors,
};
use ty_python_core::definition::{Definition, DefinitionKind};

/// Iterate over all dynamic dataclass definitions (created using `make_dataclass()` calls) to check
/// that the definition will not cause an exception to be raised at runtime. This needs to be done
/// after deferred inference completes, since bases may contain forward references.
pub(crate) fn check_dynamic_dataclass_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
) {
    let db = context.db();

    let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
        return;
    };

    let ty = binding_type(db, definition);

    let Type::ClassLiteral(ClassLiteral::DynamicDataclass(dataclass)) = ty else {
        return;
    };

    // Only check dataclasses with Definition anchors (i.e., assigned `make_dataclass()` calls).
    // Dangling `make_dataclass()` calls are validated eagerly during inference.
    let DynamicDataclassAnchor::Definition(_) = dataclass.anchor(db) else {
        return;
    };

    let value = assignment.value(context.module());
    let Some(call_expr) = value.as_call_expr() else {
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
}
