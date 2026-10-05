//! Argument classification and splat validation before parameter matching.

use std::convert::Infallible;

use super::{
    Argument, ArgumentsIter, BuilderStore, CallArguments, Preparation, PreparationStep, Splat,
    TypeInferenceBuilder, ast,
};

#[cfg(feature = "experimental-analysis")]
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
#[cfg(feature = "experimental-analysis")]
use salsa::execution_probe::{RunError, RunResult};

pub(super) trait PreparationEffects<'db, 'ast> {
    type Error;

    async fn next<'expr>(
        &self,
        cursor: &mut ArgumentsIter<'expr>,
    ) -> Result<Option<ast::ArgOrKeyword<'expr>>, Self::Error>;

    async fn append<'expr>(
        &self,
        arguments: &mut CallArguments<'expr, 'db>,
        argument: Argument<'expr>,
    ) -> Result<(), Self::Error>;

    async fn scan(&self, count: usize) -> Result<(), Self::Error>;

    async fn validate_positional(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> Result<(), Self::Error>;

    async fn validate_keywords(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> Result<(), Self::Error>;
}

pub(super) async fn advance<'db, 'ast, 'expr, E: PreparationEffects<'db, 'ast>>(
    mut preparation: Preparation<'db, 'expr>,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
    effects: &E,
) -> Result<PreparationStep<'db, 'expr>, E::Error> {
    let Some(source) = effects.next(&mut preparation.cursor).await? else {
        validate_splats(
            builders.get_mut(preparation.builder),
            preparation.source,
            effects,
        )
        .await?;
        return Ok(PreparationStep::Complete(preparation.arguments));
    };
    let (argument, splat) = CallArguments::classify_argument(&source);
    if let Some(expression) = splat {
        return Ok(PreparationStep::Infer(
            Splat {
                preparation,
                source,
                argument,
            },
            expression,
        ));
    }
    effects.append(&mut preparation.arguments, argument).await?;
    Ok(PreparationStep::Continue(preparation))
}

pub(super) async fn validate_splats<'db, 'ast, E: PreparationEffects<'db, 'ast>>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    arguments: &ast::Arguments,
    effects: &E,
) -> Result<(), E::Error> {
    effects.scan(arguments.len()).await?;
    for arg in &arguments.args {
        if let ast::Expr::Starred(ast::ExprStarred { value, .. }) = arg {
            effects.validate_positional(builder, value).await?;
        }
    }
    for keyword in &arguments.keywords {
        if keyword.arg.is_none() {
            effects.validate_keywords(builder, &keyword.value).await?;
        }
    }
    Ok(())
}

pub(super) struct OrdinaryPreparationEffects;

impl<'db, 'ast> PreparationEffects<'db, 'ast> for OrdinaryPreparationEffects {
    type Error = Infallible;

    async fn next<'expr>(
        &self,
        cursor: &mut ArgumentsIter<'expr>,
    ) -> Result<Option<ast::ArgOrKeyword<'expr>>, Infallible> {
        Ok(cursor.next())
    }

    async fn append<'expr>(
        &self,
        arguments: &mut CallArguments<'expr, 'db>,
        argument: Argument<'expr>,
    ) -> Result<(), Infallible> {
        arguments.push_argument(argument, None);
        Ok(())
    }

    async fn scan(&self, _count: usize) -> Result<(), Infallible> {
        Ok(())
    }

    async fn validate_positional(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.local_validate_positional_splat(value);
        Ok(())
    }

    async fn validate_keywords(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.local_validate_keyword_splat(value);
        Ok(())
    }
}

#[cfg(feature = "experimental-analysis")]
impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> PreparationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next<'expr>(
        &self,
        cursor: &mut ArgumentsIter<'expr>,
    ) -> RunResult<Option<ast::ArgOrKeyword<'expr>>> {
        self.local(4, 0, || cursor.next()).await
    }

    async fn append<'expr>(
        &self,
        arguments: &mut CallArguments<'expr, 'db>,
        argument: Argument<'expr>,
    ) -> RunResult<()> {
        // Preparation reserves for the complete AST argument list before creating this owner.
        // Each append adds an empty type map; argument inference admits its later mutations.
        self.local(3, 0, || arguments.push_preallocated_argument(argument))
            .await?
    }

    async fn scan(&self, count: usize) -> RunResult<()> {
        self.work(Self::checked(count.checked_add(1))?).await
    }

    async fn validate_positional(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _value: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::CallArguments).await
    }

    async fn validate_keywords(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _value: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::CallArguments).await
    }
}
