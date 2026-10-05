//! Shared diagnostics for class type-parameter defaults and enclosing-name shadowing.

mod legacy_order;

pub(in crate::types) use legacy_order::{OrderTail, OrderText, OrderVariables, base_range as legacy_order_base_range};

use std::convert::Infallible;

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, DiagnosticMessage, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::Definition;

use super::{INVALID_GENERIC_CLASS, SHADOWED_TYPE_VARIABLE};
use crate::lint::LintMetadata;
use crate::types::context::InferContext;
use crate::types::context::lint_reporting::{LintReportMetadata, begin_lint_report, finish_lint_report};
use crate::types::function::FunctionType;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, ClassLiteral, KnownInstanceType, StaticClassLiteral, Type, TypeVarKind, binding_type};

/// Whether a disallowed reference names a remaining class parameter or a variable outside that list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types) enum DefaultReference {
    LaterParameter,
    OutOfScope,
}

/// The names and kind needed to report one variable shadowing an enclosing binding.
#[derive(Debug, Clone, Copy)]
pub(in crate::types) struct ShadowReport<'a, 'db> {
    pub typevar_name: &'a Name,
    pub owner_kind: &'a str,
    pub owner_name: &'a Name,
    pub range: TextRange,
    pub kind: TypeVarKind,
    pub other: BoundTypeVarInstance<'db>,
}

