use std::cmp::Reverse;

use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::str::Quote;
use ruff_python_ast::{
    self as ast, BuildId, Builder, Expr, ExprKind, NodeList, Rewriter, StringLiteralFlags, build,
};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_parser::typing::parse_type_annotation;
use ruff_python_semantic::{
    Binding, BindingKind, Modules, NodeId, ScopeKind, SemanticModel, analyze,
};
use ruff_text_size::Ranged;

use crate::Edit;
use crate::Locator;
use crate::fix::syntax;
use crate::rules::flake8_type_checking::settings::RuntimeSemantics;
use crate::settings::LinterSettings;

/// Represents the kind of an existing or potential typing-only annotation.
///
/// Note that the order of variants is important here. `Runtime` has the highest precedence when
/// calling [`TypingReference::combine`] on two references, followed by `Future`, `Quote`, and
/// `TypingOnly` with the lowest precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TypingReference {
    /// The reference is in a runtime-evaluated context.
    Runtime,
    /// The reference is in a runtime-ambiguous context.
    RuntimeAmbiguous,
    /// The reference is in a runtime-evaluated context, but the
    /// `lint.future-annotations` setting is enabled.
    ///
    /// This takes precedence if both quoting and future imports are enabled.
    Future,
    /// The reference is in a runtime-evaluated context, but the
    /// `lint.flake8-type-checking.quote-annotations` setting is enabled.
    Quote,
    /// The reference is in a typing-only context.
    TypingOnly,
}

impl TypingReference {
    /// Determine the kind of [`TypingReference`] for all references to a binding.
    pub(crate) fn from_references(
        binding: &Binding,
        semantic: &SemanticModel,
        settings: &LinterSettings,
    ) -> Self {
        let references = binding
            .references()
            .map(|reference_id| semantic.reference(reference_id));
        let mut kind = Self::TypingOnly;
        for reference in references {
            if reference.in_type_checking_block() {
                kind = kind.combine(Self::TypingOnly);
                continue;
            }

            // if we're not in a type checking block, we necessarily need to be within a
            // type definition to be considered a typing reference
            if !reference.in_type_definition() {
                return Self::Runtime;
            }

            if reference.in_runtime_ambiguous_annotation() {
                kind = kind.combine(Self::RuntimeAmbiguous);
                continue;
            }

            if reference.in_typing_only_annotation() || reference.in_string_type_definition() {
                kind = kind.combine(Self::TypingOnly);
                continue;
            }

            // prefer `from __future__ import annotations` to quoting
            if settings.future_annotations
                && !reference.in_typing_only_annotation()
                && reference.in_runtime_evaluated_annotation()
            {
                kind = kind.combine(Self::Future);
                continue;
            }

            if settings.flake8_type_checking.quote_annotations
                && reference.in_runtime_evaluated_annotation()
            {
                kind = kind.combine(Self::Quote);
                continue;
            }

            return Self::Runtime;
        }

        kind
    }

    /// Logically combine two `TypingReference`s into one.
    ///
    /// `TypingReference::Runtime` has the highest precedence, followed by
    /// `TypingReference::Future`, `TypingReference::Quote`, and then `TypingReference::TypingOnly`.
    fn combine(self, other: TypingReference) -> TypingReference {
        self.min(other)
    }

    fn is_runtime(self) -> bool {
        matches!(self, Self::Runtime)
    }
}

/// Returns `true` if the [`Binding`] represents a runtime-required import.
pub(crate) fn is_valid_runtime_import(
    binding: &Binding,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Import(..) | BindingKind::FromImport(..) | BindingKind::SubmoduleImport(..)
    ) {
        binding.context.is_runtime()
            && TypingReference::from_references(binding, semantic, settings).is_runtime()
    } else {
        false
    }
}

/// Returns the desired `RuntimeSemantics` for a function's annotations
pub(crate) fn function_annotation_runtime_semantics(
    function_def: ast::StmtFunctionDef<'_>,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> RuntimeSemantics {
    decorator_runtime_semantics(function_def.decorator_list(), semantic, settings)
}

/// Returns the desired `RuntimeSemantics` for a class's annotations
pub(crate) fn class_annotation_runtime_semantics(
    class_def: ast::StmtClassDef<'_>,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> RuntimeSemantics {
    let semantics = base_class_runtime_semantics(class_def, semantic, settings);
    if matches!(semantics, RuntimeSemantics::Required) {
        return semantics;
    }
    semantics.combine(decorator_runtime_semantics(
        class_def.decorator_list(),
        semantic,
        settings,
    ))
}

