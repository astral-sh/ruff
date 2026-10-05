use super::*;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn local_prepare_annotation_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        state: DeferredExpressionState,
    ) -> RunResult<AnnotationScope> {
        let in_stub = if state.in_string_annotation() {
            false
        } else {
            self.file_is_stub(builder.file()).await?
        };
        self.local(3, size_of::<AnnotationScope>(), || AnnotationScope::prepare(builder, state, in_stub))
            .await
    }

    pub(super) async fn local_store_type_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.store_expression(builder, expression, ty).await
    }

    pub(super) async fn local_prepare_type_expression_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        mode: TypeExpressionMode,
    ) -> RunResult<Option<TypeExpressionScope>> {
        if matches!(mode, TypeExpressionMode::NoStore) {
            return self.initialize_value(|| None).await;
        }
        let in_stub = self.file_is_stub(builder.file()).await?;
        self.local(5, size_of::<Option<TypeExpressionScope>>(), || {
            TypeExpressionScope::prepare(builder, mode, in_stub)
        })
        .await
    }
}
