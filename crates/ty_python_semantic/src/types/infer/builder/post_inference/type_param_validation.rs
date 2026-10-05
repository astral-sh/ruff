//! Shared PEP 695 parameter scans and construction of their lint diagnostics.

use std::convert::Infallible;

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, DiagnosticMessage};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};

use crate::lint::LintMetadata;
use crate::types::context::InferContext;
use crate::types::context::lint_reporting::{
    LintReportMetadata, begin_lint_report, finish_lint_report,
};
use crate::types::diagnostic::{INVALID_TYPE_FORM, INVALID_TYPE_VARIABLE_DEFAULT};

/// The declaration whose parameter list determines the duplicate-pack diagnostic's wording.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TypeParameterOwner<'a> {
    GenericClass(&'a Name),
    TypeAlias(&'a Name),
}

/// Borrows a parameter list while retaining the first pack and later default-bearing parameters.
#[derive(Debug)]
pub(in crate::types::infer::builder) struct ParameterScan<'a> {
    pub params: &'a ast::TypeParams,
    pub cursor: usize,
    pub first: Option<&'a ast::TypeParamTypeVarTuple>,
    pub defaults: Vec<&'a ast::TypeParam>,
}

impl<'a> ParameterScan<'a> {
    pub(in crate::types::infer::builder) const fn new(params: &'a ast::TypeParams) -> Self {
        Self { params, cursor: 0, first: None, defaults: Vec::new() }
    }

    /// Inspects one parameter for the duplicate-pack check, retaining the first pack.
    pub(in crate::types::infer::builder) fn next_single(&mut self) -> Option<SinglePackStep<'a>> {
        let param = self.params.get(self.cursor)?;
        self.cursor += 1;
        match param {
            ast::TypeParam::TypeVar(_) | ast::TypeParam::ParamSpec(_) => Some(SinglePackStep::Continue),
            ast::TypeParam::TypeVarTuple(pack) => match self.first {
                Some(first) => Some(SinglePackStep::Duplicate { first, additional: pack }),
                None => {
                    self.first = Some(pack);
                    Some(SinglePackStep::Continue)
                }
            },
        }
    }

    /// Inspects one parameter for a default after the first pack; the pack's own default is excluded.
    pub(in crate::types::infer::builder) fn next_default(&mut self) -> Option<DefaultStep<'a>> {
        let param = self.params.get(self.cursor)?;
        self.cursor += 1;
        if self.first.is_some() {
            return Some(if param.default().is_some() {
                DefaultStep::Later(param)
            } else {
                DefaultStep::Continue
            });
        }
        match param {
            ast::TypeParam::TypeVarTuple(pack) => self.first = Some(pack),
            ast::TypeParam::TypeVar(_) | ast::TypeParam::ParamSpec(_) => {}
        }
        Some(DefaultStep::Continue)
    }
}

/// The result of inspecting one parameter for an additional pack.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) enum SinglePackStep<'a> {
    Continue,
    Duplicate { first: &'a ast::TypeParamTypeVarTuple, additional: &'a ast::TypeParamTypeVarTuple },
}

/// The result of inspecting one parameter for a default following the first pack.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) enum DefaultStep<'a> {
    Continue,
    Later(&'a ast::TypeParam),
}

/// Borrowed evidence for one complete parameter-list diagnostic.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) enum ParameterReport<'a> {
    Duplicate {
        owner: TypeParameterOwner<'a>,
        first: &'a ast::TypeParamTypeVarTuple,
        additional: &'a ast::TypeParamTypeVarTuple,
    },
    Defaults { first: &'a ast::TypeParamTypeVarTuple, primary: &'a ast::TypeParam, rest: &'a [&'a ast::TypeParam] },
}