/// Returns `RuntimeSemantics` based on a class's base class.
fn base_class_runtime_semantics(
    class_def: ast::StmtClassDef<'_>,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> RuntimeSemantics {
    let base_classes = &settings.flake8_type_checking.runtime_evaluated_base_classes;
    if base_classes.is_empty() {
        return RuntimeSemantics::Default;
    }
    let mut result = RuntimeSemantics::Default;
    analyze::class::any_qualified_base_class(class_def, semantic, |qualified_name| {
        match base_classes
            .iter()
            .find(|(base_class, ..)| QualifiedName::from_dotted_name(base_class) == qualified_name)
        {
            Some((_, RuntimeSemantics::Required)) => {
                // Once we encounter a runtime required name we can stop looking
                // at the rest of the names by returning `true`
                result = RuntimeSemantics::Required;
                true
            }
            Some((_, semantics)) => {
                result = semantics.combine(result);
                false
            }
            _ => false,
        }
    });
    result
}

fn decorator_runtime_semantics(
    decorator_list: NodeList<'_, ast::kind::Decorator>,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> RuntimeSemantics {
    let decorators = &settings.flake8_type_checking.runtime_evaluated_decorators;
    if decorators.is_empty() {
        return RuntimeSemantics::Default;
    }
    let mut result = RuntimeSemantics::Default;
    for decorator in decorator_list {
        let expression = map_callable(decorator.expression());
        match semantic
            // First try to resolve the qualified name normally for cases like:
            // ```python
            // from mymodule import app
            //
            // @app.get(...)
            // def test(): ...
            //  ```
            .resolve_qualified_name(expression)
            // If we can't resolve the name, then try resolving the assignment
            // in order to support cases like:
            // ```python
            // from fastapi import FastAPI
            //
            // app = FastAPI()
            //
            // @app.get(...)
            // def test(): ...
            // ```
            .or_else(|| analyze::typing::resolve_assignment(expression, semantic))
            .and_then(|qualified_name| {
                decorators.iter().find(|(decorator, ..)| {
                    QualifiedName::from_dotted_name(decorator) == qualified_name
                })
            }) {
            Some((_, RuntimeSemantics::Required)) => {
                return RuntimeSemantics::Required;
            }
            Some((_, semantics)) => {
                result = semantics.combine(result);
            }
            _ => {}
        }
    }
    result
}

/// Returns `true` if an annotation will be inspected at runtime by the `dataclasses` module.
///
/// Specifically, detects whether an annotation is to either `dataclasses.InitVar` or
/// `typing.ClassVar` within a `@dataclass` class definition.
///
/// See: <https://docs.python.org/3/library/dataclasses.html#init-only-variables>
pub(crate) fn is_dataclass_meta_annotation(annotation: Expr<'_>, semantic: &SemanticModel) -> bool {
    if !semantic.seen_module(Modules::DATACLASSES) {
        return false;
    }

    // Determine whether the assignment is in a `@dataclass` class definition.
    if let ScopeKind::Class(class_def) = semantic.current_scope().kind {
        if class_def.decorator_list().iter().any(|decorator| {
            semantic
                .resolve_qualified_name(map_callable(decorator.expression()))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["dataclasses", "dataclass"])
                })
        }) {
            // Determine whether the annotation is `typing.ClassVar`, `dataclasses.InitVar`, or `dataclasses.KW_ONLY`.
            return semantic
                .resolve_qualified_name(map_subscript(annotation))
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["dataclasses", "InitVar" | "KW_ONLY"]
                    ) || semantic.match_typing_qualified_name(&qualified_name, "ClassVar")
                });
        }
    }

    false
}

/// Returns `true` if a function is registered as a `singledispatch` or `singledispatchmethod` interface.
///
/// For example, `fun` below is a `singledispatch` interface:
/// ```python
/// from functools import singledispatch
///
///
/// @singledispatch
/// def fun(arg, verbose=False):
///     ...
/// ```
fn is_singledispatch_interface(
    function_def: ast::StmtFunctionDef<'_>,
    semantic: &SemanticModel,
) -> bool {
    function_def.decorator_list().iter().any(|decorator| {
        semantic
            .resolve_qualified_name(decorator.expression())
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["functools", "singledispatch" | "singledispatchmethod"]
                )
            })
    })
}

