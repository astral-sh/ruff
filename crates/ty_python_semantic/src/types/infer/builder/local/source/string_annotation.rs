//! Retain parsed quoted syntax while its admitted semantic continuation runs.

use std::future::Future;
use std::pin::Pin;

use ruff_db::source::SourceText;
use ruff_python_ast::{self as ast, StringFlags};
use ruff_python_parser::ParseError;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ExpressionNodeKey;

use super::super::string_annotation::{ParsedAnnotation, Scope};
use super::*;
use crate::analysis::QuotedAnnotationOperation;
use crate::types::context::InferContext;
use crate::types::infer::TypeExpressionFlags;
use crate::types::infer::builder::source_definition::controlled::storage::table_merge;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::quoted_annotations as observations;
use crate::types::relation::stable_storage::StableStorage;
use crate::types::string_annotation::{StringAnnotationEffects, parse_string_annotation_with};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits a quoted-annotation callback and its result alongside the operation's own storage.
    async fn quoted_local<T>(
        &self,
        work: usize,
        bytes: usize,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_with_fixed_transfers(work, bytes, action).await
    }

    /// Boxes a quoted continuation with its temporary future and four output transfers admitted.
    /// Payloads acquired while polling retain their own semantic or structural accounting.
    pub(super) async fn quoted_annotation_future<F: Future>(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<Pin<Box<F>>> {
        let quote = size_of::<F>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<F::Output>().checked_mul(4)?))
            .map(|bytes| (6, bytes))
            .ok_or(RunError::Contract(
                "quoted continuation byte quotation overflow",
            ));
        self.local_quoted_with_fixed_transfers(quote, || Box::pin(make()))
            .await
    }

    /// Parses one quoted expression and retains its syntax before lending it to local frames.
    pub(super) async fn local_parse_string_annotation<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        string: &ast::ExprStringLiteral,
        storage: &'expr StableStorage<ParsedAnnotation>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        let flags = self
            .quoted_local(1, 0, || builder.inference_flags())
            .await?;
        let parser = self
            .quoted_annotation_future(|| {
                parse_string_annotation_with(&builder.context, flags, string, self)
            })
            .await?;
        let Some(parsed) = parser.await? else {
            return self.quoted_local(1, 0, || None).await;
        };
        #[cfg(test)]
        observations::observe_before(observations::Stage::ParsedRetained);
        // The parsed tree is a structural producer's result. Arena admission covers its
        // header and slot; its AST/token backing retains the structural allocation policy.
        let parsed = self
            .quoted_local(3, size_of::<ParsedAnnotation>(), || {
                storage.allocate_admitted(self.access.endpoint(), 3, || parsed)
            })
            .await??;
        #[cfg(test)]
        observations::observe_after(observations::Stage::ParsedRetained);
        self.quoted_local(1, 0, || Some(parsed.expr())).await
    }

    /// Captures the quoted root's outer lookup key and the flags restored on return.
    pub(super) async fn local_prepare_string_annotation<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        string: &'expr ast::ExprStringLiteral,
        parsed: &'expr ast::Expr,
    ) -> RunResult<Scope<'expr>> {
        self.quoted_local(6, 0, || Scope::prepare(builder, string, parsed))
            .await
    }

    /// Records the original string expression before entering its parsed child.
    pub(super) async fn local_enter_string_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        scope: Scope<'_>,
    ) -> RunResult<()> {
        let quote = self
            .quoted_local(4, 0, || {
                table_merge::<ExpressionNodeKey>(
                    builder.string_annotations.len(),
                    builder.string_annotations.capacity(),
                    1,
                    0,
                )
                .map(|(quote, _)| quote)
            })
            .await?
            .ok_or(RunError::Contract(
                "quoted string-key storage quotation overflow",
            ))?;
        #[cfg(test)]
        observations::observe_before(observations::Stage::OriginalKeyStored);
        self.quoted_local(
            quote.work,
            Self::checked(quote.bytes.checked_add(size_of::<ExpressionNodeKey>() * 2))?,
            || {
                builder.string_annotations.insert(scope.original_key());
            },
        )
        .await?;
        #[cfg(test)]
        observations::observe_after(observations::Stage::OriginalKeyStored);
        self.quoted_local(3, 0, || {
            scope.enter(builder);
            #[cfg(test)]
            observations::observe_state(
                observations::StateStage::ParsedChild,
                scope.original_key(),
                scope.enclosing,
                builder.inference_flags(),
                builder.deferred_state,
            );
        })
        .await
    }

    /// Restores outer nesting and transfers any metadata from the parsed root to its string.
    pub(super) async fn local_finish_string_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        scope: Scope<'_>,
    ) -> RunResult<()> {
        let flags = self
            .quoted_local(
                Self::checked(builder.type_expression_flags.len().checked_add(4))?,
                0,
                || {
                    scope.restore(builder);
                    scope.parsed_flags(builder)
                },
            )
            .await?;
        #[cfg(test)]
        observations::observe_before(observations::Stage::FlagsTransferred);
        if flags.is_empty() {
            self.quoted_local(1, 0, || ()).await?;
        } else {
            let quote = self
                .quoted_local(4, 0, || {
                    table_merge::<(ExpressionNodeKey, TypeExpressionFlags)>(
                        builder.type_expression_flags.len(),
                        builder.type_expression_flags.capacity(),
                        1,
                        0,
                    )
                    .map(|(quote, _)| quote)
                })
                .await?
                .ok_or(RunError::Contract(
                    "quoted expression-flag storage quotation overflow",
                ))?;
            let bytes = Self::checked(
                quote
                    .bytes
                    .checked_add(size_of::<(ExpressionNodeKey, TypeExpressionFlags)>() * 2),
            )?;
            self.quoted_local(quote.work, bytes, || {
                scope.store_flags(builder, flags);
            })
            .await?;
        }
        #[cfg(test)]
        {
            observations::observe_after(observations::Stage::FlagsTransferred);
            observations::observe_state(
                observations::StateStage::QuoteFinished,
                scope.original_key(),
                scope.enclosing,
                builder.inference_flags(),
                builder.deferred_state,
            );
        }
        Ok(())
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> StringAnnotationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn source(&self, context: &InferContext<'db, 'ast>) -> RunResult<SourceText> {
        let source = self.access.source_text(context.file()).await?;
        self.quoted_local(1, 0, || source).await
    }

    async fn single_part<'string>(
        &self,
        string: &'string ast::ExprStringLiteral,
    ) -> RunResult<Option<&'string ast::StringLiteral>> {
        self.quoted_local(1, 0, || string.as_single_part_string())
            .await
    }

    async fn is_raw(&self, literal: &ast::StringLiteral) -> RunResult<bool> {
        self.quoted_local(2, 0, || literal.flags.prefix().is_raw())
            .await
    }

    async fn contents_match(
        &self,
        source: &SourceText,
        literal: &ast::StringLiteral,
    ) -> RunResult<bool> {
        let work = self
            .quoted_local(2, 0, || {
                usize::from(literal.content_range().len()).checked_add(2)
            })
            .await?
            .ok_or(RunError::Contract(
                "quoted content comparison quotation overflow",
            ))?;
        self.quoted_local(work, 0, || {
            &source[literal.content_range()] == literal.as_str()
        })
        .await
    }

    async fn parse(
        &self,
        source: &SourceText,
        literal: &ast::StringLiteral,
    ) -> RunResult<Result<ParsedAnnotation, ParseError>> {
        // Only structural parsing/indexing runs while the semantic task is parked.
        // Its returned header and selected future are admitted before entering that phase.
        self.quoted_local(
            4,
            size_of::<RunResult<Result<ParsedAnnotation, ParseError>>>() * 4,
            || (),
        )
        .await?;
        let parser = self
            .quoted_annotation_future(|| {
                self.access.endpoint().prepare_structural(|| {
                    Ok(ruff_db::parsed::parsed_string_annotation(
                        source.as_str(),
                        literal,
                    ))
                })
            })
            .await?;
        Ok(parser.await)
    }

    async fn raw_diagnostic(
        &self,
        _context: &InferContext<'db, 'ast>,
        _flags: InferenceFlags,
        _literal: &ast::StringLiteral,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::QuotedAnnotation(
            QuotedAnnotationOperation::RawStringDiagnostic,
        ))
        .await
    }

    async fn concatenation_diagnostic(
        &self,
        _context: &InferContext<'db, 'ast>,
        _string: &ast::ExprStringLiteral,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::QuotedAnnotation(
            QuotedAnnotationOperation::ConcatenatedStringDiagnostic,
        ))
        .await
    }

    async fn escape_diagnostic(
        &self,
        _context: &InferContext<'db, 'ast>,
        _flags: InferenceFlags,
        _string: &ast::ExprStringLiteral,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::QuotedAnnotation(
            QuotedAnnotationOperation::EscapeDiagnostic,
        ))
        .await
    }

    async fn syntax_diagnostic(
        &self,
        _context: &InferContext<'db, 'ast>,
        _string: &ast::ExprStringLiteral,
        _literal: &ast::StringLiteral,
        _error: &ParseError,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::QuotedAnnotation(
            QuotedAnnotationOperation::SyntaxDiagnostic,
        ))
        .await
    }
}
