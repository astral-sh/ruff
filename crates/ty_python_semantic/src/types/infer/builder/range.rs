use std::convert::Infallible;

use ruff_python_ast as ast;

use super::TypeInferenceBuilder;
use crate::types::call::CallArguments;
use crate::types::class::ClassLiteral;
use crate::types::{KnownClass, Type};

pub(super) trait RangeInferenceEffects<'db, 'ast> {
    type Error;

    async fn class_is_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn infer_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        arguments: &ast::Arguments,
        call_arguments: &CallArguments<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
}

pub(super) async fn infer_builtin_range_instance_type_with<
    'db,
    'ast,
    E: RangeInferenceEffects<'db, 'ast>,
>(
    builder: &TypeInferenceBuilder<'db, 'ast>,
    callable_type: Type<'db>,
    arguments: &ast::Arguments,
    call_arguments: &CallArguments<'_, 'db>,
    effects: &E,
) -> Result<Option<Type<'db>>, E::Error> {
    let Type::ClassLiteral(class) = callable_type else {
        return Ok(None);
    };
    if !effects.class_is_range(builder, class).await? {
        return Ok(None);
    }
    effects
        .infer_range(builder, arguments, call_arguments)
        .await
}

pub(super) struct OrdinaryRangeInferenceEffects;

impl<'db, 'ast> RangeInferenceEffects<'db, 'ast> for OrdinaryRangeInferenceEffects {
    type Error = Infallible;

    async fn class_is_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.is_known(builder.db(), KnownClass::Range))
    }

    async fn infer_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        arguments: &ast::Arguments,
        call_arguments: &CallArguments<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(builder.infer_builtin_range_instance_type_positive(arguments, call_arguments))
    }
}