/// Returns `true` if a function is registered as a `singledispatch` or `singledispatchmethod` implementation.
///
/// For example, `_` below is a `singledispatch` implementation:
/// ```python
/// from functools import singledispatch
///
///
/// @singledispatch
/// def fun(arg, verbose=False):
///     ...
///
/// @fun.register
/// def _(arg: int, verbose=False):
///     ...
/// ```
pub(crate) fn is_singledispatch_implementation(
    function_def: ast::StmtFunctionDef<'_>,
    semantic: &SemanticModel,
) -> bool {
    function_def.decorator_list().iter().any(|decorator| {
        let ExprKind::Attribute(attribute) = decorator.expression().classify() else {
            return false;
        };

        if attribute.attr().as_str() != "register" {
            return false;
        }

        let Some(id) = semantic.lookup_attribute(attribute.value()) else {
            return false;
        };

        let binding = semantic.binding(id);
        let Some(function_def) = binding
            .kind
            .as_function_definition()
            .map(|id| &semantic.scopes[*id])
            .and_then(|scope| scope.kind.as_function())
        else {
            return false;
        };

        is_singledispatch_interface(*function_def, semantic)
    })
}

/// Wrap a type annotation in quotes.
///
/// This requires more than just wrapping the reference itself in quotes. For example:
/// - When quoting `Series` in `Series[pd.Timestamp]`, we want `"Series[pd.Timestamp]"`.
/// - When quoting `kubernetes` in `kubernetes.SecurityContext`, we want `"kubernetes.SecurityContext"`.
/// - When quoting `Series` in `Series["pd.Timestamp"]`, we want `"Series[pd.Timestamp]"`.
/// - When quoting `Series` in `Series[Literal["pd.Timestamp"]]`, we want `"Series[Literal['pd.Timestamp']]"`.
///
/// In general, when expanding a component of a call chain, we want to quote the entire call chain.
pub(crate) fn quote_annotation(
    node_id: NodeId,
    semantic: &SemanticModel,
    stylist: &Stylist,
    locator: &Locator,
    flags: StringLiteralFlags,
) -> Edit {
    let expr = semantic.expression(node_id).expect("Expression not found");
    if let Some(parent_id) = semantic.parent_expression_id(node_id) {
        match semantic.expression(parent_id).map(Expr::classify) {
            Some(ExprKind::Subscript(parent)) if expr == parent.value() => {
                // If we're quoting the value of a subscript, we need to quote the entire
                // expression. For example, when quoting `DataFrame` in `DataFrame[int]`, we
                // should generate `"DataFrame[int]"`.
                return quote_annotation(parent_id, semantic, stylist, locator, flags);
            }
            Some(ExprKind::Attribute(parent)) if expr == parent.value() => {
                // If we're quoting the value of an attribute, we need to quote the entire
                // expression. For example, when quoting `DataFrame` in `pd.DataFrame`, we
                // should generate `"pd.DataFrame"`.
                return quote_annotation(parent_id, semantic, stylist, locator, flags);
            }
            Some(ExprKind::Call(parent)) if expr == parent.func() => {
                // If we're quoting the function of a call, we need to quote the entire
                // expression. For example, when quoting `DataFrame` in `DataFrame()`, we
                // should generate `"DataFrame()"`.
                return quote_annotation(parent_id, semantic, stylist, locator, flags);
            }
            Some(ExprKind::BinOp(parent)) if parent.op().is_bit_or() => {
                // If we're quoting the left or right side of a binary operation, we need to
                // quote the entire expression. For example, when quoting `DataFrame` in
                // `DataFrame | Series`, we should generate `"DataFrame | Series"`.
                return quote_annotation(parent_id, semantic, stylist, locator, flags);
            }
            _ => {}
        }
    }

    quote_type_expression(expr, semantic, stylist, locator, flags)
}

/// Wrap a type expression in quotes.
///
/// The caller expands the range to include any surrounding subscript, attribute,
/// call, or binary expression that needs to be quoted with it.
///
/// In most cases you want to call [`quote_annotation`] instead, which provides
/// that guarantee by expanding the expression before calling into this function.
pub(crate) fn quote_type_expression(
    expr: Expr<'_>,
    semantic: &SemanticModel,
    stylist: &Stylist,
    locator: &Locator,
    flags: StringLiteralFlags,
) -> Edit {
    // Quote the entire expression.
    let quote_annotator = QuoteAnnotator::new(semantic, stylist, locator, flags);

    Edit::range_replacement(quote_annotator.into_annotation(expr), expr.range())
}

