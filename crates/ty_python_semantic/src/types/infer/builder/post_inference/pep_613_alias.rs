use crate::{
    semantic_index::definition::{AnnotatedAssignmentDefinitionKind, Definition},
    types::{
        TypeCheckDiagnostics,
        infer::{InferenceFlags, TypeInferenceBuilder},
    },
};

pub(crate) fn check_pep_613_alias<'db>(
    assignment: &AnnotatedAssignmentDefinitionKind,
    definition: Definition<'db>,
    builder: &TypeInferenceBuilder<'db, '_>,
) -> Option<TypeCheckDiagnostics> {
    let context = &builder.context;

    let value = assignment.value(context.module())?;

    let annotation = assignment.annotation(context.module());
    if !builder
        .file_expression_type(annotation)
        .is_typealias_special_form()
    {
        return None;
    }

    let mut speculative = builder.speculate();

    speculative.typevar_binding_context = Some(definition);
    speculative.inference_flags |= InferenceFlags::IN_TYPE_ALIAS;
    speculative.infer_type_expression(value);
    Some(speculative.context.finish())
}
