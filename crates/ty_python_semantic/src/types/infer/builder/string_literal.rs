//! String literal selection retains expected types and the type-alias interpretation.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use super::TypeInferenceBuilder;
use crate::types::{InternedType, KnownInstanceType, Type, TypeContext};

pub(in crate::types::infer) struct StringLiteralFacts;
pub(super) struct OrdinaryStringLiteralEffects;

shared_semantic_family! {
    #[synchronous(SynchronousStringLiteralEffects)]
    pub(in crate::types::infer) trait StringLiteralEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        async fn expected(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, literal: &ast::ExprStringLiteral, expected: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, literal: &ast::ExprStringLiteral) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn length(&self, literal: &ast::ExprStringLiteral) -> Result<usize, Self::Error>;
        #[operation(child)]
        async fn intern(&self, builder: &TypeInferenceBuilder<'db, 'ast>, literal: &ast::ExprStringLiteral, length: usize) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl StringLiteralFacts {
        fn is_alias<'db>(&self, context: TypeContext<'db>) -> bool { context.is_typealias() }
        fn within_limit(&self, length: usize) -> bool { length <= TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE }
        fn general<'db>(&self) -> Type<'db> { Type::literal_string() }
    }

    #[synchronous(infer_string_literal_sync)]
    #[capabilities(effects = StringLiteralEffects, facts = StringLiteralFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn infer_string_literal_with<'db, 'ast, E: StringLiteralEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        literal: &ast::ExprStringLiteral,
        context: TypeContext<'db>,
        facts: StringLiteralFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if let Some(expected) = context.annotation {
            effects.expected(builder, literal, expected).await?;
        }
        if facts.is_alias(context) {
            return effects.alias(builder, literal).await;
        }
        let length = effects.length(literal).await?;
        if facts.within_limit(length) {
            effects.intern(builder, literal, length).await
        } else {
            Ok(facts.general())
        }
    }
}

impl<'db, 'ast> SynchronousStringLiteralEffects<'db, 'ast> for OrdinaryStringLiteralEffects {
    type Error = Infallible;

    fn expected(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        literal: &ast::ExprStringLiteral,
        expected: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.store_maybe_expected_type(ast::ExprRef::from(literal), expected);
        Ok(())
    }

    fn alias(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        literal: &ast::ExprStringLiteral,
    ) -> Result<Type<'db>, Infallible> {
        let aliased_type = builder.infer_string_type_expression(literal);
        Ok(Type::KnownInstance(KnownInstanceType::LiteralStringAlias(
            InternedType::new(builder.db(), aliased_type),
        )))
    }

    fn length(&self, literal: &ast::ExprStringLiteral) -> Result<usize, Infallible> {
        Ok(literal.value.len())
    }

    fn intern(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        literal: &ast::ExprStringLiteral,
        _length: usize,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::string_literal(builder.db(), literal.value.to_str()))
    }
}

#[cfg(feature = "experimental-analysis")]
mod source {
    use salsa::execution_probe::{RunError, RunResult};

    use super::*;
    use crate::types::infer::builder::source_definition::controlled::{
        SourceAccess, SourceEffects, SourceOperation,
    };
    use crate::types::infer::builder::source_expression::SourceExpressionOperation;

    impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
        pub(in crate::types::infer) async fn string_literal_text<'expr>(
            &self,
            literal: &'expr ast::ExprStringLiteral,
            length: usize,
        ) -> RunResult<&'expr str> {
            let parts = self.local(1, 0, || literal.value.as_slice().len()).await?;
            // Adjacent literals are first collected into a growable String, then shrunk to
            // Box<str>. Geometric growth requests less than four times the final length,
            // plus the minimum eight-byte buffer; shrinking can request the final length again.
            let bytes = if parts > 1 {
                Self::checked(length.checked_mul(5).and_then(|n| n.checked_add(8)))?
            } else {
                0
            };
            let work = Self::checked(
                length
                    .checked_add(bytes)
                    .and_then(|n| n.checked_add(parts))
                    .and_then(|n| n.checked_add(3)),
            )?;
            self.local(work, bytes, || literal.value.to_str()).await
        }
    }

    impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> StringLiteralEffects<'db, 'ast>
        for SourceEffects<'_, 'run, 'db, A>
    {
        type Error = RunError;

        async fn expected(
            &self,
            _builder: &mut TypeInferenceBuilder<'db, 'ast>,
            _literal: &ast::ExprStringLiteral,
            _expected: Type<'db>,
        ) -> RunResult<()> {
            self.unavailable(SourceOperation::Expression(
                SourceExpressionOperation::StringLiteralExpectedType,
            ))
            .await
        }

        async fn alias(
            &self,
            _builder: &mut TypeInferenceBuilder<'db, 'ast>,
            _literal: &ast::ExprStringLiteral,
        ) -> RunResult<Type<'db>> {
            self.unavailable(SourceOperation::Expression(
                SourceExpressionOperation::StringTypeAlias,
            ))
            .await
        }

        async fn length(&self, literal: &ast::ExprStringLiteral) -> RunResult<usize> {
            let parts = self.local(1, 0, || literal.value.as_slice()).await?;
            let mut next = 0;
            let mut length = 0usize;
            loop {
                let part = self
                    .local(3, 0, || {
                        let part = parts.get(next)?;
                        next += 1;
                        Some(part.value.len())
                    })
                    .await?;
                let Some(part) = part else {
                    return Ok(length);
                };
                length = Self::checked(length.checked_add(part))?;
            }
        }

        async fn intern(
            &self,
            _builder: &TypeInferenceBuilder<'db, 'ast>,
            literal: &ast::ExprStringLiteral,
            length: usize,
        ) -> RunResult<Type<'db>> {
            let value = self.string_literal_text(literal, length).await?;
            self.string_literal_value(value).await
        }
    }
}