/// Filter out any [`Edit`]s that are completely contained by any other [`Edit`].
pub(crate) fn filter_contained(edits: Vec<Edit>) -> Vec<Edit> {
    let mut edits = edits;

    // Sort such that the largest edits are prioritized.
    edits.sort_unstable_by_key(|edit| (edit.start(), Reverse(edit.end())));

    // Remove any edits that are completely contained by another edit.
    let mut filtered: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if !filtered
            .iter()
            .any(|filtered_edit| filtered_edit.range().contains_range(edit.range()))
        {
            filtered.push(edit);
        }
    }
    filtered
}

struct QuoteAnnotator<'a> {
    semantic: &'a SemanticModel<'a>,
    stylist: &'a Stylist<'a>,
    locator: &'a Locator<'a>,
    flags: StringLiteralFlags,
}

impl<'a> QuoteAnnotator<'a> {
    fn new(
        semantic: &'a SemanticModel<'a>,
        stylist: &'a Stylist<'a>,
        locator: &'a Locator<'a>,
        flags: StringLiteralFlags,
    ) -> Self {
        Self {
            semantic,
            stylist,
            locator,
            flags,
        }
    }

    fn into_annotation(mut self, expr: Expr<'_>) -> String {
        let mut builder = Builder::default();
        let expr = builder.rewrite_node(expr, &mut self);
        let inner = Generator::new(self.stylist.indentation(), self.stylist.line_ending())
            .expr(builder.get(expr));
        let quoted = syntax::string_literal(&mut builder, &inner, self.flags);
        Generator::from(self.stylist).expr(builder.get(quoted))
    }

    fn annotated_slice(
        &mut self,
        builder: &mut Builder,
        slice: Expr<'_>,
    ) -> BuildId<ast::kind::Expr> {
        let Some(tuple) = slice.as_tuple_expr() else {
            return builder.copy_node(slice);
        };
        // Only the first argument of Annotated is a type. Strings in its metadata
        // keep their values and use the opposite quote to the outer annotation.
        let mut quotes = QuoteRewriter(self.stylist.quote().opposite());
        let elts = builder.map_list(tuple.elts().iter().enumerate(), |builder, (index, elt)| {
            if index == 0 {
                builder.rewrite_node(elt, self)
            } else {
                builder.rewrite_node(elt, &mut quotes)
            }
        });
        builder
            .add(build::ExprTuple {
                range: tuple.range(),
                ctx: tuple.ctx(),
                parenthesized: tuple.parenthesized(),
                elts,
            })
            .into()
    }
}

impl Rewriter for QuoteAnnotator<'_> {
    fn replace_expr(
        &mut self,
        builder: &mut Builder,
        expr: Expr<'_>,
    ) -> Option<BuildId<ast::kind::Expr>> {
        match expr.classify() {
            ExprKind::Subscript(subscript) => {
                let Some(name) = self.semantic.resolve_qualified_name(subscript.value()) else {
                    return Some(builder.copy_node(expr));
                };
                let value = builder.copy_node(subscript.value());
                let slice = subscript.slice();
                let slice = if self.semantic.match_typing_qualified_name(&name, "Literal") {
                    // Literal arguments are values, even when a string parses as a type expression.
                    builder.rewrite_node(slice, &mut QuoteRewriter(self.stylist.quote().opposite()))
                } else if self
                    .semantic
                    .match_typing_qualified_name(&name, "Annotated")
                {
                    self.annotated_slice(builder, slice)
                } else {
                    builder.rewrite_node(slice, self)
                };
                Some(
                    builder
                        .add(build::ExprSubscript {
                            range: subscript.range(),
                            ctx: subscript.ctx(),
                            value,
                            slice,
                        })
                        .into(),
                )
            }
            ExprKind::StringLiteral(literal) => {
                // A parsed forward reference can contain further forward references.
                let annotation = parse_type_annotation(literal, self.locator.contents()).ok()?;
                Some(builder.rewrite_node(annotation.expression(), self))
            }
            _ => None,
        }
    }
}

struct QuoteRewriter(Quote);

impl Rewriter for QuoteRewriter {
    fn replace_string_literal(
        &mut self,
        builder: &mut Builder,
        literal: ast::StringLiteral<'_>,
    ) -> Option<BuildId<ast::kind::StringLiteral>> {
        Some(builder.add(build::StringLiteral {
            range: literal.range(),
            flags: literal.flags().with_quote_style(self.0),
            value: literal.value(),
        }))
    }
}