impl<'a> ParameterReport<'a> {
    pub(in crate::types::infer::builder) const fn lint(self) -> &'static LintMetadata {
        match self {
            Self::Duplicate { .. } => &INVALID_TYPE_FORM,
            Self::Defaults { .. } => &INVALID_TYPE_VARIABLE_DEFAULT,
        }
    }

    pub(in crate::types::infer::builder) fn range(self) -> TextRange {
        match self {
            Self::Duplicate { additional, .. } => additional.range(),
            Self::Defaults { primary, .. } => primary.range(),
        }
    }

    pub(in crate::types::infer::builder) fn annotation_count(self) -> usize {
        match self {
            Self::Duplicate { .. } => 2,
            Self::Defaults { rest, .. } => rest.len() + 2,
        }
    }

    /// Selects one annotation without constructing its owned message.
    pub(in crate::types::infer::builder) fn annotation(self, index: usize) -> Option<ParameterAnnotation<'a>> {
        match self {
            Self::Duplicate { first, additional, .. } => match index {
                0 => Some(ParameterAnnotation { range: additional.range(), kind: AnnotationKind::Primary, message: ParameterMessage::Named(&additional.name.id, " is an additional TypeVarTuple") }),
                1 => Some(ParameterAnnotation { range: first.range(), kind: AnnotationKind::Secondary, message: ParameterMessage::Named(&first.name.id, " is the first TypeVarTuple") }),
                _ => None,
            },
            Self::Defaults { first, primary, rest } => {
                if index == 0 {
                    Some(ParameterAnnotation { range: primary.range(), kind: AnnotationKind::Primary, message: ParameterMessage::Named(&primary.name().id, " has a default") })
                } else if let Some(param) = rest.get(index - 1) {
                    Some(ParameterAnnotation { range: param.range(), kind: AnnotationKind::Secondary, message: ParameterMessage::Named(&param.name().id, " also has a default") })
                } else if index == rest.len() + 1 {
                    Some(ParameterAnnotation { range: first.range(), kind: AnnotationKind::Secondary, message: ParameterMessage::Named(&first.name.id, " is a TypeVarTuple") })
                } else {
                    None
                }
            }
        }
    }
}

/// Whether a parameter annotation is the diagnostic's primary span or a related span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types::infer::builder) enum AnnotationKind { Primary, Secondary }

/// One annotation's borrowed range, role and message fragments.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) struct ParameterAnnotation<'a> {
    pub range: TextRange,
    pub kind: AnnotationKind,
    pub message: ParameterMessage<'a>,
}

/// Describes diagnostic text as borrowed fragments, including the ordered name enumeration.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) enum ParameterMessage<'a> {
    Text(&'static str),
    Named(&'a Name, &'static str),
    Duplicate(TypeParameterOwner<'a>),
    Defaults { first: &'a Name, primary: &'a ast::TypeParam, rest: &'a [&'a ast::TypeParam] },
}

impl<'a> ParameterMessage<'a> {
    /// Returns one text fragment; each call does constant work even for a long name enumeration.
    pub(in crate::types::infer::builder) fn part(self, index: usize) -> Option<&'a str> {
        match self {
            Self::Text(text) => (index == 0).then_some(text),
            Self::Named(name, suffix) => match index { 0 | 2 => Some("`"), 1 => Some(name.as_str()), 3 => Some(suffix), _ => None },
            Self::Duplicate(owner) => match index {
                0 => Some(match owner { TypeParameterOwner::GenericClass(_) => "Generic class `", TypeParameterOwner::TypeAlias(_) => "Type alias `" }),
                1 => Some(match owner { TypeParameterOwner::GenericClass(name) | TypeParameterOwner::TypeAlias(name) => name.as_str() }),
                2 => Some("` cannot have multiple `TypeVarTuple` type parameters"),
                _ => None,
            },
            Self::Defaults { first, primary, rest } => {
                if index == 0 { return Some(if rest.is_empty() { "Type parameter " } else { "Type parameters " }); }
                let offset = index - 1;
                let parameter = offset / 4;
                let count = rest.len() + 1;
                let param = if parameter == 0 { Some(primary) } else { rest.get(parameter - 1).copied() };
                if let Some(param) = param {
                    return match offset % 4 {
                        0 | 2 => Some("`"),
                        1 => Some(param.name().id.as_str()),
                        3 => Some(if parameter + 1 == count {
                            if rest.is_empty() { " with a default follows TypeVarTuple `" } else { " with defaults follow TypeVarTuple `" }
                        } else if parameter + 2 == count { " and " } else { ", " }),
                        _ => None,
                    };
                }
                match offset - count * 4 { 0 => Some(first.as_str()), 1 => Some("`"), _ => None }
            }
        }
    }
}

