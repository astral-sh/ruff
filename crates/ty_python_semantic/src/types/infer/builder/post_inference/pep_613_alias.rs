use crate::types::infer::builder::ContextualExpressionMetadata;
use crate::types::infer::{InferenceFlags, TypeInferenceBuilder};
use ty_python_core::definition::{AnnotatedAssignmentDefinitionKind, Definition};

pub(crate) fn analyze_pep_613_alias<'db>(
    assignment: &AnnotatedAssignmentDefinitionKind,
    definition: Definition<'db>,
    builder: &mut TypeInferenceBuilder<'db, '_>,
    report_diagnostics: bool,
) {
    let context = &builder.context;

    let Some(value) = assignment.value(context.module()) else {
        return;
    };

    let annotation = assignment.annotation(context.module());
    if !builder
        .file_expression_type(annotation)
        .is_typealias_special_form()
    {
        return;
    }

    let mut speculative = builder.speculate();
    if !report_diagnostics {
        speculative.context.suppress_diagnostics();
    }

    speculative.typevar_binding_context = Some(definition);
    speculative.context.inference_flags |= InferenceFlags::IN_TYPE_ALIAS;
    speculative.infer_type_expression(value);

    ContextualExpressionMetadata::take_from_builder(&mut speculative).extend_into(builder);
    let diagnostics = speculative.context.finish();
    if report_diagnostics {
        builder.context.extend(&diagnostics);
    }
}
