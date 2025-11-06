use std::cmp::Ordering;

use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::source_text;
use ruff_diagnostics::Edit;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{Token, TokenAt, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::{
    Completion as SemanticCompletion, ModuleName, NameKind, SemanticModel,
    types::{CycleDetector, Type},
};

use crate::docstring::Docstring;
use crate::find_node::covering_node;
use crate::goto::DefinitionsOrTargets;
use crate::importer::{ImportRequest, Importer};
use crate::symbols::QueryPattern;
use crate::{Db, all_symbols};

#[derive(Clone, Debug)]
pub struct Completion<'db> {
    /// The label shown to the user for this suggestion.
    pub name: Name,
    /// The text that should be inserted at the cursor
    /// when the completion is selected.
    ///
    /// When this is not set, `name` is used.
    pub insert: Option<Box<str>>,
    /// The type of this completion, if available.
    ///
    /// Generally speaking, this is always available
    /// *unless* this was a completion corresponding to
    /// an unimported symbol. In that case, computing the
    /// type of all such symbols could be quite expensive.
    pub ty: Option<Type<'db>>,
    /// The "kind" of this completion.
    ///
    /// When this is set, it takes priority over any kind
    /// inferred from `ty`.
    ///
    /// Usually this is set when `ty` is `None`, since it
    /// may be cheaper to compute at scale (e.g., for
    /// unimported symbol completions).
    ///
    /// Callers should use [`Completion::kind`] to get the
    /// kind, which will take type information into account
    /// if this kind is not present.
    pub kind: Option<CompletionKind>,
    /// The name of the module that this completion comes from.
    ///
    /// This is generally only present when this is a completion
    /// suggestion for an unimported symbol.
    pub module_name: Option<&'db ModuleName>,
    /// An import statement to insert (or ensure is already
    /// present) when this completion is selected.
    pub import: Option<Edit>,
    /// Whether this suggestion came from builtins or not.
    ///
    /// At time of writing (2025-06-26), this information
    /// doesn't make it into the LSP response. Instead, we
    /// use it mainly in tests so that we can write less
    /// noisy tests.
    pub builtin: bool,
    /// Whether this item only exists for type checking purposes and
    /// will be missing at runtime
    pub is_type_check_only: bool,
    /// The documentation associated with this item, if
    /// available.
    pub documentation: Option<Docstring>,
}

impl<'db> Completion<'db> {
    fn from_semantic_completion(
        db: &'db dyn Db,
        semantic: SemanticCompletion<'db>,
    ) -> Completion<'db> {
        let definition = semantic
            .ty
            .and_then(|ty| DefinitionsOrTargets::from_ty(db, ty));
        let documentation = definition.and_then(|def| def.docstring(db));
        let is_type_check_only = semantic.is_type_check_only(db);
        Completion {
            name: semantic.name,
            insert: None,
            ty: semantic.ty,
            kind: None,
            module_name: None,
            import: None,
            builtin: semantic.builtin,
            is_type_check_only,
            documentation,
        }
    }

    /// Returns the "kind" of this completion.
    ///
    /// This is meant to be a very general classification of this completion.
    /// Typically, this is communicated from the LSP server to a client, and
    /// the client uses this information to help improve the UX (perhaps by
    /// assigning an icon of some kind to the completion).
    pub fn kind(&self, db: &'db dyn Db) -> Option<CompletionKind> {
        type CompletionKindVisitor<'db> =
            CycleDetector<CompletionKind, Type<'db>, Option<CompletionKind>>;

        fn imp<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            visitor: &CompletionKindVisitor<'db>,
        ) -> Option<CompletionKind> {
            Some(match ty {
                Type::FunctionLiteral(_)
                | Type::DataclassDecorator(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_) => CompletionKind::Function,
                Type::BoundMethod(_) | Type::KnownBoundMethod(_) => CompletionKind::Method,
                Type::ModuleLiteral(_) => CompletionKind::Module,
                Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                    CompletionKind::Class
                }
                // This is a little weird for "struct." I'm mostly interpreting
                // "struct" here as a more general "object." ---AG
                Type::NominalInstance(_)
                | Type::PropertyInstance(_)
                | Type::BoundSuper(_)
                | Type::TypedDict(_) => CompletionKind::Struct,
                Type::IntLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::TypeIs(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_) => CompletionKind::Value,
                Type::EnumLiteral(_) => CompletionKind::Enum,
                Type::ProtocolInstance(_) => CompletionKind::Interface,
                Type::TypeVar(_) => CompletionKind::TypeParameter,
                Type::Union(union) => union
                    .elements(db)
                    .iter()
                    .find_map(|&ty| imp(db, ty, visitor))?,
                Type::Intersection(intersection) => intersection
                    .iter_positive(db)
                    .find_map(|ty| imp(db, ty, visitor))?,
                Type::Dynamic(_)
                | Type::Never
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy => return None,
                Type::TypeAlias(alias) => {
                    visitor.visit(ty, || imp(db, alias.value_type(db), visitor))?
                }
            })
        }
        self.kind.or_else(|| {
            self.ty
                .and_then(|ty| imp(db, ty, &CompletionKindVisitor::default()))
        })
    }
}

/// The "kind" of a completion.
///
/// This is taken directly from the LSP completion specification:
/// <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind>
///
/// The idea here is that [`Completion::kind`] defines the mapping to this from
/// `Type` (and possibly other information), which might be interesting and
/// contentious. Then the outer edges map this to the LSP types, which is
/// expected to be mundane and boring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

#[derive(Clone, Debug, Default)]
pub struct CompletionSettings {
    pub auto_import: bool,
}

pub fn completion<'db>(
    db: &'db dyn Db,
    settings: &CompletionSettings,
    file: File,
    offset: TextSize,
) -> Vec<Completion<'db>> {
    let parsed = parsed_module(db, file).load(db);

    let tokens = tokens_start_before(parsed.tokens(), offset);

    if is_in_comment(tokens) || is_in_string(tokens) || is_in_definition_place(db, tokens, file) {
        return vec![];
    }

    let typed = find_typed_text(db, file, &parsed, offset);
    let typed_query = typed
        .as_deref()
        .map(QueryPattern::new)
        .unwrap_or_else(QueryPattern::matches_all_symbols);

    let Some(target_token) = CompletionTargetTokens::find(&parsed, offset) else {
        return vec![];
    };
    let Some(target) = target_token.ast(&parsed, offset) else {
        return vec![];
    };

    let model = SemanticModel::new(db, file);
    let (semantic_completions, scoped) = match target {
        CompletionTargetAst::ObjectDot { expr } => (model.attribute_completions(expr), None),
        CompletionTargetAst::ObjectDotInImport { import, name } => {
            (model.import_submodule_completions(import, name), None)
        }
        CompletionTargetAst::ObjectDotInImportFrom { import } => {
            (model.from_import_submodule_completions(import), None)
        }
        CompletionTargetAst::ImportFrom { import, name } => {
            (model.from_import_completions(import, name), None)
        }
        CompletionTargetAst::Import { .. } | CompletionTargetAst::ImportViaFrom { .. } => {
            (model.import_completions(), None)
        }
        CompletionTargetAst::Scoped(scoped) => {
            (model.scoped_completions(scoped.node), Some(scoped))
        }
    };
    let mut completions: Vec<Completion<'_>> = semantic_completions
        .into_iter()
        .filter(|c| typed_query.is_match_symbol_name(c.name.as_str()))
        .map(|c| Completion::from_semantic_completion(db, c))
        .collect();

    if scoped.is_some() {
        add_keyword_value_completions(db, &typed_query, &mut completions);
    }
    if settings.auto_import {
        if let Some(scoped) = scoped {
            add_unimported_completions(
                db,
                file,
                &parsed,
                scoped,
                typed.as_deref(),
                &mut completions,
            );
        }
    }
    completions.sort_by(compare_suggestions);
    completions.dedup_by(|c1, c2| (&c1.name, c1.module_name) == (&c2.name, c2.module_name));
    completions
}

/// Adds a subset of completions derived from keywords.
///
/// Note that at present, these should only be added to "scoped"
/// completions. i.e., This will include `None`, `True`, `False`, etc.
fn add_keyword_value_completions<'db>(
    db: &'db dyn Db,
    query: &QueryPattern,
    completions: &mut Vec<Completion<'db>>,
) {
    let keywords = [
        ("None", Type::none(db)),
        ("True", Type::BooleanLiteral(true)),
        ("False", Type::BooleanLiteral(false)),
    ];
    for (name, ty) in keywords {
        if !query.is_match_symbol_name(name) {
            continue;
        }
        completions.push(Completion {
            name: ast::name::Name::new(name),
            insert: None,
            ty: Some(ty),
            kind: None,
            module_name: None,
            import: None,
            is_type_check_only: false,
            builtin: true,
            documentation: None,
        });
    }
}

/// Adds completions not in scope.
///
/// `scoped` should be information about the identified scope
/// in which the cursor is currently placed.
///
/// The completions returned will auto-insert import statements
/// when selected into `File`.
fn add_unimported_completions<'db>(
    db: &'db dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    scoped: ScopedTarget<'_>,
    typed: Option<&str>,
    completions: &mut Vec<Completion<'db>>,
) {
    let Some(typed) = typed else {
        return;
    };
    let source = source_text(db, file);
    let stylist = Stylist::from_tokens(parsed.tokens(), source.as_str());
    let importer = Importer::new(db, &stylist, file, source.as_str(), parsed);
    let members = importer.members_in_scope_at(scoped.node, scoped.node.start());

    for symbol in all_symbols(db, typed) {
        if symbol.module.file(db) == Some(file) {
            continue;
        }

        let request =
            ImportRequest::import_from(symbol.module.name(db).as_str(), &symbol.symbol.name);
        // FIXME: `all_symbols` doesn't account for wildcard imports.
        // Since we're looking at every module, this is probably
        // "fine," but it might mean that we import a symbol from the
        // "wrong" module.
        let import_action = importer.import(request, &members);
        completions.push(Completion {
            name: ast::name::Name::new(&symbol.symbol.name),
            insert: Some(import_action.symbol_text().into()),
            ty: None,
            kind: symbol.symbol.kind.to_completion_kind(),
            module_name: Some(symbol.module.name(db)),
            import: import_action.import().cloned(),
            builtin: false,
            // TODO: `is_type_check_only` requires inferring the type of the symbol
            is_type_check_only: false,
            documentation: None,
        });
    }
}

/// The kind of tokens identified under the cursor.
#[derive(Debug)]
enum CompletionTargetTokens<'t> {
    /// A `object.attribute` token form was found, where
    /// `attribute` may be empty.
    ///
    /// This requires a name token followed by a dot token.
    ///
    /// This is "possibly" an `object.attribute` because
    /// the object token may not correspond to an object
    /// or it may correspond to *part* of an object.
    /// This is resolved when we try to find an overlapping
    /// AST `ExprAttribute` node later. If we couldn't, then
    /// this is probably not an `object.attribute`.
    PossibleObjectDot {
        /// The token preceding the dot.
        object: &'t Token,
        /// The token, if non-empty, following the dot.
        ///
        /// For right now, this is only used to determine which
        /// module in an `import` statement to return submodule
        /// completions for. But we could use it for other things,
        /// like only returning completions that start with a prefix
        /// corresponding to this token.
        attribute: Option<&'t Token>,
    },
    /// A `from module import attribute` token form was found, where
    /// `attribute` may be empty.
    ImportFrom {
        /// The module being imported from.
        module: &'t Token,
    },
    /// A `import module` token form was found, where `module` may be
    /// empty.
    Import {
        /// The token corresponding to the `import` keyword.
        import: &'t Token,
        /// The token closest to the cursor.
        ///
        /// This is currently unused, but we should use this
        /// eventually to remove completions that aren't a
        /// prefix of what has already been typed. (We are
        /// currently relying on the LSP client to do this.)
        #[expect(dead_code)]
        module: &'t Token,
    },
    /// A token was found under the cursor, but it didn't
    /// match any of our anticipated token patterns.
    Generic { token: &'t Token },
    /// No token was found. We generally treat this like
    /// `Generic` (i.e., offer scope based completions).
    Unknown,
}