/// Selects finite report descriptors; traversal and owned mutations remain effect operations.
#[derive(Debug)]
pub(in crate::types::infer::builder) struct ParameterValidationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Executes parameter-list scans and owned report construction with ordinary or admitted effects.
    #[synchronous(SynchronousParameterValidationEffects)]
    pub(in crate::types::infer::builder) trait ParameterValidationEffects {
        type Error;
        #[operation(local)]
        async fn scan<'a>(&self, params: &'a ast::TypeParams) -> Result<ParameterScan<'a>, Self::Error>;
        #[operation(local)] #[progress]
        async fn next_single<'a>(&self, scan: &mut ParameterScan<'a>) -> Result<Option<SinglePackStep<'a>>, Self::Error>;
        #[operation(local)] #[progress]
        async fn next_default<'a>(&self, scan: &mut ParameterScan<'a>) -> Result<Option<DefaultStep<'a>>, Self::Error>;
        #[operation(local)]
        async fn retain_default<'a>(&self, scan: &mut ParameterScan<'a>, param: &'a ast::TypeParam) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report(&self, report: ParameterReport<'_>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn begin(&self, report: ParameterReport<'_>) -> Result<Option<LintReportMetadata>, Self::Error>;
        #[operation(child)]
        async fn message(&self, message: ParameterMessage<'_>) -> Result<DiagnosticMessage, Self::Error>;
        #[operation(local)]
        async fn create(&self, metadata: &LintReportMetadata, report: ParameterReport<'_>, headline: DiagnosticMessage) -> Result<Diagnostic, Self::Error>;
        #[operation(local)]
        async fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(local)] #[progress]
        async fn next_annotation<'a>(&self, report: ParameterReport<'a>, cursor: &mut usize) -> Result<Option<ParameterAnnotation<'a>>, Self::Error>;
        #[operation(local)]
        async fn annotate(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, annotation: ParameterAnnotation<'_>, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn info(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> Result<(), Self::Error>;
        #[operation(local)] #[progress]
        async fn next_part<'a>(&self, message: ParameterMessage<'a>, cursor: &mut usize) -> Result<Option<&'a str>, Self::Error>;
        #[operation(local)]
        async fn add_length(&self, length: &mut usize, part: &str) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn buffer(&self, length: usize) -> Result<String, Self::Error>;
        #[operation(local)]
        async fn append(&self, buffer: &mut String, part: &str) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_message(&self, buffer: String) -> Result<DiagnosticMessage, Self::Error>;
    }

    #[finite_capability]
    impl ParameterValidationFacts {
        fn defaults<'a>(&self, scan: &'a ParameterScan<'a>) -> Option<ParameterReport<'a>> {
            let (primary, rest) = scan.defaults.split_first()?;
            Some(ParameterReport::Defaults { first: scan.first?, primary, rest })
        }
        fn headline<'a>(&self, report: ParameterReport<'a>) -> ParameterMessage<'a> {
            match report {
                ParameterReport::Duplicate { owner, .. } => ParameterMessage::Duplicate(owner),
                ParameterReport::Defaults { .. } => ParameterMessage::Text("Type parameters with defaults cannot follow a TypeVarTuple parameter"),
            }
        }
        fn concise<'a>(&self, report: ParameterReport<'a>) -> Option<ParameterMessage<'a>> {
            match report {
                ParameterReport::Duplicate { .. } => None,
                ParameterReport::Defaults { first, primary, rest } => Some(ParameterMessage::Defaults { first: &first.name.id, primary, rest }),
            }
        }
        fn annotation_message<'a>(&self, annotation: ParameterAnnotation<'a>) -> ParameterMessage<'a> { annotation.message }
        fn link(&self, report: ParameterReport<'_>) -> ParameterMessage<'static> {
            ParameterMessage::Text(match report {
                ParameterReport::Duplicate { .. } => "See https://typing.python.org/en/latest/spec/generics.html#multiple-type-variable-tuples-not-allowed",
                ParameterReport::Defaults { .. } => "See https://typing.python.org/en/latest/spec/generics.html#defaults-following-typevartuple",
            })
        }
    }

    #[synchronous(check_single_pack_sync)]
    #[capabilities(effects = ParameterValidationEffects)]
    #[passive_values(ParameterReport::Duplicate)]
    pub(in crate::types::infer::builder) async fn check_single_pack_with<E: ParameterValidationEffects>(params: &ast::TypeParams, owner: TypeParameterOwner<'_>, effects: &E) -> Result<(), E::Error> {
        let mut scan = effects.scan(params).await?;
        #[cursor_loop]
        while let Some(step) = effects.next_single(&mut scan).await? {
            match step {
                SinglePackStep::Continue => {}
                SinglePackStep::Duplicate { first, additional } => {
                    effects.report(ParameterReport::Duplicate { owner, first, additional }).await?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    #[synchronous(check_defaults_after_pack_sync)]
    #[capabilities(effects = ParameterValidationEffects, facts = ParameterValidationFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_defaults_after_pack_with<E: ParameterValidationEffects>(params: &ast::TypeParams, facts: ParameterValidationFacts, effects: &E) -> Result<(), E::Error> {
        let mut scan = effects.scan(params).await?;
        #[cursor_loop]
        while let Some(step) = effects.next_default(&mut scan).await? {
            match step {
                DefaultStep::Continue => {}
                DefaultStep::Later(param) => effects.retain_default(&mut scan, param).await?,
            }
        }
        if let Some(report) = facts.defaults(&scan) { effects.report(report).await?; }
        Ok(())
    }

    #[synchronous(parameter_report_sync)]
    #[capabilities(effects = ParameterValidationEffects, facts = ParameterValidationFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn parameter_report_with<E: ParameterValidationEffects>(report: ParameterReport<'_>, facts: ParameterValidationFacts, effects: &E) -> Result<(), E::Error> {
        let Some(metadata) = effects.begin(report).await? else { return Ok(()); };
        let headline = effects.message(facts.headline(report)).await?;
        let mut diagnostic = effects.create(&metadata, report, headline).await?;
        if let Some(concise) = facts.concise(report) {
            let message = effects.message(concise).await?;
            effects.concise(&mut diagnostic, message).await?;
        }
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(annotation) = effects.next_annotation(report, &mut cursor).await? {
            let message = effects.message(facts.annotation_message(annotation)).await?;
            effects.annotate(&metadata, &mut diagnostic, annotation, message).await?;
        }
        let link = effects.message(facts.link(report)).await?;
        effects.info(&mut diagnostic, link).await?;
        effects.finish(metadata, diagnostic).await?;
        Ok(())
    }

    #[synchronous(parameter_message_sync)]
    #[capabilities(effects = ParameterValidationEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn parameter_message_with<E: ParameterValidationEffects>(message: ParameterMessage<'_>, effects: &E) -> Result<DiagnosticMessage, E::Error> {
        let mut cursor = 0;
        let mut length = 0;
        #[cursor_loop]
        while let Some(part) = effects.next_part(message, &mut cursor).await? { effects.add_length(&mut length, part).await?; }
        let mut buffer = effects.buffer(length).await?;
        let mut copy_cursor = 0;
        #[cursor_loop]
        while let Some(part) = effects.next_part(message, &mut copy_cursor).await? { effects.append(&mut buffer, part).await?; }
        effects.finish_message(buffer).await
    }
}