/// Borrowed fragments of the class-default and shadowing messages.
#[derive(Debug, Clone, Copy)]
pub(in crate::types) enum ReportText<'a> {
    LegacyOrder(OrderText<'a>),
    Default { name: &'a Name, referenced: &'a Name, reference: DefaultReference },
    DefinedHere(&'a Name),
    Shadow { name: &'a Name, owner_kind: &'a str, owner_name: &'a Name, kind: TypeVarKind },
    Primary { name: &'a Name, owner_kind: &'a str },
    Enclosing { name: &'a Name, kind: TypeVarKind },
}

impl<'a> ReportText<'a> {
    /// Selects one fragment without allocating or scanning the name text it borrows.
    pub(in crate::types) fn part(self, index: usize) -> Option<&'a str> {
        match self {
            Self::LegacyOrder(text) => text.part(index),
            Self::Default { name, referenced, reference } => match index {
                0 => Some("Default of `"),
                1 => Some(name.as_str()),
                2 => Some(match reference {
                    DefaultReference::LaterParameter => "` cannot reference later type parameter `",
                    DefaultReference::OutOfScope => "` cannot reference out-of-scope type variable `",
                }),
                3 => Some(referenced.as_str()),
                4 => Some("`"),
                _ => None,
            },
            Self::DefinedHere(name) => match index {
                0 => Some("`"), 1 => Some(name.as_str()), 2 => Some("` defined here"), _ => None,
            },
            Self::Shadow { name, owner_kind, owner_name, kind } => match index {
                0 => Some("Generic "), 1 => Some(owner_kind), 2 => Some(" `"),
                3 => Some(owner_name.as_str()), 4 => Some("` uses "),
                5 => Some(typevar_word(kind)), 6 => Some(" `"), 7 => Some(name.as_str()),
                8 => Some("` already bound by an enclosing scope"), _ => None,
            },
            Self::Primary { name, owner_kind } => match index {
                0 => Some("`"), 1 => Some(name.as_str()), 2 => Some("` used in "),
                3 => Some(owner_kind), 4 => Some(" definition here"), _ => None,
            },
            Self::Enclosing { name, kind } => match index {
                0 => Some(match kind {
                    TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => "ParamSpec",
                    TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => "TypeVarTuple",
                    TypeVarKind::LegacyTypeVar | TypeVarKind::Pep695TypeVar | TypeVarKind::TypingSelf | TypeVarKind::Pep613Alias => "Type variable",
                }),
                1 => Some(" `"), 2 => Some(name.as_str()),
                3 => Some("` is bound in this enclosing scope"), _ => None,
            },
        }
    }
}

const fn typevar_word(kind: TypeVarKind) -> &'static str {
    match kind {
        TypeVarKind::LegacyTypeVar | TypeVarKind::Pep695TypeVar | TypeVarKind::TypingSelf | TypeVarKind::Pep613Alias => "type variable",
        TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => "ParamSpec",
        TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => "TypeVarTuple",
    }
}

/// Selects definition annotations in the same order as the invalid-reference message.
pub(in crate::types) const fn default_annotation<'db>(bad: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, reference: DefaultReference, cursor: usize) -> Option<TypeVarInstance<'db>> {
    match (cursor, reference) {
        (0, _) => Some(bad),
        (1, DefaultReference::LaterParameter) => Some(referenced),
        _ => None,
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Resolves report inputs and constructs one complete owned diagnostic before publication.
    #[synchronous(SynchronousClassGenericReportEffects)]
    pub(in crate::types) trait ClassGenericReportEffects<'db> {
        type Error;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)] #[progress]
        async fn next_legacy_base(&self, bases: &[Type<'db>], cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn legacy_base_range(&self, node: &ast::StmtClassDef, bases: &[Type<'db>], index: Option<usize>) -> Result<TextRange, Self::Error>;
        #[operation(child)]
        async fn order_primary(&self, first: TypeVarInstance<'db>, remaining: &[TypeVarInstance<'db>]) -> Result<DiagnosticMessage, Self::Error>;
        #[operation(local)]
        async fn order_tail<'a>(&self, first: TypeVarInstance<'db>, remaining: &'a [TypeVarInstance<'db>]) -> Result<OrderTail<'a, 'db>, Self::Error>;
        #[operation(local)]
        async fn order_names(&self, variables: &OrderVariables<'_, 'db>) -> Result<Vec<&'db Name>, Self::Error>;
        #[operation(local)] #[progress]
        async fn next_order_variable(&self, variables: &mut OrderVariables<'_, 'db>) -> Result<Option<TypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn retain_order_name(&self, names: &mut Vec<&'db Name>, name: &'db Name) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn class_range(&self, class: StaticClassLiteral<'db>) -> Result<TextRange, Self::Error>;
        #[operation(child)]
        async fn begin(&self, lint: &'static LintMetadata, range: TextRange) -> Result<Option<LintReportMetadata>, Self::Error>;
        #[operation(source)]
        async fn name(&self, variable: TypeVarInstance<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(local)] #[progress]
        async fn next_annotation(&self, bad: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, reference: DefaultReference, cursor: &mut usize) -> Result<Option<TypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn definition_span(&self, variable: TypeVarInstance<'db>) -> Result<Option<Span>, Self::Error>;
        #[operation(source)]
        async fn binding_definition(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(child)]
        async fn binding_type(&self, definition: Definition<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn class_span(&self, class: ClassLiteral<'db>) -> Result<Span, Self::Error>;
        #[operation(child)]
        async fn function_span(&self, function: FunctionType<'db>) -> Result<Span, Self::Error>;
        #[operation(child)]
        async fn enclosing_span(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Span>, Self::Error>;
        #[operation(source)]
        async fn bound_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(child)]
        async fn message(&self, text: ReportText<'_>) -> Result<DiagnosticMessage, Self::Error>;
        #[operation(local)]
        async fn create(&self, metadata: &LintReportMetadata, headline: DiagnosticMessage, primary: Option<DiagnosticMessage>, annotation_capacity: usize) -> Result<Diagnostic, Self::Error>;
        #[operation(local)]
        async fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn annotate(&self, diagnostic: &mut Diagnostic, span: Span, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn additional_primary(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> Result<(), Self::Error>;
        #[operation(local)] #[progress]
        async fn next_part<'a>(&self, text: ReportText<'a>, cursor: &mut usize) -> Result<Option<&'a str>, Self::Error>;
        #[operation(local)]
        async fn add_length(&self, length: &mut usize, part: &str) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn buffer(&self, length: usize) -> Result<String, Self::Error>;
        #[operation(local)]
        async fn append(&self, buffer: &mut String, part: &str) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_message(&self, buffer: String) -> Result<DiagnosticMessage, Self::Error>;
    }

    /// Reports every non-defaulted parameter after the first default, at the first explicit Generic or Protocol base.
    #[synchronous(legacy_default_order_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values(ReportText::LegacyOrder, OrderText::Headline, OrderText::Concise, OrderText::EarlierDefault, ReportText::DefinedHere, DefaultReference::LaterParameter, INVALID_GENERIC_CLASS)]
    pub(in crate::types) async fn legacy_default_order_with<'db, E: ClassGenericReportEffects<'db>>(
        class: StaticClassLiteral<'db>, node: &ast::StmtClassDef,
        first_default: TypeVarInstance<'db>, first_offender: TypeVarInstance<'db>,
        later_offenders: &[TypeVarInstance<'db>], effects: &E,
    ) -> Result<(), E::Error> {
        let bases = effects.explicit_bases(class).await?;
        let mut base_cursor = 0;
        #[passive_state]
        let mut base_index = None;
        #[cursor_loop]
        while let Some(entry) = effects.next_legacy_base(bases, &mut base_cursor).await? {
            let (index, base) = entry;
            if let Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_) | KnownInstanceType::SubscriptedProtocol(_)) = base {
                base_index = Some(index);
                break;
            }
        }
        let range = effects.legacy_base_range(node, bases, base_index).await?;
        let Some(metadata) = effects.begin(&INVALID_GENERIC_CLASS, range).await? else { return Ok(()); };
        let headline = effects.message(ReportText::LegacyOrder(OrderText::Headline)).await?;
        let offender = effects.name(first_offender).await?;
        let default = effects.name(first_default).await?;
        let concise = effects.message(ReportText::LegacyOrder(OrderText::Concise { offender, default })).await?;
        let primary = effects.order_primary(first_offender, later_offenders).await?;
        let mut diagnostic = effects.create(&metadata, headline, Some(primary), 4).await?;
        effects.concise(&mut diagnostic, concise).await?;
        let default = effects.name(first_default).await?;
        let earlier = effects.message(ReportText::LegacyOrder(OrderText::EarlierDefault(default))).await?;
        effects.additional_primary(&metadata, &mut diagnostic, earlier).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(variable) = effects.next_annotation(first_default, first_offender, DefaultReference::LaterParameter, &mut cursor).await? {
            if let Some(span) = effects.definition_span(variable).await? {
                let name = effects.name(variable).await?;
                let message = effects.message(ReportText::DefinedHere(name)).await?;
                effects.annotate(&mut diagnostic, span, message).await?;
            }
        }
        effects.finish(metadata, diagnostic).await
    }

    /// Builds the primary message in declaration order, retaining the ordinary name-read sequence.
    /// Name reads can refuse, so matching [`crate::diagnostic::format_enumeration`] keeps the
    /// same name at each source-read boundary.
    #[synchronous(order_primary_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values(ReportText::LegacyOrder, OrderText::Single, OrderText::Multiple)]
    pub(in crate::types) async fn order_primary_with<'db, E: ClassGenericReportEffects<'db>>(
        first: TypeVarInstance<'db>, remaining: &[TypeVarInstance<'db>], effects: &E,
    ) -> Result<DiagnosticMessage, E::Error> {
        match effects.order_tail(first, remaining).await? {
            OrderTail::Single => {
                let name = effects.name(first).await?;
                effects.message(ReportText::LegacyOrder(OrderText::Single(name))).await
            }
            OrderTail::Multiple { last, penultimate, mut earlier } => {
                let last = effects.name(last).await?;
                let penultimate = effects.name(penultimate).await?;
                let mut names = effects.order_names(&earlier).await?;
                #[cursor_loop]
                while let Some(variable) = effects.next_order_variable(&mut earlier).await? {
                    let name = effects.name(variable).await?;
                    effects.retain_order_name(&mut names, name).await?;
                }
                effects.message(ReportText::LegacyOrder(OrderText::Multiple { earlier: &names, penultimate, last })).await
            }
        }
    }

    /// Reports a default that references a variable other than an earlier type parameter in the same class's generic context.
    #[synchronous(invalid_default_reference_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values(ReportText::Default, ReportText::DefinedHere, INVALID_GENERIC_CLASS)]
    pub(in crate::types) async fn invalid_default_reference_with<'db, E: ClassGenericReportEffects<'db>>(class: StaticClassLiteral<'db>, bad: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, reference: DefaultReference, effects: &E) -> Result<(), E::Error> {
        let range = effects.class_range(class).await?;
        let Some(metadata) = effects.begin(&INVALID_GENERIC_CLASS, range).await? else { return Ok(()); };
        let name = effects.name(bad).await?;
        let referenced_name = effects.name(referenced).await?;
        let headline = effects.message(ReportText::Default { name, referenced: referenced_name, reference }).await?;
        let mut diagnostic = effects.create(&metadata, headline, None, 3).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(variable) = effects.next_annotation(bad, referenced, reference, &mut cursor).await? {
            if let Some(span) = effects.definition_span(variable).await? {
                let name = effects.name(variable).await?;
                let message = effects.message(ReportText::DefinedHere(name)).await?;
                effects.annotate(&mut diagnostic, span, message).await?;
            }
        }
        effects.finish(metadata, diagnostic).await
    }

    /// Reports one shadowed variable and includes its enclosing declaration when that declaration has a class or function span.
    #[synchronous(shadow_report_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values(ReportText::Shadow, ReportText::Primary, ReportText::Enclosing, SHADOWED_TYPE_VARIABLE)]
    pub(in crate::types) async fn shadow_report_with<'db, E: ClassGenericReportEffects<'db>>(report: ShadowReport<'_, 'db>, effects: &E) -> Result<(), E::Error> {
        let Some(metadata) = effects.begin(&SHADOWED_TYPE_VARIABLE, report.range).await? else { return Ok(()); };
        let text = ReportText::Shadow { name: report.typevar_name, owner_kind: report.owner_kind, owner_name: report.owner_name, kind: report.kind };
        let headline = effects.message(text).await?;
        let primary = effects.message(ReportText::Primary { name: report.typevar_name, owner_kind: report.owner_kind }).await?;
        let mut diagnostic = effects.create(&metadata, headline, Some(primary), 2).await?;
        let concise = effects.message(text).await?;
        effects.concise(&mut diagnostic, concise).await?;
        if let Some(span) = effects.enclosing_span(report.other).await? {
            let kind = effects.bound_kind(report.other).await?;
            let message = effects.message(ReportText::Enclosing { name: report.typevar_name, kind }).await?;
            effects.annotate(&mut diagnostic, span, message).await?;
        }
        effects.finish(metadata, diagnostic).await
    }

    /// Resolves the enclosing variable's canonical binding and selects its class or function span.
    #[synchronous(enclosing_binding_span_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enclosing_binding_span_with<'db, E: ClassGenericReportEffects<'db>>(variable: BoundTypeVarInstance<'db>, effects: &E) -> Result<Option<Span>, E::Error> {
        let Some(definition) = effects.binding_definition(variable).await? else { return Ok(None); };
        match effects.binding_type(definition).await? {
            Type::ClassLiteral(class) => Ok(Some(effects.class_span(class).await?)),
            Type::FunctionLiteral(function) => Ok(Some(effects.function_span(function).await?)),
            _ => Ok(None),
        }
    }

    /// Builds one message from borrowed fragments, sizing its buffer before copying the text.
    #[synchronous(report_message_sync)]
    #[capabilities(effects = ClassGenericReportEffects)]
    #[passive_values()]
    pub(in crate::types) async fn report_message_with<'db, E: ClassGenericReportEffects<'db>>(text: ReportText<'_>, effects: &E) -> Result<DiagnosticMessage, E::Error> {
        let mut cursor = 0;
        let mut length = 0;
        #[cursor_loop]
        while let Some(part) = effects.next_part(text, &mut cursor).await? { effects.add_length(&mut length, part).await?; }
        let mut buffer = effects.buffer(length).await?;
        let mut copy_cursor = 0;
        #[cursor_loop]
        while let Some(part) = effects.next_part(text, &mut copy_cursor).await? { effects.append(&mut buffer, part).await?; }
        effects.finish_message(buffer).await
    }
}

/// Constructs the report's primary annotation and reserves its possible secondary annotations.
pub(in crate::types) fn create_diagnostic(metadata: &LintReportMetadata, headline: DiagnosticMessage, primary: Option<DiagnosticMessage>, annotation_capacity: usize) -> Diagnostic {
    let mut diagnostic = Diagnostic::new_with_capacity(DiagnosticId::Lint(metadata.id.name()), metadata.severity, headline, annotation_capacity, usize::from(metadata.verbose));
    let annotation = Annotation::primary(metadata.primary_span.clone());
    diagnostic.annotate(match primary { Some(message) => annotation.message(message), None => annotation });
    diagnostic
}

/// Resolves ordinary dependencies and publishes only the completed shared report.
#[derive(Debug)]
pub(super) struct OrdinaryClassGenericReportEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

impl<'db> SynchronousClassGenericReportEffects<'db> for OrdinaryClassGenericReportEffects<'_, 'db, '_> {
    type Error = Infallible;
    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Infallible> { Ok(class.explicit_bases(self.context.db())) }
    fn next_legacy_base(&self, bases: &[Type<'db>], cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Infallible> {
        let result = bases.get(*cursor).map(|base| (*cursor, *base));
        *cursor += usize::from(result.is_some());
        Ok(result)
    }
    fn legacy_base_range(&self, node: &ast::StmtClassDef, _bases: &[Type<'db>], index: Option<usize>) -> Result<TextRange, Infallible> {
        let index = index.expect(
            "It should not be possible for a class to have \
            a legacy generic context if it does \
            not inherit from `Protocol[]` or `Generic[]`",
        );
        let base = &node.bases()[index];
        Ok(base.as_subscript_expr().map(|subscript| &*subscript.slice).unwrap_or(base).range())
    }
    fn order_primary(&self, first: TypeVarInstance<'db>, remaining: &[TypeVarInstance<'db>]) -> Result<DiagnosticMessage, Infallible> { order_primary_sync(first, remaining, self) }
    fn order_tail<'a>(&self, first: TypeVarInstance<'db>, remaining: &'a [TypeVarInstance<'db>]) -> Result<OrderTail<'a, 'db>, Infallible> { Ok(OrderTail::new(first, remaining)) }
    fn order_names(&self, variables: &OrderVariables<'_, 'db>) -> Result<Vec<&'db Name>, Infallible> { Ok(Vec::with_capacity(variables.len())) }
    fn next_order_variable(&self, variables: &mut OrderVariables<'_, 'db>) -> Result<Option<TypeVarInstance<'db>>, Infallible> { Ok(variables.next()) }
    fn retain_order_name(&self, names: &mut Vec<&'db Name>, name: &'db Name) -> Result<(), Infallible> { names.push(name); Ok(()) }
    fn class_range(&self, class: StaticClassLiteral<'db>) -> Result<TextRange, Infallible> { Ok(class.header_range(self.context.db())) }
    fn begin(&self, lint: &'static LintMetadata, range: TextRange) -> Result<Option<LintReportMetadata>, Infallible> { Ok(begin_lint_report(self.context, lint, range)) }
    fn name(&self, variable: TypeVarInstance<'db>) -> Result<&'db Name, Infallible> { Ok(variable.name(self.context.db())) }
    fn next_annotation(&self, bad: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, reference: DefaultReference, cursor: &mut usize) -> Result<Option<TypeVarInstance<'db>>, Infallible> { let result = default_annotation(bad, referenced, reference, *cursor); *cursor += usize::from(result.is_some()); Ok(result) }
    fn definition_span(&self, variable: TypeVarInstance<'db>) -> Result<Option<Span>, Infallible> {
        let db = self.context.db();
        Ok(variable.definition(db).map(|definition| Span::from(definition.full_range(db, &parsed_module(db, definition.python_file(db)).load(db)))))
    }
    fn binding_definition(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Infallible> { Ok(variable.binding_context(self.context.db()).definition()) }
    fn binding_type(&self, definition: Definition<'db>) -> Result<Type<'db>, Infallible> { Ok(binding_type(self.context.db(), definition)) }
    fn class_span(&self, class: ClassLiteral<'db>) -> Result<Span, Infallible> { Ok(class.header_span(self.context.db())) }
    fn function_span(&self, function: FunctionType<'db>) -> Result<Span, Infallible> { Ok(function.spans(self.context.db()).signature) }
    fn enclosing_span(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Span>, Infallible> { enclosing_binding_span_sync(variable, self) }
    fn bound_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Infallible> { Ok(variable.kind(self.context.db())) }
    fn message(&self, text: ReportText<'_>) -> Result<DiagnosticMessage, Infallible> { report_message_sync(text, self) }
    fn create(&self, metadata: &LintReportMetadata, headline: DiagnosticMessage, primary: Option<DiagnosticMessage>, annotation_capacity: usize) -> Result<Diagnostic, Infallible> { Ok(create_diagnostic(metadata, headline, primary, annotation_capacity)) }
    fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Infallible> { diagnostic.set_concise_message(message); Ok(()) }
    fn annotate(&self, diagnostic: &mut Diagnostic, span: Span, message: DiagnosticMessage) -> Result<(), Infallible> { diagnostic.annotate(Annotation::secondary(span).message(message)); Ok(()) }
    fn additional_primary(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> Result<(), Infallible> { diagnostic.annotate(Annotation::primary(metadata.primary_span.clone()).message(message)); Ok(()) }
    fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> Result<(), Infallible> { finish_lint_report(self.context, metadata, diagnostic); Ok(()) }
    fn next_part<'a>(&self, text: ReportText<'a>, cursor: &mut usize) -> Result<Option<&'a str>, Infallible> { let result = text.part(*cursor); *cursor += usize::from(result.is_some()); Ok(result) }
    fn add_length(&self, length: &mut usize, part: &str) -> Result<(), Infallible> { *length += part.len(); Ok(()) }
    fn buffer(&self, length: usize) -> Result<String, Infallible> { Ok(String::with_capacity(length)) }
    fn append(&self, buffer: &mut String, part: &str) -> Result<(), Infallible> { buffer.push_str(part); Ok(()) }
    fn finish_message(&self, buffer: String) -> Result<DiagnosticMessage, Infallible> { Ok(DiagnosticMessage::from(buffer)) }
}