impl<'t> CompletionTargetTokens<'t> {
    /// Look for the best matching token pattern at the given offset.
    fn find(parsed: &ParsedModuleRef, offset: TextSize) -> Option<CompletionTargetTokens<'_>> {
        static OBJECT_DOT_EMPTY: [TokenKind; 1] = [TokenKind::Dot];
        static OBJECT_DOT_NON_EMPTY: [TokenKind; 2] = [TokenKind::Dot, TokenKind::Name];

        let offset = match parsed.tokens().at_offset(offset) {
            TokenAt::None => return Some(CompletionTargetTokens::Unknown),
            TokenAt::Single(tok) => tok.end(),
            TokenAt::Between(_, tok) => tok.start(),
        };
        let before = tokens_start_before(parsed.tokens(), offset);
        Some(
            // Our strategy when it comes to `object.attribute` here is
            // to look for the `.` and then take the token immediately
            // preceding it. Later, we look for an `ExprAttribute` AST
            // node that overlaps (even partially) with this token. And
            // that's the object we try to complete attributes for.
            if let Some([_dot]) = token_suffix_by_kinds(before, OBJECT_DOT_EMPTY) {
                let object = before[..before.len() - 1].last()?;
                CompletionTargetTokens::PossibleObjectDot {
                    object,
                    attribute: None,
                }
            } else if let Some([_dot, attribute]) =
                token_suffix_by_kinds(before, OBJECT_DOT_NON_EMPTY)
            {
                let object = before[..before.len() - 2].last()?;
                CompletionTargetTokens::PossibleObjectDot {
                    object,
                    attribute: Some(attribute),
                }
            } else if let Some(module) = import_from_tokens(before) {
                CompletionTargetTokens::ImportFrom { module }
            } else if let Some((import, module)) = import_tokens(before) {
                CompletionTargetTokens::Import { import, module }
            } else if let Some([_]) = token_suffix_by_kinds(before, [TokenKind::Float]) {
                // If we're writing a `float`, then we should
                // specifically not offer completions. This wouldn't
                // normally be an issue, but if completions are
                // automatically triggered by a `.` (which is what we
                // request as an LSP server), then we can get here
                // in the course of just writing a decimal number.
                return None;
            } else if let Some([_]) = token_suffix_by_kinds(before, [TokenKind::Ellipsis]) {
                // Similarly as above. If we've just typed an ellipsis,
                // then we shouldn't show completions. Note that
                // this doesn't prevent `....<CURSOR>` from showing
                // completions (which would be the attributes available
                // on an `ellipsis` object).
                return None;
            } else {
                let Some(last) = before.last() else {
                    return Some(CompletionTargetTokens::Unknown);
                };
                CompletionTargetTokens::Generic { token: last }
            },
        )
    }

    /// Returns a corresponding AST node for these tokens.
    ///
    /// `offset` should be the offset of the cursor.
    ///
    /// If no plausible AST node could be found, then `None` is returned.
    fn ast(
        &self,
        parsed: &'t ParsedModuleRef,
        offset: TextSize,
    ) -> Option<CompletionTargetAst<'t>> {
        match *self {
            CompletionTargetTokens::PossibleObjectDot { object, attribute } => {
                let covering_node = covering_node(parsed.syntax().into(), object.range())
                    .find_last(|node| {
                        // We require that the end of the node range not
                        // exceed the cursor offset. This avoids selecting
                        // a node "too high" in the AST in cases where
                        // completions are requested in the middle of an
                        // expression. e.g., `foo.<CURSOR>.bar`.
                        if node.is_expr_attribute() {
                            return node.range().end() <= offset;
                        }
                        // For import statements though, they can't be
                        // nested, so we don't care as much about the
                        // cursor being strictly after the statement.
                        // And indeed, sometimes it won't be! e.g.,
                        //
                        //   import re, os.p<CURSOR>, zlib
                        //
                        // So just return once we find an import.
                        node.is_stmt_import() || node.is_stmt_import_from()
                    })
                    .ok()?;
                match covering_node.node() {
                    ast::AnyNodeRef::ExprAttribute(expr) => {
                        Some(CompletionTargetAst::ObjectDot { expr })
                    }
                    ast::AnyNodeRef::StmtImport(import) => {
                        let range = attribute
                            .map(Ranged::range)
                            .unwrap_or_else(|| object.range());
                        // Find the name that overlaps with the
                        // token we identified for the attribute.
                        let name = import
                            .names
                            .iter()
                            .position(|alias| alias.range().contains_range(range))?;
                        Some(CompletionTargetAst::ObjectDotInImport { import, name })
                    }
                    ast::AnyNodeRef::StmtImportFrom(import) => {
                        Some(CompletionTargetAst::ObjectDotInImportFrom { import })
                    }
                    _ => None,
                }
            }
            CompletionTargetTokens::ImportFrom { module, .. } => {
                let covering_node = covering_node(parsed.syntax().into(), module.range())
                    .find_first(|node| node.is_stmt_import_from())
                    .ok()?;
                let ast::AnyNodeRef::StmtImportFrom(import) = covering_node.node() else {
                    return None;
                };
                Some(CompletionTargetAst::ImportFrom { import, name: None })
            }
            CompletionTargetTokens::Import { import, .. } => {
                let covering_node = covering_node(parsed.syntax().into(), import.range())
                    .find_first(|node| node.is_stmt_import() || node.is_stmt_import_from())
                    .ok()?;
                match covering_node.node() {
                    ast::AnyNodeRef::StmtImport(import) => {
                        Some(CompletionTargetAst::Import { import, name: None })
                    }
                    ast::AnyNodeRef::StmtImportFrom(import) => {
                        Some(CompletionTargetAst::ImportViaFrom { import })
                    }
                    _ => None,
                }
            }
            CompletionTargetTokens::Generic { token } => {
                let node = covering_node(parsed.syntax().into(), token.range()).node();
                Some(CompletionTargetAst::Scoped(ScopedTarget { node }))
            }
            CompletionTargetTokens::Unknown => {
                let range = TextRange::empty(offset);
                let covering_node = covering_node(parsed.syntax().into(), range);
                Some(CompletionTargetAst::Scoped(ScopedTarget {
                    node: covering_node.node(),
                }))
            }
        }
    }
}