/// Constructs a unique diagnostic whose content will fit in its prepared vectors.
pub(in crate::types::infer::builder) fn create_parameter_diagnostic(metadata: &LintReportMetadata, report: ParameterReport<'_>, headline: DiagnosticMessage) -> Diagnostic {
    Diagnostic::new_with_capacity(DiagnosticId::Lint(metadata.id.name()), metadata.severity, headline, report.annotation_count(), 1 + usize::from(metadata.verbose))
}

/// Adds a prepared annotation while retaining the unique diagnostic's existing capacities.
pub(in crate::types::infer::builder) fn annotate_parameter(metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, annotation: ParameterAnnotation<'_>, message: DiagnosticMessage) {
    let span = metadata.primary_span.clone().with_range(annotation.range);
    let annotation = match annotation.kind { AnnotationKind::Primary => Annotation::primary(span), AnnotationKind::Secondary => Annotation::secondary(span) };
    diagnostic.annotate(annotation.message(message));
}

/// Runs the shared checks immediately and inserts only finished diagnostics into the ordinary context.
struct OrdinaryParameterValidationEffects<'a, 'db, 'ast> { context: &'a InferContext<'db, 'ast> }

impl SynchronousParameterValidationEffects for OrdinaryParameterValidationEffects<'_, '_, '_> {
    type Error = Infallible;
    fn scan<'a>(&self, params: &'a ast::TypeParams) -> Result<ParameterScan<'a>, Infallible> { Ok(ParameterScan::new(params)) }
    fn next_single<'a>(&self, scan: &mut ParameterScan<'a>) -> Result<Option<SinglePackStep<'a>>, Infallible> { Ok(scan.next_single()) }
    fn next_default<'a>(&self, scan: &mut ParameterScan<'a>) -> Result<Option<DefaultStep<'a>>, Infallible> { Ok(scan.next_default()) }
    fn retain_default<'a>(&self, scan: &mut ParameterScan<'a>, param: &'a ast::TypeParam) -> Result<(), Infallible> { scan.defaults.push(param); Ok(()) }
    fn report(&self, report: ParameterReport<'_>) -> Result<(), Infallible> { parameter_report_sync(report, ParameterValidationFacts, self) }
    fn begin(&self, report: ParameterReport<'_>) -> Result<Option<LintReportMetadata>, Infallible> { Ok(begin_lint_report(self.context, report.lint(), report.range())) }
    fn message(&self, message: ParameterMessage<'_>) -> Result<DiagnosticMessage, Infallible> { parameter_message_sync(message, self) }
    fn create(&self, metadata: &LintReportMetadata, report: ParameterReport<'_>, headline: DiagnosticMessage) -> Result<Diagnostic, Infallible> { Ok(create_parameter_diagnostic(metadata, report, headline)) }
    fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Infallible> { diagnostic.set_concise_message(message); Ok(()) }
    fn next_annotation<'a>(&self, report: ParameterReport<'a>, cursor: &mut usize) -> Result<Option<ParameterAnnotation<'a>>, Infallible> { let annotation = report.annotation(*cursor); *cursor += usize::from(annotation.is_some()); Ok(annotation) }
    fn annotate(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, annotation: ParameterAnnotation<'_>, message: DiagnosticMessage) -> Result<(), Infallible> { annotate_parameter(metadata, diagnostic, annotation, message); Ok(()) }
    fn info(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Infallible> { diagnostic.info(message); Ok(()) }
    fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> Result<(), Infallible> { finish_lint_report(self.context, metadata, diagnostic); Ok(()) }
    fn next_part<'a>(&self, message: ParameterMessage<'a>, cursor: &mut usize) -> Result<Option<&'a str>, Infallible> { let part = message.part(*cursor); *cursor += usize::from(part.is_some()); Ok(part) }
    fn add_length(&self, length: &mut usize, part: &str) -> Result<(), Infallible> { *length += part.len(); Ok(()) }
    fn buffer(&self, length: usize) -> Result<String, Infallible> { Ok(String::with_capacity(length)) }
    fn append(&self, buffer: &mut String, part: &str) -> Result<(), Infallible> { buffer.push_str(part); Ok(()) }
    fn finish_message(&self, buffer: String) -> Result<DiagnosticMessage, Infallible> { Ok(DiagnosticMessage::from(buffer)) }
}

/// Check that a PEP 695 class or type alias parameter list contains at most one `TypeVarTuple`.
///
/// Classes and type aliases can be explicitly specialized, so multiple `TypeVarTuple`s would make
/// it ambiguous which pack consumes each type argument. Generic functions cannot be explicitly
/// specialized and intentionally do not use this validation.
pub(crate) fn check_single_typevar_tuple_pep695(context: &InferContext<'_, '_>, type_params: &ast::TypeParams, owner: TypeParameterOwner<'_>) {
    match check_single_pack_sync(type_params, owner, &OrdinaryParameterValidationEffects { context }) { Ok(()) => {}, Err(never) => match never {} }
}

/// Check that no type parameter with a default follows a `TypeVarTuple` in a PEP 695
/// type parameter list. This is prohibited by the typing spec because a `TypeVarTuple`
/// consumes all remaining positional type arguments.
///
/// This check is used for both classes and type aliases with PEP 695 type parameters.
pub(crate) fn check_no_default_after_typevar_tuple_pep695(context: &InferContext<'_, '_>, type_params: &ast::TypeParams) {
    match check_defaults_after_pack_sync(type_params, ParameterValidationFacts, &OrdinaryParameterValidationEffects { context }) { Ok(()) => {}, Err(never) => match never {} }
}
