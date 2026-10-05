//! Assignment constructors share classification while retaining precise source boundaries.

use ty_python_core::scope::ScopeKind;

use super::*;

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> assignment::AssignmentEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn named_tuple_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<NamedTupleKind>> {
        call::CallEffects::named_tuple_kind(self, builder, ty).await
    }

    async fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<TypingModule>> {
        call::CallEffects::typed_dict_module(self, builder, ty).await
    }

    async fn is_new_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        call::CallEffects::function_is_known(self, builder, ty, KnownFunction::NewClass).await
    }

    async fn enum_base(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        call::CallEffects::enum_base(self, builder, ty).await
    }

    async fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.known_call_class(builder.db(), ty).await
    }

    async fn special_call(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _call: &ast::ExprCall,
        _definition: Definition<'db>,
        kind: assignment::SpecialCall,
    ) -> RunResult<Type<'db>> {
        self.unavailable(match kind {
            assignment::SpecialCall::NamedTuple(_) => SourceOperation::AssignmentNamedTuple,
            assignment::SpecialCall::TypedDict(_) => SourceOperation::AssignmentTypedDict,
            assignment::SpecialCall::NewClass => SourceOperation::AssignmentNewClass,
            assignment::SpecialCall::ParamSpec(_) => SourceOperation::AssignmentParamSpec,
            assignment::SpecialCall::TypeVarTuple(_) => SourceOperation::AssignmentTypeVarTuple,
            assignment::SpecialCall::NewType => SourceOperation::AssignmentNewType,
            assignment::SpecialCall::BuiltinType => SourceOperation::AssignmentBuiltinType,
            assignment::SpecialCall::TypeAliasType(_) => SourceOperation::AssignmentTypeAliasType,
        })
        .await
    }

    async fn optional_special_call(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _call: &ast::ExprCall,
        _definition: Definition<'db>,
        kind: assignment::OptionalSpecialCall,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(match kind {
            assignment::OptionalSpecialCall::Enum(_) => SourceOperation::AssignmentEnum,
            assignment::OptionalSpecialCall::Sentinel => SourceOperation::AssignmentSentinel,
        })
        .await
    }

    async fn is_class_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        let scope = self
            .field(builder.scope().read_fields(builder.db()).file_scope_id())
            .await?;
        self.local(1, 0, || {
            builder.index.scope(scope).kind() == ScopeKind::Class
        })
        .await
    }

    async fn desugared_decorator(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _callable_type: Type<'db>,
        _call: &ast::ExprCall,
        _ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::AssignmentDesugaredDecorator)
            .await
    }
}