/// The AST node patterns that we support identifying under the cursor.
#[derive(Debug)]
enum CompletionTargetAst<'t> {
    /// A `object.attribute` scenario, where we want to
    /// list attributes on `object` for completions.
    ObjectDot { expr: &'t ast::ExprAttribute },
    /// A `import module.submodule` scenario, where we only want to
    /// list submodules for completions.
    ObjectDotInImport {
        /// The import statement.
        import: &'t ast::StmtImport,
        /// An index into `import.names`. The index is guaranteed to be
        /// valid.
        name: usize,
    },
    /// A `from module.submodule` scenario, where we only want to list
    /// submodules for completions.
    ObjectDotInImportFrom { import: &'t ast::StmtImportFrom },
    /// A `from module import attribute` scenario, where we want to
    /// list attributes on `module` for completions.
    ImportFrom {
        /// The import statement.
        import: &'t ast::StmtImportFrom,
        /// An index into `import.names` if relevant. When this is
        /// set, the index is guaranteed to be valid.
        name: Option<usize>,
    },
    /// A `import module` scenario, where we want to
    /// list available modules for completions.
    Import {
        /// The import statement.
        #[expect(dead_code)]
        import: &'t ast::StmtImport,
        /// An index into `import.names` if relevant. When this is
        /// set, the index is guaranteed to be valid.
        #[expect(dead_code)]
        name: Option<usize>,
    },
    /// A `from module` scenario, where we want to
    /// list available modules for completions.
    ImportViaFrom {
        /// The import statement.
        #[expect(dead_code)]
        import: &'t ast::StmtImportFrom,
    },
    /// A scoped scenario, where we want to list all items available in
    /// the most narrow scope containing the giving AST node.
    Scoped(ScopedTarget<'t>),
}

#[derive(Clone, Copy, Debug)]
struct ScopedTarget<'t> {
    /// The node with the smallest range that fully covers
    /// the token under the cursor.
    node: ast::AnyNodeRef<'t>,
}

/// Returns a slice of tokens that all start before the given
/// [`TextSize`] offset.
///
/// If the given offset is between two tokens, the returned slice will end just
/// before the following token. In other words, if the offset is between the
/// end of previous token and start of next token, the returned slice will end
/// just before the next token.
///
/// Unlike `Tokens::before`, this never panics. If `offset` is within a token's
/// range (including if it's at the very beginning), then that token will be
/// included in the slice returned.
fn tokens_start_before(tokens: &Tokens, offset: TextSize) -> &[Token] {
    let partition_point = tokens.partition_point(|token| token.start() < offset);

    &tokens[..partition_point]
}

/// Returns a suffix of `tokens` corresponding to the `kinds` given.
///
/// If a suffix of `tokens` with the given `kinds` could not be found,
/// then `None` is returned.
///
/// This is useful for matching specific patterns of token sequences
/// in order to identify what kind of completions we should offer.
fn token_suffix_by_kinds<const N: usize>(
    tokens: &[Token],
    kinds: [TokenKind; N],
) -> Option<[&Token; N]> {
    if kinds.len() > tokens.len() {
        return None;
    }
    for (token, expected_kind) in tokens.iter().rev().zip(kinds.iter().rev()) {
        if &token.kind() != expected_kind {
            return None;
        }
    }
    Some(std::array::from_fn(|i| {
        &tokens[tokens.len() - (kinds.len() - i)]
    }))
}

/// Looks for the start of a `from module import <CURSOR>` statement.
///
/// If found, one arbitrary token forming `module` is returned.
fn import_from_tokens(tokens: &[Token]) -> Option<&Token> {
    use TokenKind as TK;

    /// The number of tokens we're willing to consume backwards from
    /// the cursor's position until we give up looking for a `from
    /// module import <CURSOR>` pattern. The state machine below has
    /// lots of opportunities to bail way earlier than this, but if
    /// there's, e.g., a long list of name tokens for something that
    /// isn't an import, then we could end up doing a lot of wasted
    /// work here. Probably humans aren't often working with single
    /// import statements over 1,000 tokens long.
    ///
    /// The other thing to consider here is that, by the time we get to
    /// this point, ty has already done some work proportional to the
    /// length of `tokens` anyway. The unit of work we do below is very
    /// small.
    const LIMIT: usize = 1_000;

    /// A state used to "parse" the tokens preceding the user's cursor,
    /// in reverse, to detect a "from import" statement.
    enum S {
        Start,
        Names,
        Module,
    }

    let mut state = S::Start;
    let mut module_token: Option<&Token> = None;
    // Move backward through the tokens until we get to
    // the `from` token.
    for token in tokens.iter().rev().take(LIMIT) {
        state = match (state, token.kind()) {
            // It's okay to pop off a newline token here initially,
            // since it may occur when the name being imported is
            // empty.
            (S::Start, TK::Newline) => S::Names,
            // Munch through tokens that can make up an alias.
            // N.B. We could also consider taking any token here
            // *except* some limited set of tokens (like `Newline`).
            // That might work well if it turns out that listing
            // all possible allowable tokens is too brittle.
            (
                S::Start | S::Names,
                TK::Name
                | TK::Comma
                | TK::As
                | TK::Case
                | TK::Match
                | TK::Type
                | TK::Star
                | TK::Lpar
                | TK::Rpar
                | TK::NonLogicalNewline
                // It's not totally clear the conditions under
                // which this occurs (I haven't read our tokenizer),
                // but it appears in code like this, where this is
                // the entire file contents:
                //
                //     from sys import (
                //         abiflags,
                //         <CURSOR>
                //
                // It seems harmless to just allow this "unknown"
                // token here to make the above work.
                | TK::Unknown,
            ) => S::Names,
            (S::Start | S::Names, TK::Import) => S::Module,
            // Munch through tokens that can make up a module.
            (
                S::Module,
                TK::Name | TK::Dot | TK::Ellipsis | TK::Case | TK::Match | TK::Type | TK::Unknown,
            ) => {
                // It's okay if there are multiple module
                // tokens here. Just taking the last one
                // (which is the one appearing first in
                // the source code) is fine. We only need
                // this to find the corresponding AST node,
                // so any of the tokens should work fine.
                module_token = Some(token);
                S::Module
            }
            (S::Module, TK::From) => return module_token,
            _ => return None,
        };
    }
    None
}

/// Looks for the start of a `import <CURSOR>` statement.
///
/// This also handles cases like `import foo, c<CURSOR>, bar`.
///
/// If found, a token corresponding to the `import` or `from` keyword
/// and the closest point of the `<CURSOR>` is returned.
///
/// It is assumed that callers will call `from_import_tokens` first to
/// try and recognize a `from ... import ...` statement before using
/// this.
fn import_tokens(tokens: &[Token]) -> Option<(&Token, &Token)> {
    use TokenKind as TK;

    /// A look-back limit, in order to bound work.
    ///
    /// See `LIMIT` in `import_from_tokens` for more context.
    const LIMIT: usize = 1_000;

    /// A state used to "parse" the tokens preceding the user's cursor,
    /// in reverse, to detect a `import` statement.
    enum S {
        Start,
        Names,
    }

    let mut state = S::Start;
    let module_token = tokens.last()?;
    // Move backward through the tokens until we get to
    // the `import` token.
    for token in tokens.iter().rev().take(LIMIT) {
        state = match (state, token.kind()) {
            // It's okay to pop off a newline token here initially,
            // since it may occur when the name being imported is
            // empty.
            (S::Start, TK::Newline) => S::Names,
            // Munch through tokens that can make up an alias.
            (S::Start | S::Names, TK::Name | TK::Comma | TK::As | TK::Unknown) => S::Names,
            (S::Start | S::Names, TK::Import | TK::From) => {
                return Some((token, module_token));
            }
            _ => return None,
        };
    }
    None
}

/// Looks for the text typed immediately before the cursor offset
/// given.
///
/// If there isn't any typed text or it could not otherwise be found,
/// then `None` is returned.
fn find_typed_text(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    offset: TextSize,
) -> Option<String> {
    let source = source_text(db, file);
    let tokens = tokens_start_before(parsed.tokens(), offset);
    let last = tokens.last()?;
    if !matches!(last.kind(), TokenKind::Name) {
        return None;
    }
    // This one's weird, but if the cursor is beyond
    // what is in the closest `Name` token, then it's
    // likely we can't infer anything about what has
    // been typed. This likely means there is whitespace
    // or something that isn't represented in the token
    // stream. So just give up.
    if last.end() < offset {
        return None;
    }
    Some(source[last.range()].to_string())
}

/// Whether the last token is within a comment or not.
fn is_in_comment(tokens: &[Token]) -> bool {
    tokens.last().is_some_and(|t| t.kind().is_comment())
}

/// Whether the last token is positioned within a string token (regular, f-string, t-string, etc).
///
/// Note that this will return `false` when the last token is positioned within an
/// interpolation block in an f-string or a t-string.
fn is_in_string(tokens: &[Token]) -> bool {
    tokens.last().is_some_and(|t| {
        matches!(
            t.kind(),
            TokenKind::String | TokenKind::FStringMiddle | TokenKind::TStringMiddle
        )
    })
}

/// Returns true when the tokens indicate that the definition of a new name is being introduced at the end.
fn is_in_definition_place(db: &dyn Db, tokens: &[Token], file: File) -> bool {
    let is_definition_keyword = |token: &Token| {
        if matches!(
            token.kind(),
            TokenKind::Def | TokenKind::Class | TokenKind::Type
        ) {
            true
        } else if token.kind() == TokenKind::Name {
            let source = source_text(db, file);
            &source[token.range()] == "type"
        } else {
            false
        }
    };

    tokens
        .len()
        .checked_sub(2)
        .and_then(|i| tokens.get(i))
        .is_some_and(is_definition_keyword)
}

/// Order completions according to the following rules:
///
/// 1) Names with no underscore prefix
/// 2) Names starting with `_` but not dunders
/// 3) `__dunder__` names
///
/// Among each category, type-check-only items are sorted last,
/// and otherwise completions are sorted lexicographically.
///
/// This has the effect of putting all dunder attributes after "normal"
/// attributes, and all single-underscore attributes after dunder attributes.
fn compare_suggestions(c1: &Completion, c2: &Completion) -> Ordering {
    fn key<'a>(completion: &'a Completion) -> (bool, bool, NameKind, bool, &'a Name) {
        (
            completion.module_name.is_some(),
            completion.builtin,
            NameKind::classify(&completion.name),
            completion.is_type_check_only,
            &completion.name,
        )
    }

    key(c1).cmp(&key(c2))
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_python_parser::{Mode, ParseOptions, TokenKind, Tokens};
    use ty_python_semantic::ModuleName;

    use crate::completion::{Completion, completion};
    use crate::tests::{CursorTest, CursorTestBuilder};

    use super::{CompletionKind, CompletionSettings, token_suffix_by_kinds};

    #[test]
    fn token_suffixes_match() {
        insta::assert_debug_snapshot!(
            token_suffix_by_kinds(&tokenize("foo.x"), [TokenKind::Newline]),
            @r"
        Some(
            [
                Newline 5..5,
            ],
        )
        ",
        );

        insta::assert_debug_snapshot!(
            token_suffix_by_kinds(&tokenize("foo.x"), [TokenKind::Name, TokenKind::Newline]),
            @r"
        Some(
            [
                Name 4..5,
                Newline 5..5,
            ],
        )
        ",
        );

        let all = [
            TokenKind::Name,
            TokenKind::Dot,
            TokenKind::Name,
            TokenKind::Newline,
        ];
        insta::assert_debug_snapshot!(
            token_suffix_by_kinds(&tokenize("foo.x"), all),
            @r"
        Some(
            [
                Name 0..3,
                Dot 3..4,
                Name 4..5,
                Newline 5..5,
            ],
        )
        ",
        );
    }

    #[test]
    fn token_suffixes_nomatch() {
        insta::assert_debug_snapshot!(
            token_suffix_by_kinds(&tokenize("foo.x"), [TokenKind::Name]),
            @"None",
        );

        let too_many = [
            TokenKind::Dot,
            TokenKind::Name,
            TokenKind::Dot,
            TokenKind::Name,
            TokenKind::Newline,
        ];
        insta::assert_debug_snapshot!(
            token_suffix_by_kinds(&tokenize("foo.x"), too_many),
            @"None",
        );
    }

    #[test]
    fn empty() {
        let test = completion_test_builder(
            "\
<CURSOR>
",
        );

        assert_snapshot!(
            test.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn builtins() {
        let builder = completion_test_builder(
            "\
<CURSOR>
",
        );
        let test = builder.build();

        test.contains("filter");
        // Sunder items should be filtered out
        test.not_contains("_T");
        // Dunder attributes should not be stripped
        test.contains("__annotations__");
        // See `private_symbols_in_stub` for more comprehensive testing private of symbol filtering.
    }

    #[test]
    fn builtins_not_included_object_attr() {
        let builder = completion_test_builder(
            "\
import re

re.<CURSOR>
",
        );
        builder.build().not_contains("filter");
    }

    #[test]
    fn builtins_not_included_import() {
        let builder = completion_test_builder(
            "\
from re import <CURSOR>
",
        );
        builder.build().not_contains("filter");
    }

    #[test]
    fn imports1() {
        let builder = completion_test_builder(
            "\
import re

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"re");
    }

    #[test]
    fn imports2() {
        let builder = completion_test_builder(
            "\
from os import path

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"path");
    }

    // N.B. We don't currently explore module APIs. This
    // is still just emitting symbols from the detected scope.
    #[test]
    fn module_api() {
        let builder = completion_test_builder(
            "\
import re

re.<CURSOR>
",
        );
        builder.build().contains("findall");
    }

    #[test]
    fn private_symbols_in_stub() {
        let builder = CursorTest::builder()
            .source(
                "package/__init__.pyi",
                r#"\
from typing import TypeAlias, Literal, TypeVar, ParamSpec, TypeVarTuple, Protocol

public_name = 1
_private_name = 1
__mangled_name = 1
__dunder_name__ = 1

public_type_var = TypeVar("public_type_var")
_private_type_var = TypeVar("_private_type_var")
__mangled_type_var = TypeVar("__mangled_type_var")

public_param_spec = ParamSpec("public_param_spec")
_private_param_spec = ParamSpec("_private_param_spec")

public_type_var_tuple = TypeVarTuple("public_type_var_tuple")
_private_type_var_tuple = TypeVarTuple("_private_type_var_tuple")

public_explicit_type_alias: TypeAlias = Literal[1]
_private_explicit_type_alias: TypeAlias = Literal[1]

public_implicit_union_alias = int | str
_private_implicit_union_alias = int | str

class PublicProtocol(Protocol):
    def method(self) -> None: ...

class _PrivateProtocol(Protocol):
    def method(self) -> None: ...
"#,
            )
            .source("main.py", "import package; package.<CURSOR>")
            .completion_test_builder();

        let test = builder.build();
        test.contains("public_name");
        test.contains("_private_name");
        test.contains("__mangled_name");
        test.contains("__dunder_name__");
        test.contains("public_type_var");
        test.not_contains("_private_type_var");
        test.not_contains("__mangled_type_var");
        test.contains("public_param_spec");
        test.not_contains("_private_param_spec");
        test.contains("public_type_var_tuple");
        test.not_contains("_private_type_var_tuple");
        test.contains("public_explicit_type_alias");
        test.not_contains("_private_explicit_type_alias");
        test.contains("public_implicit_union_alias");
        test.not_contains("_private_implicit_union_alias");
        test.contains("PublicProtocol");
        test.not_contains("_PrivateProtocol");
    }

    /// Unlike [`private_symbols_in_stub`], this test doesn't use a `.pyi` file so all of the names
    /// are visible.
    #[test]
    fn private_symbols_in_module() {
        let builder = CursorTest::builder()
            .source(
                "package/__init__.py",
                r#"\
from typing import TypeAlias, Literal, TypeVar, ParamSpec, TypeVarTuple, Protocol

public_name = 1
_private_name = 1
__mangled_name = 1
__dunder_name__ = 1

public_type_var = TypeVar("public_type_var")
_private_type_var = TypeVar("_private_type_var")
__mangled_type_var = TypeVar("__mangled_type_var")

public_param_spec = ParamSpec("public_param_spec")
_private_param_spec = ParamSpec("_private_param_spec")

public_type_var_tuple = TypeVarTuple("public_type_var_tuple")
_private_type_var_tuple = TypeVarTuple("_private_type_var_tuple")

public_explicit_type_alias: TypeAlias = Literal[1]
_private_explicit_type_alias: TypeAlias = Literal[1]

class PublicProtocol(Protocol):
    def method(self) -> None: ...

class _PrivateProtocol(Protocol):
    def method(self) -> None: ...
"#,
            )
            .source("main.py", "import package; package.<CURSOR>")
            .completion_test_builder();

        let test = builder.build();
        test.contains("public_name");
        test.contains("_private_name");
        test.contains("__mangled_name");
        test.contains("__dunder_name__");
        test.contains("public_type_var");
        test.contains("_private_type_var");
        test.contains("__mangled_type_var");
        test.contains("public_param_spec");
        test.contains("_private_param_spec");
        test.contains("public_type_var_tuple");
        test.contains("_private_type_var_tuple");
        test.contains("public_explicit_type_alias");
        test.contains("_private_explicit_type_alias");
        test.contains("PublicProtocol");
        test.contains("_PrivateProtocol");
    }

    #[test]
    fn one_function_prefix() {
        let builder = completion_test_builder(
            "\
def foo(): ...

f<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn one_function_not_prefix() {
        let builder = completion_test_builder(
            "\
def foo(): ...

g<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn one_function_blank() {
        let builder = completion_test_builder(
            "\
def foo(): ...

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        ");
    }

    #[test]
    fn nested_function_prefix() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo(): ...

f<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn nested_function_blank() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo(): ...

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        ");
    }

    #[test]
    fn nested_function_not_in_global_scope_prefix() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo(): ...
    f<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        foofoo
        ");
    }

    #[test]
    fn nested_function_not_in_global_scope_blank() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo(): ...
    <CURSOR>
",
        );

        // FIXME: Should include `foofoo`.
        //
        // `foofoo` isn't included at present (2025-05-22). The problem
        // here is that the AST for `def foo():` doesn't encompass the
        // trailing indentation. So when the cursor position is in that
        // trailing indentation, we can't (easily) get a handle to the
        // right scope. And even if we could, the AST expressions for
        // `def foo():` and `def foofoo(): ...` end at precisely the
        // same point. So there is no AST we can hold after the end of
        // `foofoo` but before the end of `foo`. So at the moment, it's
        // not totally clear how to get the right scope.
        //
        // If we didn't want to change the ranges on the AST nodes,
        // another approach here would be to get the inner most scope,
        // and explore its ancestors until we get to a level that
        // matches the current cursor's indentation. This seems fraught
        // however. It's not clear to me that we can always assume a
        // correspondence between scopes and indentation level.
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix1() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        foofoo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix2() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        foofoo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix3() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        foofoo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix4() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix5() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
        f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        foofoo
        foofoofoo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank1() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        //
        // The tests below fail for the same reason that
        // `nested_function_not_in_global_scope_blank` fails: there is no
        // space in the AST ranges after the end of `foofoofoo` but before
        // the end of `foofoo`. So either the AST needs to be tweaked to
        // account for the indented whitespace, or some other technique
        // needs to be used to get the scope containing `foofoo` but not
        // `foofoofoo`.
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank2() {
        let builder = completion_test_builder(
            " \
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank3() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>
def frob(): ...
            ",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank4() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>

def frob(): ...
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank5() {
        let builder = completion_test_builder(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...

    <CURSOR>

def frob(): ...
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        frob
        ");
    }

    /// Regression test for <https://github.com/astral-sh/ty/issues/1392>
    ///
    /// This test ensures completions work when the cursor is at the
    /// start of a zero-length token.
    #[test]
    fn completion_at_eof() {
        completion_test_builder("def f(msg: str):\n    msg.<CURSOR>")
            .build()
            .contains("upper")
            .contains("capitalize");

        completion_test_builder("def f(msg: str):\n    msg.u<CURSOR>")
            .build()
            .contains("upper")
            .not_contains("capitalize");
    }

    #[test]
    fn list_comprehension1() {
        let builder = completion_test_builder(
            "\
[<CURSOR> for bar in [1, 2, 3]]
",
        );

        // TODO: it would be good if `bar` was included here, but
        // the list comprehension is not yet valid and so we do not
        // detect this as a definition of `bar`.
        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn list_comprehension2() {
        let builder = completion_test_builder(
            "\
[f<CURSOR> for foo in [1, 2, 3]]
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix1() {
        let builder = completion_test_builder(
            "\
(lambda foo: (1 + f<CURSOR> + 2))(2)
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix2() {
        let builder = completion_test_builder(
            "\
(lambda foo: f<CURSOR> + 1)(2)
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix3() {
        let builder = completion_test_builder(
            "\
(lambda foo: (f<CURSOR> + 1))(2)
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix4() {
        let builder = completion_test_builder(
            "\
(lambda foo: 1 + f<CURSOR>)(2)
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_blank1() {
        let builder = completion_test_builder(
            "\
(lambda foo: 1 + <CURSOR> + 2)(2)
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_blank2() {
        let builder = completion_test_builder(
            "\
(lambda foo: <CURSOR> + 1)(2)
",
        );

        // FIXME: Should include `foo`.
        //
        // These fails for similar reasons as above: the body of the
        // lambda doesn't include the position of <CURSOR> because
        // <CURSOR> is inside leading or trailing whitespace. (Even
        // when enclosed in parentheses. Specifically, parentheses
        // aren't part of the node's range unless it's relevant e.g.,
        // tuples.)
        //
        // The `lambda_blank1` test works because there are expressions
        // on either side of <CURSOR>.
        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn lambda_blank3() {
        let builder = completion_test_builder(
            "\
(lambda foo: (<CURSOR> + 1))(2)
",
        );

        // FIXME: Should include `foo`.
        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn lambda_blank4() {
        let builder = completion_test_builder(
            "\
(lambda foo: 1 + <CURSOR>)(2)
",
        );

        // FIXME: Should include `foo`.
        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn class_prefix1() {
        let builder = completion_test_builder(
            "\
class Foo:
    bar = 1
    quux = b<CURSOR>
    frob = 3
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        bar
        frob
        ");
    }

    #[test]
    fn class_prefix2() {
        let builder = completion_test_builder(
            "\
class Foo:
    bar = 1
    quux = b<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"bar");
    }

    #[test]
    fn class_blank1() {
        let builder = completion_test_builder(
            "\
class Foo:
    bar = 1
    quux = <CURSOR>
    frob = 3
",
        );

        // FIXME: Should include `bar`, `quux` and `frob`.
        // (Unclear if `Foo` should be included, but a false
        // positive isn't the end of the world.)
        //
        // These don't work for similar reasons as other
        // tests above with the <CURSOR> inside of whitespace.
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Foo
        ");
    }

    #[test]
    fn class_blank2() {
        let builder = completion_test_builder(
            "\
class Foo:
    bar = 1
    quux = <CURSOR>
    frob = 3
",
        );

        // FIXME: Should include `bar`, `quux` and `frob`.
        // (Unclear if `Foo` should be included, but a false
        // positive isn't the end of the world.)
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Foo
        ");
    }

    #[test]
    fn class_super1() {
        let builder = completion_test_builder(
            "\
class Bar: ...

class Foo(<CURSOR>):
    bar = 1
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super2() {
        let builder = completion_test_builder(
            "\
class Foo(<CURSOR>):
    bar = 1

class Bar: ...
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super3() {
        let builder = completion_test_builder(
            "\
class Foo(<CURSOR>
    bar = 1

class Bar: ...
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super4() {
        let builder = completion_test_builder(
            "\
class Bar: ...

class Foo(<CURSOR>",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_init1() {
        let builder = completion_test_builder(
            "\
class Quux:
    def __init__(self):
        self.foo = 1
        self.bar = 2
        self.baz = 3

quux = Quux()
quux.<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
        bar :: Unknown | Literal[2]
        baz :: Unknown | Literal[3]
        foo :: Unknown | Literal[1]
        __annotations__ :: dict[str, Any]
        __class__ :: type[Quux]
        __delattr__ :: bound method Quux.__delattr__(name: str, /) -> None
        __dict__ :: dict[str, Any]
        __dir__ :: bound method Quux.__dir__() -> Iterable[str]
        __doc__ :: str | None
        __eq__ :: bound method Quux.__eq__(value: object, /) -> bool
        __format__ :: bound method Quux.__format__(format_spec: str, /) -> str
        __getattribute__ :: bound method Quux.__getattribute__(name: str, /) -> Any
        __getstate__ :: bound method Quux.__getstate__() -> object
        __hash__ :: bound method Quux.__hash__() -> int
        __init__ :: bound method Quux.__init__() -> Unknown
        __init_subclass__ :: bound method type[Quux].__init_subclass__() -> None
        __module__ :: str
        __ne__ :: bound method Quux.__ne__(value: object, /) -> bool
        __new__ :: def __new__(cls) -> Self@__new__
        __reduce__ :: bound method Quux.__reduce__() -> str | tuple[Any, ...]
        __reduce_ex__ :: bound method Quux.__reduce_ex__(protocol: SupportsIndex, /) -> str | tuple[Any, ...]
        __repr__ :: bound method Quux.__repr__() -> str
        __setattr__ :: bound method Quux.__setattr__(name: str, value: Any, /) -> None
        __sizeof__ :: bound method Quux.__sizeof__() -> int
        __str__ :: bound method Quux.__str__() -> str
        __subclasshook__ :: bound method type[Quux].__subclasshook__(subclass: type, /) -> bool
        ");
    }

    #[test]
    fn class_init2() {
        let builder = completion_test_builder(
            "\
class Quux:
    def __init__(self):
        self.foo = 1
        self.bar = 2
        self.baz = 3

quux = Quux()
quux.b<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
        bar :: Unknown | Literal[2]
        baz :: Unknown | Literal[3]
        __getattribute__ :: bound method Quux.__getattribute__(name: str, /) -> Any
        __init_subclass__ :: bound method type[Quux].__init_subclass__() -> None
        __subclasshook__ :: bound method type[Quux].__subclasshook__(subclass: type, /) -> bool
        ");
    }

    #[test]
    fn metaclass1() {
        let builder = completion_test_builder(
            "\
class Meta(type):
    @property
    def meta_attr(self) -> int:
        return 0

class C(metaclass=Meta): ...

C.<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
        meta_attr :: int
        mro :: bound method <class 'C'>.mro() -> list[type]
        __annotate__ :: @Todo | None
        __annotations__ :: dict[str, Any]
        __base__ :: type | None
        __bases__ :: tuple[type, ...]
        __basicsize__ :: int
        __call__ :: bound method <class 'C'>.__call__(...) -> Any
        __class__ :: <class 'Meta'>
        __delattr__ :: def __delattr__(self, name: str, /) -> None
        __dict__ :: MappingProxyType[str, Any]
        __dictoffset__ :: int
        __dir__ :: def __dir__(self) -> Iterable[str]
        __doc__ :: str | None
        __eq__ :: def __eq__(self, value: object, /) -> bool
        __flags__ :: int
        __format__ :: def __format__(self, format_spec: str, /) -> str
        __getattribute__ :: def __getattribute__(self, name: str, /) -> Any
        __getstate__ :: def __getstate__(self) -> object
        __hash__ :: def __hash__(self) -> int
        __init__ :: def __init__(self) -> None
        __init_subclass__ :: bound method <class 'C'>.__init_subclass__() -> None
        __instancecheck__ :: bound method <class 'C'>.__instancecheck__(instance: Any, /) -> bool
        __itemsize__ :: int
        __module__ :: str
        __mro__ :: tuple[type, ...]
        __name__ :: str
        __ne__ :: def __ne__(self, value: object, /) -> bool
        __new__ :: def __new__(cls) -> Self@__new__
        __or__ :: bound method <class 'C'>.__or__[Self](value: Any, /) -> UnionType | Self@__or__
        __prepare__ :: bound method <class 'Meta'>.__prepare__(name: str, bases: tuple[type, ...], /, **kwds: Any) -> MutableMapping[str, object]
        __qualname__ :: str
        __reduce__ :: def __reduce__(self) -> str | tuple[Any, ...]
        __reduce_ex__ :: def __reduce_ex__(self, protocol: SupportsIndex, /) -> str | tuple[Any, ...]
        __repr__ :: def __repr__(self) -> str
        __ror__ :: bound method <class 'C'>.__ror__[Self](value: Any, /) -> UnionType | Self@__ror__
        __setattr__ :: def __setattr__(self, name: str, value: Any, /) -> None
        __sizeof__ :: def __sizeof__(self) -> int
        __str__ :: def __str__(self) -> str
        __subclasscheck__ :: bound method <class 'C'>.__subclasscheck__(subclass: type, /) -> bool
        __subclasses__ :: bound method <class 'C'>.__subclasses__[Self]() -> list[Self@__subclasses__]
        __subclasshook__ :: bound method <class 'C'>.__subclasshook__(subclass: type, /) -> bool
        __text_signature__ :: str | None
        __type_params__ :: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
        __weakrefoffset__ :: int
        ");
    }

    #[test]
    fn metaclass2() {
        let builder = completion_test_builder(
            "\
class Meta(type):
    @property
    def meta_attr(self) -> int:
        return 0

class C(metaclass=Meta): ...

Meta.<CURSOR>
",
        );

        insta::with_settings!({
            // The formatting of some types are different depending on
            // whether we're in release mode or not. These differences
            // aren't really relevant for completion tests AFAIK, so
            // just redact them. ---AG
            filters => [(r"(?m)\s*__(annotations|new|annotate)__.+$", "")]},
            {
                assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
                meta_attr :: property
                mro :: def mro(self) -> list[type]
                __base__ :: type | None
                __bases__ :: tuple[type, ...]
                __basicsize__ :: int
                __call__ :: def __call__(self, *args: Any, **kwds: Any) -> Any
                __class__ :: <class 'type'>
                __delattr__ :: def __delattr__(self, name: str, /) -> None
                __dict__ :: MappingProxyType[str, Any]
                __dictoffset__ :: int
                __dir__ :: def __dir__(self) -> Iterable[str]
                __doc__ :: str | None
                __eq__ :: def __eq__(self, value: object, /) -> bool
                __flags__ :: int
                __format__ :: def __format__(self, format_spec: str, /) -> str
                __getattribute__ :: def __getattribute__(self, name: str, /) -> Any
                __getstate__ :: def __getstate__(self) -> object
                __hash__ :: def __hash__(self) -> int
                __init__ :: Overload[(self, o: object, /) -> None, (self, name: str, bases: tuple[type, ...], dict: dict[str, Any], /, **kwds: Any) -> None]
                __init_subclass__ :: bound method <class 'Meta'>.__init_subclass__() -> None
                __instancecheck__ :: def __instancecheck__(self, instance: Any, /) -> bool
                __itemsize__ :: int
                __module__ :: str
                __mro__ :: tuple[type, ...]
                __name__ :: str
                __ne__ :: def __ne__(self, value: object, /) -> bool
                __or__ :: def __or__[Self](self: Self@__or__, value: Any, /) -> UnionType | Self@__or__
                __prepare__ :: bound method <class 'Meta'>.__prepare__(name: str, bases: tuple[type, ...], /, **kwds: Any) -> MutableMapping[str, object]
                __qualname__ :: str
                __reduce__ :: def __reduce__(self) -> str | tuple[Any, ...]
                __reduce_ex__ :: def __reduce_ex__(self, protocol: SupportsIndex, /) -> str | tuple[Any, ...]
                __repr__ :: def __repr__(self) -> str
                __ror__ :: def __ror__[Self](self: Self@__ror__, value: Any, /) -> UnionType | Self@__ror__
                __setattr__ :: def __setattr__(self, name: str, value: Any, /) -> None
                __sizeof__ :: def __sizeof__(self) -> int
                __str__ :: def __str__(self) -> str
                __subclasscheck__ :: def __subclasscheck__(self, subclass: type, /) -> bool
                __subclasses__ :: def __subclasses__[Self](self: Self@__subclasses__) -> list[Self@__subclasses__]
                __subclasshook__ :: bound method <class 'Meta'>.__subclasshook__(subclass: type, /) -> bool
                __text_signature__ :: str | None
                __type_params__ :: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
                __weakrefoffset__ :: int
                ");
            }
        );
    }

    #[test]
    fn class_init3() {
        let builder = completion_test_builder(
            "\
class Quux:
    def __init__(self):
        self.foo = 1
        self.bar = 2
        self.<CURSOR>
        self.baz = 3
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        bar
        baz
        foo
        __annotations__
        __class__
        __delattr__
        __dict__
        __dir__
        __doc__
        __eq__
        __format__
        __getattribute__
        __getstate__
        __hash__
        __init__
        __init_subclass__
        __module__
        __ne__
        __new__
        __reduce__
        __reduce_ex__
        __repr__
        __setattr__
        __sizeof__
        __str__
        __subclasshook__
        ");
    }

    #[test]
    fn class_attributes1() {
        let builder = completion_test_builder(
            "\
class Quux:
    some_attribute: int = 1

    def __init__(self):
        self.foo = 1
        self.bar = 2
        self.baz = 3

    def some_method(self) -> int:
        return 1

    @property
    def some_property(self) -> int:
        return 1

    @classmethod
    def some_class_method(self) -> int:
        return 1

    @staticmethod
    def some_static_method(self) -> int:
        return 1

Quux.<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
        mro :: bound method <class 'Quux'>.mro() -> list[type]
        some_attribute :: int
        some_class_method :: bound method <class 'Quux'>.some_class_method() -> int
        some_method :: def some_method(self) -> int
        some_property :: property
        some_static_method :: def some_static_method(self) -> int
        __annotate__ :: @Todo | None
        __annotations__ :: dict[str, Any]
        __base__ :: type | None
        __bases__ :: tuple[type, ...]
        __basicsize__ :: int
        __call__ :: bound method <class 'Quux'>.__call__(...) -> Any
        __class__ :: <class 'type'>
        __delattr__ :: def __delattr__(self, name: str, /) -> None
        __dict__ :: MappingProxyType[str, Any]
        __dictoffset__ :: int
        __dir__ :: def __dir__(self) -> Iterable[str]
        __doc__ :: str | None
        __eq__ :: def __eq__(self, value: object, /) -> bool
        __flags__ :: int
        __format__ :: def __format__(self, format_spec: str, /) -> str
        __getattribute__ :: def __getattribute__(self, name: str, /) -> Any
        __getstate__ :: def __getstate__(self) -> object
        __hash__ :: def __hash__(self) -> int
        __init__ :: def __init__(self) -> Unknown
        __init_subclass__ :: bound method <class 'Quux'>.__init_subclass__() -> None
        __instancecheck__ :: bound method <class 'Quux'>.__instancecheck__(instance: Any, /) -> bool
        __itemsize__ :: int
        __module__ :: str
        __mro__ :: tuple[type, ...]
        __name__ :: str
        __ne__ :: def __ne__(self, value: object, /) -> bool
        __new__ :: def __new__(cls) -> Self@__new__
        __or__ :: bound method <class 'Quux'>.__or__[Self](value: Any, /) -> UnionType | Self@__or__
        __prepare__ :: bound method <class 'type'>.__prepare__(name: str, bases: tuple[type, ...], /, **kwds: Any) -> MutableMapping[str, object]
        __qualname__ :: str
        __reduce__ :: def __reduce__(self) -> str | tuple[Any, ...]
        __reduce_ex__ :: def __reduce_ex__(self, protocol: SupportsIndex, /) -> str | tuple[Any, ...]
        __repr__ :: def __repr__(self) -> str
        __ror__ :: bound method <class 'Quux'>.__ror__[Self](value: Any, /) -> UnionType | Self@__ror__
        __setattr__ :: def __setattr__(self, name: str, value: Any, /) -> None
        __sizeof__ :: def __sizeof__(self) -> int
        __str__ :: def __str__(self) -> str
        __subclasscheck__ :: bound method <class 'Quux'>.__subclasscheck__(subclass: type, /) -> bool
        __subclasses__ :: bound method <class 'Quux'>.__subclasses__[Self]() -> list[Self@__subclasses__]
        __subclasshook__ :: bound method <class 'Quux'>.__subclasshook__(subclass: type, /) -> bool
        __text_signature__ :: str | None
        __type_params__ :: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
        __weakrefoffset__ :: int
        ");
    }

    #[test]
    fn enum_attributes() {
        let builder = completion_test_builder(
            "\
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

Answer.<CURSOR>
",
        );

        insta::with_settings!({
            // See above: filter out some members which contain @Todo types that are
            // rendered differently in release mode.
            filters => [(r"(?m)\s*__(call|reduce_ex|annotate|signature)__.+$", "")]},
            {
                assert_snapshot!(builder.skip_builtins().type_signatures().build().snapshot(), @r"
                NO :: Literal[Answer.NO]
                YES :: Literal[Answer.YES]
                mro :: bound method <class 'Answer'>.mro() -> list[type]
                name :: Any
                value :: Any
                __annotations__ :: dict[str, Any]
                __base__ :: type | None
                __bases__ :: tuple[type, ...]
                __basicsize__ :: int
                __bool__ :: bound method <class 'Answer'>.__bool__() -> Literal[True]
                __class__ :: <class 'EnumMeta'>
                __contains__ :: bound method <class 'Answer'>.__contains__(value: object) -> bool
                __copy__ :: def __copy__(self) -> Self@__copy__
                __deepcopy__ :: def __deepcopy__(self, memo: Any) -> Self@__deepcopy__
                __delattr__ :: def __delattr__(self, name: str, /) -> None
                __dict__ :: MappingProxyType[str, Any]
                __dictoffset__ :: int
                __dir__ :: def __dir__(self) -> list[str]
                __doc__ :: str | None
                __eq__ :: def __eq__(self, value: object, /) -> bool
                __flags__ :: int
                __format__ :: def __format__(self, format_spec: str) -> str
                __getattribute__ :: def __getattribute__(self, name: str, /) -> Any
                __getitem__ :: bound method <class 'Answer'>.__getitem__[_EnumMemberT](name: str) -> _EnumMemberT@__getitem__
                __getstate__ :: def __getstate__(self) -> object
                __hash__ :: def __hash__(self) -> int
                __init__ :: def __init__(self) -> None
                __init_subclass__ :: bound method <class 'Answer'>.__init_subclass__() -> None
                __instancecheck__ :: bound method <class 'Answer'>.__instancecheck__(instance: Any, /) -> bool
                __itemsize__ :: int
                __iter__ :: bound method <class 'Answer'>.__iter__[_EnumMemberT]() -> Iterator[_EnumMemberT@__iter__]
                __len__ :: bound method <class 'Answer'>.__len__() -> int
                __members__ :: MappingProxyType[str, Unknown]
                __module__ :: str
                __mro__ :: tuple[type, ...]
                __name__ :: str
                __ne__ :: def __ne__(self, value: object, /) -> bool
                __new__ :: def __new__(cls, value: object) -> Self@__new__
                __or__ :: bound method <class 'Answer'>.__or__[Self](value: Any, /) -> UnionType | Self@__or__
                __order__ :: str
                __prepare__ :: bound method <class 'EnumMeta'>.__prepare__(cls: str, bases: tuple[type, ...], **kwds: Any) -> _EnumDict
                __qualname__ :: str
                __reduce__ :: def __reduce__(self) -> str | tuple[Any, ...]
                __repr__ :: def __repr__(self) -> str
                __reversed__ :: bound method <class 'Answer'>.__reversed__[_EnumMemberT]() -> Iterator[_EnumMemberT@__reversed__]
                __ror__ :: bound method <class 'Answer'>.__ror__[Self](value: Any, /) -> UnionType | Self@__ror__
                __setattr__ :: def __setattr__(self, name: str, value: Any, /) -> None
                __sizeof__ :: def __sizeof__(self) -> int
                __str__ :: def __str__(self) -> str
                __subclasscheck__ :: bound method <class 'Answer'>.__subclasscheck__(subclass: type, /) -> bool
                __subclasses__ :: bound method <class 'Answer'>.__subclasses__[Self]() -> list[Self@__subclasses__]
                __subclasshook__ :: bound method <class 'Answer'>.__subclasshook__(subclass: type, /) -> bool
                __text_signature__ :: str | None
                __type_params__ :: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
                __weakrefoffset__ :: int
                _add_alias_ :: def _add_alias_(self, name: str) -> None
                _add_value_alias_ :: def _add_value_alias_(self, value: Any) -> None
                _generate_next_value_ :: def _generate_next_value_(name: str, start: int, count: int, last_values: list[Any]) -> Any
                _ignore_ :: str | list[str]
                _member_map_ :: dict[str, Enum]
                _member_names_ :: list[str]
                _missing_ :: bound method <class 'Answer'>._missing_(value: object) -> Any
                _name_ :: str
                _order_ :: str
                _value2member_map_ :: dict[Any, Enum]
                _value_ :: Any
                ");
            }
        );
    }

    // We don't yet take function parameters into account.
    #[test]
    fn call_prefix1() {
        let builder = completion_test_builder(
            "\
def bar(okay=None): ...

foo = 1

bar(o<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn call_blank1() {
        let builder = completion_test_builder(
            "\
def bar(okay=None): ...

foo = 1

bar(<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        bar
        foo
        ");
    }

    #[test]
    fn duplicate1() {
        let builder = completion_test_builder(
            "\
def foo(): ...

class C:
    def foo(self): ...
    def bar(self):
        f<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        self
        ");
    }

    #[test]
    fn instance_methods_are_not_regular_functions1() {
        let builder = completion_test_builder(
            "\
class C:
    def foo(self): ...

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"C");
    }

    #[test]
    fn instance_methods_are_not_regular_functions2() {
        let builder = completion_test_builder(
            "\
class C:
    def foo(self): ...
    def bar(self):
        f<CURSOR>
",
        );

        // FIXME: Should NOT include `foo` here, since
        // that is only a method that can be called on
        // `self`.
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        foo
        self
        ");
    }

    #[test]
    fn identifier_keyword_clash1() {
        let builder = completion_test_builder(
            "\
classy_variable_name = 1

class<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"classy_variable_name");
    }

    #[test]
    fn identifier_keyword_clash2() {
        let builder = completion_test_builder(
            "\
some_symbol = 1

print(f\"{some<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"some_symbol");
    }

    #[test]
    fn statically_unreachable_symbols() {
        let builder = completion_test_builder(
            "\
if 1 + 2 != 3:
    hidden_symbol = 1

hidden_<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn completions_inside_unreachable_sections() {
        let builder = completion_test_builder(
            "\
import sys

if sys.platform == \"not-my-current-platform\":
    only_available_in_this_branch = 1

    on<CURSOR>
",
        );

        // TODO: ideally, `only_available_in_this_branch` should be available here, but we
        // currently make no effort to provide a good IDE experience within sections that
        // are unreachable
        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn star_import() {
        let builder = completion_test_builder(
            "\
from typing import *

Re<CURSOR>
",
        );

        // `ReadableBuffer` is a symbol in `typing`, but it is not re-exported
        builder
            .build()
            .contains("Reversible")
            .not_contains("ReadableBuffer");
    }

    #[test]
    fn attribute_access_empty_list() {
        let builder = completion_test_builder(
            "\
[].<CURSOR>
",
        );
        builder.build().contains("append");
    }

    #[test]
    fn attribute_access_empty_dict() {
        let builder = completion_test_builder(
            "\
{}.<CURSOR>
",
        );

        builder.build().contains("values").not_contains("add");
    }

    #[test]
    fn attribute_access_set() {
        let builder = completion_test_builder(
            "\
{1}.<CURSOR>
",
        );

        builder.build().contains("add").not_contains("values");
    }

    #[test]
    fn attribute_parens() {
        let builder = completion_test_builder(
            "\
class A:
    x: str

a = A()
(a).<CURSOR>
",
        );

        builder.build().contains("x");
    }

    #[test]
    fn attribute_double_parens() {
        let builder = completion_test_builder(
            "\
class A:
    x: str

a = A()
((a)).<CURSOR>
",
        );

        builder.build().contains("x");
    }

    #[test]
    fn attribute_on_constructor_directly() {
        let builder = completion_test_builder(
            "\
class A:
    x: str

A().<CURSOR>
",
        );

        builder.build().contains("x");
    }

    #[test]
    fn attribute_not_on_integer() {
        let builder = completion_test_builder(
            "\
3.<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn attribute_on_integer() {
        let builder = completion_test_builder(
            "\
(3).<CURSOR>
",
        );

        builder.build().contains("bit_length");
    }

    #[test]
    fn attribute_on_float() {
        let builder = completion_test_builder(
            "\
3.14.<CURSOR>
",
        );

        builder.build().contains("conjugate");
    }

    #[test]
    fn nested_attribute_access1() {
        let builder = completion_test_builder(
            "\
class A:
    x: str

class B:
    a: A

b = B()
b.a.<CURSOR>
",
        );

        builder.build().not_contains("a").contains("x");
    }

    #[test]
    fn nested_attribute_access2() {
        let builder = completion_test_builder(
            "\
class B:
    c: int

class A:
    b: B

a = A()
([1] + [a.b.<CURSOR>] + [3]).pop()
",
        );

        builder
            .build()
            .contains("c")
            .not_contains("b")
            .not_contains("pop");
    }

    #[test]
    fn nested_attribute_access3() {
        let builder = completion_test_builder(
            "\
a = A()
([1] + [\"abc\".<CURSOR>] + [3]).pop()
",
        );

        builder
            .build()
            .contains("capitalize")
            .not_contains("append")
            .not_contains("pop");
    }

    #[test]
    fn nested_attribute_access4() {
        let builder = completion_test_builder(
            "\
class B:
    c: int

class A:
    b: B

def foo() -> A:
    return A()

foo().<CURSOR>
",
        );

        builder.build().contains("b").not_contains("c");
    }

    #[test]
    fn nested_attribute_access5() {
        let builder = completion_test_builder(
            "\
class B:
    c: int

class A:
    b: B

def foo() -> A:
    return A()

foo().b.<CURSOR>
",
        );

        builder.build().contains("c").not_contains("b");
    }

    #[test]
    fn betwixt_attribute_access1() {
        let builder = completion_test_builder(
            "\
class Foo:
    xyz: str

class Bar:
    foo: Foo

class Quux:
    bar: Bar

quux = Quux()
quux.<CURSOR>.foo.xyz
",
        );

        builder
            .build()
            .contains("bar")
            .not_contains("xyz")
            .not_contains("foo");
    }

    #[test]
    fn betwixt_attribute_access2() {
        let builder = completion_test_builder(
            "\
class Foo:
    xyz: str

class Bar:
    foo: Foo

class Quux:
    bar: Bar

quux = Quux()
quux.b<CURSOR>.foo.xyz
",
        );

        builder
            .build()
            .contains("bar")
            .not_contains("xyz")
            .not_contains("foo");
    }

    #[test]
    fn betwixt_attribute_access3() {
        let builder = completion_test_builder(
            "\
class Foo:
    xyz: str

class Bar:
    foo: Foo

class Quux:
    bar: Bar

quux = Quux()
<CURSOR>.foo.xyz
",
        );

        builder.build().contains("quux");
    }

    #[test]
    fn betwixt_attribute_access4() {
        let builder = completion_test_builder(
            "\
class Foo:
    xyz: str

class Bar:
    foo: Foo

class Quux:
    bar: Bar

quux = Quux()
q<CURSOR>.foo.xyz
",
        );

        builder.build().contains("quux");
    }

    #[test]
    fn ellipsis1() {
        let builder = completion_test_builder(
            "\
...<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn ellipsis2() {
        let builder = completion_test_builder(
            "\
....<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        __annotations__
        __class__
        __delattr__
        __dict__
        __dir__
        __doc__
        __eq__
        __format__
        __getattribute__
        __getstate__
        __hash__
        __init__
        __init_subclass__
        __module__
        __ne__
        __new__
        __reduce__
        __reduce_ex__
        __repr__
        __setattr__
        __sizeof__
        __str__
        __subclasshook__
        ");
    }

    #[test]
    fn ellipsis3() {
        let builder = completion_test_builder(
            "\
class Foo: ...<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn ordering() {
        let builder = completion_test_builder(
            "\
class A:
    foo: str
    _foo: str
    __foo__: str
    __foo: str
    FOO: str
    _FOO: str
    __FOO__: str
    __FOO: str

A.<CURSOR>
",
        );

        assert_snapshot!(
            builder.filter(|c| c.name.contains("FOO") || c.name.contains("foo")).build().snapshot(),
            @r"
        FOO
        foo
        __FOO__
        __foo__
        _FOO
        __FOO
        __foo
        _foo
        ",
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_function_identifier1() {
        let builder = completion_test_builder(
            "\
def m<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_function_identifier2() {
        let builder = completion_test_builder(
            "\
def m<CURSOR>(): pass
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn fscope_id_missing_function_identifier3() {
        let builder = completion_test_builder(
            "\
def m(): pass
<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"
        m
        ");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_class_identifier1() {
        let builder = completion_test_builder(
            "\
class M<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_type_alias1() {
        let builder = completion_test_builder(
            "\
Fo<CURSOR> = float
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"Fo");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_import1() {
        let builder = completion_test_builder(
            "\
import fo<CURSOR>
",
        );

        // This snapshot would generate a big list of modules,
        // which is kind of annoying. So just assert that it
        // runs without panicking and produces some non-empty
        // output.
        assert!(!builder.skip_builtins().build().completions().is_empty());
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_import2() {
        let builder = completion_test_builder(
            "\
import foo as ba<CURSOR>
",
        );

        // This snapshot would generate a big list of modules,
        // which is kind of annoying. So just assert that it
        // runs without panicking and produces some non-empty
        // output.
        assert!(!builder.skip_builtins().build().completions().is_empty());
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_from_import1() {
        let builder = completion_test_builder(
            "\
from fo<CURSOR> import wat
",
        );

        // This snapshot would generate a big list of modules,
        // which is kind of annoying. So just assert that it
        // runs without panicking and produces some non-empty
        // output.
        assert!(!builder.skip_builtins().build().completions().is_empty());
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_from_import2() {
        let builder = completion_test_builder(
            "\
from foo import wa<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_from_import3() {
        let builder = completion_test_builder(
            "\
from foo import wat as ba<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_try_except1() {
        let builder = completion_test_builder(
            "\
try:
    pass
except Type<CURSOR>:
    pass
",
        );

        assert_snapshot!(
            builder.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_global1() {
        let builder = completion_test_builder(
            "\
def _():
    global fo<CURSOR>
",
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn string_dot_attr1() {
        let builder = completion_test_builder(
            r#"
foo = 1
bar = 2

class Foo:
    def method(self): ...

f = Foo()

# String, this is not an attribute access
"f.<CURSOR>
"#,
        );

        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"<No completions found>");
    }

    #[test]
    fn string_dot_attr2() {
        let builder = completion_test_builder(
            r#"
foo = 1
bar = 2

class Foo:
    def method(self): ...

f = Foo()

# F-string, this is an attribute access
f"{f.<CURSOR>
"#,
        );

        builder.build().contains("method");
    }

    #[test]
    fn string_dot_attr3() {
        let builder = completion_test_builder(
            r#"
foo = 1
bar = 2

class Foo:
    def method(self): ...

f = Foo()

# T-string, this is an attribute access
t"{f.<CURSOR>
"#,
        );

        builder.build().contains("method");
    }

    #[test]
    fn no_panic_for_attribute_table_that_contains_subscript() {
        let builder = completion_test_builder(
            r#"
class Point:
    def orthogonal_direction(self):
        self[0].is_zero

def test_point(p2: Point):
    p2.<CURSOR>
"#,
        );
        builder.build().contains("orthogonal_direction");
    }

    #[test]
    fn from_import1() {
        let builder = completion_test_builder(
            "\
from sys import <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import2() {
        let builder = completion_test_builder(
            "\
from sys import abiflags, <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import3() {
        let builder = completion_test_builder(
            "\
from sys import <CURSOR>, abiflags
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import4() {
        let builder = completion_test_builder(
            "\
from sys import abiflags, \
    <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import5() {
        let builder = completion_test_builder(
            "\
from sys import abiflags as foo, <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import6() {
        let builder = completion_test_builder(
            "\
from sys import abiflags as foo, g<CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import7() {
        let builder = completion_test_builder(
            "\
from sys import abiflags as foo, \
    <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import8() {
        let builder = completion_test_builder(
            "\
from sys import abiflags as foo, \
    g<CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import9() {
        let builder = completion_test_builder(
            "\
from sys import (
    abiflags,
    <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import10() {
        let builder = completion_test_builder(
            "\
from sys import (
    abiflags,
    <CURSOR>
)
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import11() {
        let builder = completion_test_builder(
            "\
from sys import (
    <CURSOR>
)
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import_unknown_in_module() {
        let builder = completion_test_builder(
            "\
foo = 1
from ? import <CURSOR>
",
        );
        assert_snapshot!(builder.skip_builtins().build().snapshot(), @r"<No completions found>");
    }

    #[test]
    fn from_import_unknown_in_import_names1() {
        let builder = completion_test_builder(
            "\
from sys import ?, <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import_unknown_in_import_names2() {
        let builder = completion_test_builder(
            "\
from sys import ??, <CURSOR>
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn from_import_unknown_in_import_names3() {
        let builder = completion_test_builder(
            "\
from sys import ??, <CURSOR>, ??
",
        );
        builder.build().contains("getsizeof");
    }

    #[test]
    fn relative_from_import1() {
        CursorTest::builder()
            .source("package/__init__.py", "")
            .source(
                "package/foo.py",
                "\
Cheetah = 1
Lion = 2
Cougar = 3
",
            )
            .source("package/sub1/sub2/bar.py", "from ...foo import <CURSOR>")
            .completion_test_builder()
            .build()
            .contains("Cheetah");
    }

    #[test]
    fn relative_from_import2() {
        CursorTest::builder()
            .source("package/__init__.py", "")
            .source(
                "package/sub1/foo.py",
                "\
Cheetah = 1
Lion = 2
Cougar = 3
",
            )
            .source("package/sub1/sub2/bar.py", "from ..foo import <CURSOR>")
            .completion_test_builder()
            .build()
            .contains("Cheetah");
    }

    #[test]
    fn relative_from_import3() {
        CursorTest::builder()
            .source("package/__init__.py", "")
            .source(
                "package/sub1/sub2/foo.py",
                "\
Cheetah = 1
Lion = 2
Cougar = 3
",
            )
            .source("package/sub1/sub2/bar.py", "from .foo import <CURSOR>")
            .completion_test_builder()
            .build()
            .contains("Cheetah");
    }

    #[test]
    fn from_import_with_submodule1() {
        CursorTest::builder()
            .source("main.py", "from package import <CURSOR>")
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/bar.pyi", "")
            .source("package/foo-bar.py", "")
            .source("package/data.txt", "")
            .source("package/sub/__init__.py", "")
            .source("package/not-a-submodule/__init__.py", "")
            .completion_test_builder()
            .build()
            .contains("foo")
            .contains("bar")
            .contains("sub")
            .not_contains("foo-bar")
            .not_contains("data")
            .not_contains("not-a-submodule");
    }

    #[test]
    fn from_import_with_vendored_submodule1() {
        let builder = completion_test_builder(
            "\
from http import <CURSOR>
",
        );
        builder.build().contains("client");
    }

    #[test]
    fn from_import_with_vendored_submodule2() {
        let builder = completion_test_builder(
            "\
from email import <CURSOR>
",
        );
        builder.build().contains("mime").not_contains("base");
    }

    #[test]
    fn import_submodule_not_attribute1() {
        let builder = completion_test_builder(
            "\
import importlib
importlib.<CURSOR>
",
        );
        builder.build().not_contains("resources");
    }

    #[test]
    fn import_submodule_not_attribute2() {
        let builder = completion_test_builder(
            "\
import importlib.resources
importlib.<CURSOR>
",
        );
        builder.build().contains("resources");
    }

    #[test]
    fn import_submodule_not_attribute3() {
        let builder = completion_test_builder(
            "\
import importlib
import importlib.resources
importlib.<CURSOR>
",
        );
        builder.build().contains("resources");
    }

    #[test]
    fn import_with_leading_character() {
        let builder = completion_test_builder(
            "\
import c<CURSOR>
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_without_leading_character() {
        let builder = completion_test_builder(
            "\
import <CURSOR>
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_multiple() {
        let builder = completion_test_builder(
            "\
import re, c<CURSOR>, sys
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_with_aliases() {
        let builder = completion_test_builder(
            "\
import re as regexp, c<CURSOR>, sys as system
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_over_multiple_lines() {
        let builder = completion_test_builder(
            "\
import re as regexp, \\
    c<CURSOR>, \\
    sys as system
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_unknown_in_module() {
        let builder = completion_test_builder(
            "\
import ?, <CURSOR>
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_via_from_with_leading_character() {
        let builder = completion_test_builder(
            "\
from c<CURSOR>
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_via_from_without_leading_character() {
        let builder = completion_test_builder(
            "\
from <CURSOR>
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_statement_with_submodule_with_leading_character() {
        let builder = completion_test_builder(
            "\
import os.p<CURSOR>
",
        );
        builder.build().contains("path").not_contains("abspath");
    }

    #[test]
    fn import_statement_with_submodule_multiple() {
        let builder = completion_test_builder(
            "\
import re, os.p<CURSOR>, zlib
",
        );
        builder.build().contains("path").not_contains("abspath");
    }

    #[test]
    fn import_statement_with_submodule_without_leading_character() {
        let builder = completion_test_builder(
            "\
import os.<CURSOR>
",
        );
        builder.build().contains("path").not_contains("abspath");
    }

    #[test]
    fn import_via_from_with_submodule_with_leading_character() {
        let builder = completion_test_builder(
            "\
from os.p<CURSOR>
",
        );
        builder.build().contains("path").not_contains("abspath");
    }

    #[test]
    fn import_via_from_with_submodule_without_leading_character() {
        let builder = completion_test_builder(
            "\
from os.<CURSOR>
",
        );
        builder.build().contains("path").not_contains("abspath");
    }

    #[test]
    fn auto_import_with_submodule() {
        CursorTest::builder()
            .source("main.py", "Abra<CURSOR>")
            .source("package/__init__.py", "AbraKadabra = 1")
            .completion_test_builder()
            .auto_import()
            .build()
            .contains("AbraKadabra");
    }

    #[test]
    fn auto_import_should_not_include_symbols_in_current_module() {
        let snapshot = CursorTest::builder()
            .source("main.py", "Kadabra = 1\nKad<CURSOR>")
            .source("package/__init__.py", "AbraKadabra = 1")
            .completion_test_builder()
            .auto_import()
            .type_signatures()
            .module_names()
            .filter(|c| c.name.contains("Kadabra"))
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @r"
        Kadabra :: Literal[1] :: Current module
        AbraKadabra :: Unavailable :: package
        ");
    }

    #[test]
    fn import_type_check_only_lowers_ranking() {
        let builder = CursorTest::builder()
            .source(
                "main.py",
                r#"
                import foo
                foo.A<CURSOR>
                "#,
            )
            .source(
                "foo/__init__.py",
                r#"
                from typing import type_check_only

                @type_check_only
                class Apple: pass

                class Banana: pass
                class Cat: pass
                class Azorubine: pass
                "#,
            )
            .completion_test_builder();

        let test = builder.build();
        let completions = test.completions();

        let [apple_pos, banana_pos, cat_pos, azo_pos, ann_pos] =
            ["Apple", "Banana", "Cat", "Azorubine", "__annotations__"].map(|name| {
                completions
                    .iter()
                    .position(|comp| comp.name == name)
                    .unwrap()
            });

        assert!(completions[apple_pos].is_type_check_only);
        assert!(apple_pos > banana_pos.max(cat_pos).max(azo_pos));
        assert!(ann_pos > apple_pos);
    }

    #[test]
    fn type_check_only_is_type_check_only() {
        // `@typing.type_check_only` is a function that's unavailable at runtime
        // and so should be the last "non-underscore" completion in `typing`
        let builder = completion_test_builder("from typing import t<CURSOR>");
        let test = builder.build();
        let last_nonunderscore = test
            .completions()
            .iter()
            .filter(|c| !c.name.starts_with('_'))
            .next_back()
            .unwrap();

        assert_eq!(&last_nonunderscore.name, "type_check_only");
        assert!(last_nonunderscore.is_type_check_only);
    }

    #[test]
    fn regression_test_issue_642() {
        // Regression test for https://github.com/astral-sh/ty/issues/642

        let test = completion_test_builder(
            r#"
            match 0:
                case 1 i<CURSOR>:
                    pass
            "#,
        );

        assert_snapshot!(
            test.skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
    }

    #[test]
    fn completion_kind_recursive_type_alias() {
        let builder = completion_test_builder(
            r#"
            type T = T | None
            def f(rec: T):
                re<CURSOR>
            "#,
        );
        let test = builder.build();

        let completion = test.completions().iter().find(|c| c.name == "rec").unwrap();
        assert_eq!(completion.kind(builder.db()), Some(CompletionKind::Struct));
    }

    #[test]
    fn no_completions_in_comment() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
# zqzq<CURSOR>
",
        );

        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"zqzq<CURSOR>\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"\"\"Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"\"\"Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('''Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_string_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('''zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('''Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"zqzq<CURSOR>\")
        ",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"{Foo} and Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"zqzq<CURSOR>
        ",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'{Foo} and Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"\"\"{Foo} and Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"\"\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'''{Foo} and Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_fstring_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'''zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'''{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"zqzq<CURSOR>\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"{Foo} and Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'{Foo} and Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"\"\"{Foo} and Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"\"\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'''{Foo} and Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn no_completions_in_tstring_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'''zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'''{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(test.skip_builtins().build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn typevar_with_upper_bound() {
        let builder = completion_test_builder(
            "\
def f[T: str](msg: T):
    msg.<CURSOR>
",
        );
        let test = builder.build();
        test.contains("upper");
        test.contains("capitalize");
    }

    #[test]
    fn typevar_with_constraints() {
        // Test TypeVar with constraints
        let builder = completion_test_builder(
            "\
from typing import TypeVar

class A:
    only_on_a: int
    on_a_and_b: str

class B:
    only_on_b: float
    on_a_and_b: str

T = TypeVar('T', A, B)

def f(x: T):
    x.<CURSOR>
",
        );
        let test = builder.build();

        test.contains("on_a_and_b");
        test.not_contains("only_on_a");
        test.not_contains("only_on_b");
    }

    #[test]
    fn typevar_without_bounds_or_constraints() {
        let test = completion_test_builder(
            "\
def f[T](x: T):
    x.<CURSOR>
",
        );
        test.build().contains("__repr__");
    }

    #[test]
    fn no_completions_in_function_def_name() {
        let builder = completion_test_builder(
            "\
foo = 1

def f<CURSOR>
    ",
        );

        assert!(builder.build().completions().is_empty());
    }

    #[test]
    fn completions_in_function_def_empty_name() {
        let builder = completion_test_builder(
            "\
def <CURSOR>
        ",
        );

        // This is okay because the ide will not request completions when the cursor is in this position.
        assert!(!builder.build().completions().is_empty());
    }

    #[test]
    fn no_completions_in_class_def_name() {
        let builder = completion_test_builder(
            "\
foo = 1

class f<CURSOR>
    ",
        );

        assert!(builder.build().completions().is_empty());
    }

    #[test]
    fn completions_in_class_def_empty_name() {
        let builder = completion_test_builder(
            "\
class <CURSOR>
        ",
        );

        // This is okay because the ide will not request completions when the cursor is in this position.
        assert!(!builder.build().completions().is_empty());
    }

    #[test]
    fn no_completions_in_type_def_name() {
        let builder = completion_test_builder(
            "\
foo = 1

type f<CURSOR> = int
    ",
        );

        assert!(builder.build().completions().is_empty());
    }

    #[test]
    fn no_completions_in_maybe_type_def_name() {
        let builder = completion_test_builder(
            "\
foo = 1

type f<CURSOR>
       ",
        );

        assert!(builder.build().completions().is_empty());
    }

    #[test]
    fn completions_in_type_def_empty_name() {
        let builder = completion_test_builder(
            "\
type <CURSOR>
        ",
        );

        // This is okay because the ide will not request completions when the cursor is in this position.
        assert!(!builder.build().completions().is_empty());
    }

    #[test]
    fn favour_symbols_currently_imported() {
        let snapshot = CursorTest::builder()
            .source("main.py", "long_nameb = 1\nlong_name<CURSOR>")
            .source("foo.py", "def long_namea(): ...")
            .completion_test_builder()
            .type_signatures()
            .auto_import()
            .module_names()
            .filter(|c| c.name.contains("long_name"))
            .build()
            .snapshot();

        // Even though long_namea is alphabetically before long_nameb,
        // long_nameb is currently imported and should be preferred.
        assert_snapshot!(snapshot, @r"
        long_nameb :: Literal[1] :: Current module
        long_namea :: Unavailable :: foo
        ");
    }

    #[test]
    fn favour_imported_over_builtin() {
        let snapshot =
            completion_test_builder("from typing import Protocol\nclass Foo(P<CURSOR>: ...")
                .filter(|c| c.name.starts_with('P'))
                .build()
                .snapshot();

        // Here we favour `Protocol` over the other completions
        // because `Protocol` has been imported, and the other completions are builtin.
        assert_snapshot!(snapshot, @r"
        Protocol
        PendingDeprecationWarning
        PermissionError
        ProcessLookupError
        PythonFinalizationError
        ");
    }

    /// A way to create a simple single-file (named `main.py`) completion test
    /// builder.
    ///
    /// Use cases that require multiple files with a `<CURSOR>` marker
    /// in a file other than `main.py` can use `CursorTest::builder()`
    /// and then `CursorTestBuilder::completion_test_builder()`.
    fn completion_test_builder(source: &str) -> CompletionTestBuilder {
        CursorTest::builder()
            .source("main.py", source)
            .completion_test_builder()
    }

    /// A builder for executing a completion test.
    ///
    /// This mostly owns the responsibility for generating snapshots
    /// of completions from a cursor position in source code. Most of
    /// the options involve some kind of filtering or adjustment to
    /// apply to the snapshots, depending on what one wants to test.
    struct CompletionTestBuilder {
        cursor_test: CursorTest,
        settings: CompletionSettings,
        skip_builtins: bool,
        type_signatures: bool,
        module_names: bool,
        // This doesn't seem like a "very complex" type to me... ---AG
        #[allow(clippy::type_complexity)]
        predicate: Option<Box<dyn Fn(&Completion) -> bool>>,
    }

    impl CompletionTestBuilder {
        /// Returns completions based on this configuration.
        fn build(&self) -> CompletionTest<'_> {
            let original = completion(
                &self.cursor_test.db,
                &self.settings,
                self.cursor_test.cursor.file,
                self.cursor_test.cursor.offset,
            );
            let filtered = original
                .iter()
                .filter(|c| !self.skip_builtins || !c.builtin)
                .filter(|c| {
                    self.predicate
                        .as_ref()
                        .map(|predicate| predicate(c))
                        .unwrap_or(true)
                })
                .cloned()
                .collect();
            CompletionTest {
                db: self.db(),
                original,
                filtered,
                type_signatures: self.type_signatures,
                module_names: self.module_names,
            }
        }

        /// Returns the underlying test DB.
        fn db(&self) -> &ty_project::TestDb {
            &self.cursor_test.db
        }

        /// When enabled, symbols that aren't in scope but available
        /// in the environment will be included.
        ///
        /// Not enabled by default.
        fn auto_import(mut self) -> CompletionTestBuilder {
            self.settings.auto_import = true;
            self
        }

        /// When set, builtins from completions are skipped. This is
        /// useful in tests to reduce noise for scope based completions.
        ///
        /// Not enabled by default.
        fn skip_builtins(mut self) -> CompletionTestBuilder {
            self.skip_builtins = true;
            self
        }

        /// When set, type signatures of each completion item are
        /// included in the snapshot. This is useful when one wants
        /// to specifically test types, but it usually best to leave
        /// off as it can add lots of noise.
        ///
        /// Not enabled by default.
        fn type_signatures(mut self) -> CompletionTestBuilder {
            self.type_signatures = true;
            self
        }

        /// When set, the module name for each symbol is included
        /// in the snapshot (if available).
        fn module_names(mut self) -> CompletionTestBuilder {
            self.module_names = true;
            self
        }

        /// Apply arbitrary filtering to completions.
        fn filter(
            mut self,
            predicate: impl Fn(&Completion) -> bool + 'static,
        ) -> CompletionTestBuilder {
            self.predicate = Some(Box::new(predicate));
            self
        }
    }

    struct CompletionTest<'db> {
        db: &'db ty_project::TestDb,
        /// The original completions returned before any additional
        /// test-specific filtering. We keep this around in order to
        /// slightly modify the test snapshot generated. This
        /// lets us differentiate between "absolutely no completions
        /// were returned" and "completions were returned, but you
        /// filtered them out."
        original: Vec<Completion<'db>>,
        /// The completions that the test should act upon. These are
        /// filtered by things like `skip_builtins`.
        filtered: Vec<Completion<'db>>,
        /// Whether type signatures should be included in the snapshot
        /// generated by `CompletionTest::snapshot`.
        type_signatures: bool,
        /// Whether module names should be included in the snapshot
        /// generated by `CompletionTest::snapshot`.
        module_names: bool,
    }

    impl<'db> CompletionTest<'db> {
        fn snapshot(&self) -> String {
            if self.original.is_empty() {
                return "<No completions found>".to_string();
            } else if self.filtered.is_empty() {
                // It'd be nice to include the actual number of
                // completions filtered out, but in practice, the
                // number is environment dependent. For example, on
                // Windows, there are 231 builtins, but on Unix, there
                // are 230. So we just leave out the number I guess.
                // ---AG
                return "<No completions found after filtering out completions>".to_string();
            }
            self.filtered
                .iter()
                .map(|c| {
                    let mut snapshot = c.name.as_str().to_string();
                    if self.type_signatures {
                        let ty =
                            c.ty.map(|ty| ty.display(self.db).to_string())
                                .unwrap_or_else(|| "Unavailable".to_string());
                        snapshot = format!("{snapshot} :: {ty}");
                    }
                    if self.module_names {
                        let module_name = c
                            .module_name
                            .map(ModuleName::as_str)
                            .unwrap_or("Current module");
                        snapshot = format!("{snapshot} :: {module_name}");
                    }
                    snapshot
                })
                .collect::<Vec<String>>()
                .join("\n")
        }

        #[track_caller]
        fn contains(&self, expected: &str) -> &CompletionTest<'db> {
            assert!(
                self.filtered
                    .iter()
                    .any(|completion| completion.name == expected),
                "Expected completions to include `{expected}`"
            );
            self
        }

        #[track_caller]
        fn not_contains(&self, unexpected: &str) -> &CompletionTest<'db> {
            assert!(
                self.filtered
                    .iter()
                    .all(|completion| completion.name != unexpected),
                "Expected completions to not include `{unexpected}`",
            );
            self
        }

        /// Returns the underlying completions if the convenience assertions
        /// aren't sufficiently expressive.
        fn completions(&self) -> &[Completion<'db>] {
            &self.filtered
        }
    }

    impl CursorTestBuilder {
        fn completion_test_builder(&self) -> CompletionTestBuilder {
            CompletionTestBuilder {
                cursor_test: self.build(),
                settings: CompletionSettings::default(),
                skip_builtins: false,
                type_signatures: false,
                module_names: false,
                predicate: None,
            }
        }
    }

    fn tokenize(src: &str) -> Tokens {
        let parsed = ruff_python_parser::parse(src, ParseOptions::from(Mode::Module))
            .expect("valid Python source for token stream");
        parsed.tokens().clone()
    }
}
