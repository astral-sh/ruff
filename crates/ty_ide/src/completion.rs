use std::cmp::Ordering;

use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::source_text;
use ruff_diagnostics::Edit;
use ruff_python_ast::name::Name;
use ruff_python_ast::token::{Token, TokenAt, TokenKind, Tokens};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_codegen::Stylist;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use ty_python_semantic::types::UnionType;
use ty_python_semantic::{
    Completion as SemanticCompletion, KnownModule, ModuleName, NameKind, SemanticModel,
    types::{CycleDetector, KnownClass, Type},
};

use crate::docstring::Docstring;
use crate::find_node::covering_node;
use crate::goto::Definitions;
use crate::importer::{ImportRequest, Importer};
use crate::symbols::QueryPattern;
use crate::{Db, all_symbols};

/// A collection of completions built up from various sources.
#[derive(Clone)]
struct Completions<'db> {
    db: &'db dyn Db,
    items: Vec<Completion<'db>>,
    query: QueryPattern,
}

impl<'db> Completions<'db> {
    /// Create a new empty collection of completions.
    ///
    /// The given typed text should correspond to what we believe
    /// the user has typed as part of the next symbol they are writing.
    /// This collection will treat it as a query when present, and only
    /// add completions that match it.
    fn fuzzy(db: &'db dyn Db, typed: Option<&str>) -> Completions<'db> {
        let query = typed
            .map(QueryPattern::fuzzy)
            .unwrap_or_else(QueryPattern::matches_all_symbols);
        Completions {
            db,
            items: vec![],
            query,
        }
    }

    fn exactly(db: &'db dyn Db, symbol: &str) -> Completions<'db> {
        let query = QueryPattern::exactly(symbol);
        Completions {
            db,
            items: vec![],
            query,
        }
    }

    /// Convert this collection into a simple
    /// sequence of completions.
    fn into_completions(mut self) -> Vec<Completion<'db>> {
        self.items.sort_by(compare_suggestions);
        self.items
            .dedup_by(|c1, c2| (&c1.name, c1.module_name) == (&c2.name, c2.module_name));
        self.items
    }

    fn into_imports(mut self) -> Vec<ImportEdit> {
        self.items.sort_by(compare_suggestions);
        self.items
            .dedup_by(|c1, c2| (&c1.name, c1.module_name) == (&c2.name, c2.module_name));
        self.items
            .into_iter()
            .filter_map(|item| {
                Some(ImportEdit {
                    label: format!("import {}", item.qualified?),
                    edit: item.import?,
                })
            })
            .collect()
    }

    /// Attempts to adds the given completion to this collection.
    ///
    /// When added, `true` is returned.
    ///
    /// This might not add the completion for a variety of reasons.
    /// For example, if the symbol name does not match this collection's
    /// query.
    fn try_add(&mut self, completion: Completion<'db>) -> bool {
        if !self.query.is_match_symbol_name(completion.name.as_str()) {
            return false;
        }
        self.force_add(completion);
        true
    }

    /// Attempts to adds the given semantic completion to this collection.
    ///
    /// When added, `true` is returned.
    fn try_add_semantic(&mut self, completion: SemanticCompletion<'db>) -> bool {
        self.try_add(Completion::from_semantic_completion(self.db, completion))
    }

    /// Always adds the given completion to this collection.
    fn force_add(&mut self, completion: Completion<'db>) {
        self.items.push(completion);
    }

    /// Tags completions with whether they are known to be usable in
    /// a `raise` context.
    ///
    /// It's possible that some completions are usable in a `raise`
    /// but aren't marked by this method. That is, false negatives are
    /// possible but false positives are not.
    fn tag_raisable(&mut self) {
        let raisable_type = UnionType::from_elements(
            self.db,
            [
                KnownClass::BaseException.to_subclass_of(self.db),
                KnownClass::BaseException.to_instance(self.db),
            ],
        );
        for item in &mut self.items {
            let Some(ty) = item.ty else { continue };
            item.is_definitively_raisable = ty.is_assignable_to(self.db, raisable_type);
        }
    }

    /// Removes any completion that doesn't satisfy the given predicate.
    fn retain(&mut self, predicate: impl FnMut(&Completion<'_>) -> bool) {
        self.items.retain(predicate);
    }
}

impl<'db> Extend<SemanticCompletion<'db>> for Completions<'db> {
    fn extend<T>(&mut self, it: T)
    where
        T: IntoIterator<Item = SemanticCompletion<'db>>,
    {
        for c in it {
            self.try_add_semantic(c);
        }
    }
}

impl<'db> Extend<Completion<'db>> for Completions<'db> {
    fn extend<T>(&mut self, it: T)
    where
        T: IntoIterator<Item = Completion<'db>>,
    {
        for c in it {
            self.try_add(c);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Completion<'db> {
    /// The label shown to the user for this suggestion.
    pub name: Name,
    /// The fully qualified name, when available.
    ///
    /// This is only set when `module_name` is available.
    pub qualified: Option<Name>,
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
    /// Whether this item can definitively be used in a `raise` context.
    ///
    /// Note that this may not always be computed. (i.e., Only computed
    /// when we are in a `raise` context.) And also note that if this
    /// is `true`, then it's definitively usable in `raise`, but if
    /// it's `false`, it _may_ still be usable in `raise`.
    pub is_definitively_raisable: bool,
    /// The documentation associated with this item, if
    /// available.
    pub documentation: Option<Docstring>,
}

impl<'db> Completion<'db> {
    fn from_semantic_completion(
        db: &'db dyn Db,
        semantic: SemanticCompletion<'db>,
    ) -> Completion<'db> {
        let definition = semantic.ty.and_then(|ty| Definitions::from_ty(db, ty));
        let documentation = definition.and_then(|def| def.docstring(db));
        let is_type_check_only = semantic.is_type_check_only(db);
        Completion {
            name: semantic.name,
            qualified: None,
            insert: None,
            ty: semantic.ty,
            kind: None,
            module_name: None,
            import: None,
            builtin: semantic.builtin,
            is_type_check_only,
            is_definitively_raisable: false,
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
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_) => CompletionKind::Struct,
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

    fn keyword(name: &str) -> Self {
        Completion {
            name: name.into(),
            qualified: None,
            insert: None,
            ty: None,
            kind: Some(CompletionKind::Keyword),
            module_name: None,
            import: None,
            builtin: false,
            is_type_check_only: false,
            is_definitively_raisable: false,
            documentation: None,
        }
    }

    fn value_keyword(name: &str, ty: Type<'db>) -> Completion<'db> {
        Completion {
            name: name.into(),
            qualified: None,
            insert: None,
            ty: Some(ty),
            kind: Some(CompletionKind::Keyword),
            module_name: None,
            import: None,
            builtin: true,
            is_type_check_only: false,
            is_definitively_raisable: false,
            documentation: None,
        }
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
    let typed = find_typed_text(db, file, &parsed, offset);

    if is_in_no_completions_place(db, file, &parsed, offset, tokens, typed.as_deref()) {
        return vec![];
    }

    let mut completions = Completions::fuzzy(db, typed.as_deref());

    if let Some(import) = ImportStatement::detect(db, file, &parsed, tokens, typed.as_deref()) {
        import.add_completions(db, file, &mut completions);
    } else {
        let Some(target_token) = CompletionTargetTokens::find(&parsed, offset) else {
            return vec![];
        };
        let Some(target) = target_token.ast(&parsed, offset) else {
            return vec![];
        };

        let model = SemanticModel::new(db, file);
        let (semantic_completions, scoped) = match target {
            CompletionTargetAst::ObjectDot { expr } => (model.attribute_completions(expr), None),
            CompletionTargetAst::Scoped(scoped) => {
                (model.scoped_completions(scoped.node), Some(scoped))
            }
        };

        completions.extend(semantic_completions);
        if scoped.is_some() {
            add_keyword_completions(db, &mut completions);
        }
        if settings.auto_import {
            if let Some(scoped) = scoped {
                add_unimported_completions(
                    db,
                    file,
                    &parsed,
                    scoped,
                    |module_name: &ModuleName, symbol: &str| {
                        ImportRequest::import_from(module_name.as_str(), symbol)
                    },
                    &mut completions,
                );
            }
        }
    }

    if is_raising_exception(tokens) {
        completions.tag_raisable();

        // As a special case, and because it's a common footgun, we
        // specifically disallow `NotImplemented` in this context.
        // `NotImplementedError` should be used instead. So if we can
        // definitively detect `NotImplemented`, then we can safely
        // omit it from suggestions.
        completions.retain(|item| {
            let Some(ty) = item.ty else { return true };
            !ty.is_notimplemented(db)
        });
    }

    completions.into_completions()
}

pub(crate) struct ImportEdit {
    pub label: String,
    pub edit: Edit,
}

pub(crate) fn missing_imports(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    symbol: &str,
    node: AnyNodeRef,
) -> Vec<ImportEdit> {
    let mut completions = Completions::exactly(db, symbol);
    let scoped = ScopedTarget { node };
    add_unimported_completions(
        db,
        file,
        parsed,
        scoped,
        |module_name: &ModuleName, symbol: &str| {
            ImportRequest::import_from(module_name.as_str(), symbol).force()
        },
        &mut completions,
    );

    completions.into_imports()
}

/// Adds completions derived from keywords.
///
/// This should generally only be used when offering "scoped" completions.
/// This will include keywords corresponding to Python values (like `None`)
/// and general language keywords (like `raise`).
fn add_keyword_completions<'db>(db: &'db dyn Db, completions: &mut Completions<'db>) {
    let keyword_values = [
        ("None", Type::none(db)),
        ("True", Type::BooleanLiteral(true)),
        ("False", Type::BooleanLiteral(false)),
    ];
    for (name, ty) in keyword_values {
        completions.try_add(Completion::value_keyword(name, ty));
    }

    // Note that we specifically omit the `type` keyword here, since
    // it will be included via `builtins`. This does make its sorting
    // priority slighty different than other keywords, but it's not
    // clear (to me, AG) if that's an issue or not. Since the builtin
    // completion has an actual type associated with it, we use that
    // instead of a keyword completion.
    let keywords = [
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "case", "match",
    ];
    for name in keywords {
        completions.try_add(Completion::keyword(name));
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
    create_import_request: impl for<'a> Fn(&'a ModuleName, &'a str) -> ImportRequest<'a>,
    completions: &mut Completions<'db>,
) {
    // This is redundant since `all_symbols` will also bail
    // when the query can match everything. But we bail here
    // to avoid building an `Importer` and other plausibly
    // costly work when we know we won't use it.
    if completions.query.will_match_everything() {
        return;
    }

    let source = source_text(db, file);
    let stylist = Stylist::from_tokens(parsed.tokens(), source.as_str());
    let importer = Importer::new(db, &stylist, file, source.as_str(), parsed);
    let members = importer.members_in_scope_at(scoped.node, scoped.node.start());

    for symbol in all_symbols(db, file, &completions.query) {
        if symbol.file() == file || symbol.module().is_known(db, KnownModule::Builtins) {
            continue;
        }

        let module_name = symbol.module().name(db);
        let (name, qualified, request) = symbol
            .name_in_file()
            .map(|name| {
                let qualified = format!("{module_name}.{name}");
                (name, qualified, create_import_request(module_name, name))
            })
            .unwrap_or_else(|| {
                let name = module_name.as_str();
                let qualified = name.to_string();
                (name, qualified, ImportRequest::module(name))
            });
        // FIXME: `all_symbols` doesn't account for wildcard imports.
        // Since we're looking at every module, this is probably
        // "fine," but it might mean that we import a symbol from the
        // "wrong" module.
        let import_action = importer.import(request, &members);
        // N.B. We use `add` here because `all_symbols` already
        // takes our query into account.
        completions.force_add(Completion {
            name: ast::name::Name::new(name),
            qualified: Some(ast::name::Name::new(qualified)),
            insert: Some(import_action.symbol_text().into()),
            ty: None,
            kind: symbol.kind().to_completion_kind(),
            module_name: Some(module_name),
            import: import_action.import().cloned(),
            builtin: false,
            // TODO: `is_type_check_only` requires inferring the type of the symbol
            is_type_check_only: false,
            is_definitively_raisable: false,
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
        #[expect(dead_code)]
        attribute: Option<&'t Token>,
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
            CompletionTargetTokens::PossibleObjectDot { object, .. } => {
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

/// A representation of the completion context for a possibly incomplete import
/// statement.
#[derive(Clone, Debug)]
enum ImportStatement<'a> {
    FromImport(FromImport<'a>),
    Import(Import<'a>),
    Incomplete(IncompleteImport),
}

/// A representation of the completion context for a possibly incomplete
/// `from ... import ...` statement.
#[derive(Clone, Debug)]
struct FromImport<'a> {
    ast: &'a ast::StmtImportFrom,
    kind: FromImportKind,
}

/// The kind of completions to offer for a `from import` statement.
///
/// This is either something like `from col<CURSOR>`, where we should
/// offer module completions, or `from collections.<CURSOR>`, where
/// we should offer submodule completions or
/// `from collections import default<CURSOR>` where we should offer
/// submodule/attribute completions.
#[derive(Clone, Debug)]
enum FromImportKind {
    Module,
    Submodule {
        parent: ModuleName,
    },
    Relative {
        parent: ModuleName,
        /// When `true`, an `import` keyword is allowed next.
        /// For example, `from ...<CURSOR>` should offer `import`
        /// but also submodule completions.
        import_keyword_allowed: bool,
    },
    Attribute,
}

/// A representation of the completion context for a possibly incomplete
/// `import ...` statement.
#[derive(Clone, Debug)]
struct Import<'a> {
    #[expect(dead_code)]
    ast: &'a ast::StmtImport,
    kind: ImportKind,
}

/// The kind of completions to offer for an `import` statement.
///
/// This is either something like `import col<CURSOR>`, where we should
/// offer module completions, or `import collections.<CURSOR>`, where
/// we should offer submodule completions.
#[derive(Clone, Debug)]
enum ImportKind {
    Module,
    Submodule { parent: ModuleName },
}

/// Occurs when we detect that an import statement
/// is likely incomplete by virtue of a missing or
/// in-progress `as` or `import` keyword.
#[derive(Clone, Debug)]
enum IncompleteImport {
    As,
    Import,
}

impl<'a> ImportStatement<'a> {
    /// The number of tokens we're willing to consume backwards from
    /// the cursor's position until we give up looking for an import
    /// statement. The state machine below has lots of opportunities
    /// to bail way earlier than this, but if there's, e.g., a long
    /// list of name tokens for something that isn't an import, then we
    /// could end up doing a lot of wasted work here. Probably humans
    /// aren't often working with single import statements over 1,000
    /// tokens long.
    ///
    /// The other thing to consider here is that, by the time we get to
    /// this point, ty has already done some work proportional to the
    /// length of `tokens` anyway. The unit of work we do below is very
    /// small.
    const LIMIT: usize = 1_000;

    /// Attempts to detect an import statement in reverse starting at
    /// the end of `tokens`. That is, `tokens` should correspond to the
    /// sequence of tokens up to the end user's cursor. `typed` should
    /// correspond to the text the user has typed, which is usually,
    /// but not always, the text corresponding to the last token in
    /// `tokens`.
    fn detect(
        db: &'a dyn Db,
        file: File,
        parsed: &'a ParsedModuleRef,
        tokens: &'a [Token],
        typed: Option<&str>,
    ) -> Option<ImportStatement<'a>> {
        use TokenKind as TK;

        // This state machine moves backwards through the token stream,
        // starting at where the user's cursor is and ending when
        // either a `from` token is found, or a token that cannot
        // possibly appear in an import statement at a particular
        // position is found.
        //
        // To understand this state machine, it's recommended to become
        // familiar with the grammar for Python import statements:
        // https://docs.python.org/3/reference/grammar.html

        /// The current state of the parser below.
        #[derive(Clone, Copy, Debug)]
        enum S {
            /// Our initial state.
            Start,
            /// We just saw an `import` token.
            Import,
            /// We just saw a first "name" token. That is,
            /// a name-like token that appears just before
            /// the end user's cursor.
            ///
            /// This isn't just limited to `TokenKind::Name`.
            /// This also includes keywords and things like
            /// "unknown" tokens that can stand in for names
            /// at times.
            FirstName,
            /// A name seen immediately after the first name. This
            /// indicates we have an incomplete import statement like
            /// `import foo a<CURSOR>` or `from foo imp<CURSOR>`. But
            /// we mush on.
            AdjacentName,

            /// A state where we expect to see the start of or
            /// continuation of a list of names following `import`.
            /// In the [grammar], this is either `dotted_as_names`
            /// or `import_from_as_names`.
            ///
            /// [grammar]: https://docs.python.org/3/reference/grammar.html
            NameList,
            /// Occurs after seeing a name-like token at the end
            /// of a name list. This could be an alias, a dotted
            /// name or a non-dotted name.
            NameListNameOrAlias,
            /// Occurs when we've seen an `as` in a list of names.
            As,
            /// Occurs when we see a name-like token after an `as`
            /// keyword.
            AsName,

            /// Occurs when we see a `.` between name-like tokens
            /// after an `as` keyword. This implies we must parse
            /// a `from` statement, since an `as` in a `from` can
            /// never alias a dotted name.
            AsDottedNameDot,
            /// Occurs when we see a name-like token after a
            /// `.name as`.
            AsDottedName,
            /// Occurs when we see a comma right before `a.b as foo`.
            AsDottedNameComma,
            /// Occurs before `, a.b as foo`. In this state, we can
            /// see either a non-dotted alias or a dotted name.
            AsDottedNameOrAlias,
            /// Occurs before `bar, a.b as foo`. In this state, we can
            /// see a `.`, `as`, or `import`.
            AsDottedNameOrAliasName,

            /// Occurs when we've seen a dot right before the cursor
            /// or after the first name-like token. That is, `.name`.
            /// This could be from `import module.name` or `from ..name
            /// import blah`.
            InitialDot,
            /// Occurs when we see `foo.bar<CURSOR>`. When we enter
            /// this state, it means we must be in an `import`
            /// statement, since `from foo.bar` is always invalid.
            InitialDotName,
            /// Occurs when we see `.foo.bar<CURSOR>`. This lets us
            /// continue consuming a dotted name.
            InitialDottedName,

            // When the states below occur, we are locked into
            // recognizing a `from ... import ...` statement.
            /// Occurs when we've seen an ellipsis right before the
            /// cursor or after the first name-like token. That is,
            /// `...name`. This must be from a
            /// `from ...name import blah` statement.
            FromEllipsisName,
            /// A state for consuming `.` and `...` in a `from` import
            /// statement. We enter this after seeing a `.` or a `...`
            /// right after an `import` statement or a `...` right
            /// before the end user's cursor. Either way, we have to
            /// consume only dots at this point until we find a `from`
            /// token.
            FromDots,
            /// Occurs when we've seen an `import` followed by a name-like
            /// token. i.e., `from name import` or `from ...name import`.
            FromDottedName,
            /// Occurs when we've seen an `import` followed by a
            /// name-like token with a dot. i.e., `from .name import`
            /// or `from ..name import`.
            FromDottedNameDot,
            /// A `*` was just seen, which must mean the import is of
            /// the form `from module import *`.
            FromStar,
            /// A left parenthesis was just seen.
            FromLpar,

            // Below are terminal states. Once we reach one
            // of these, the state machine ends.
            /// We just saw a `from` token. We never have any
            /// outgoing transitions from this.
            From,
            /// This is like `import`, but used in a context
            /// where we know we're in an import statement and
            /// specifically *not* a `from ... import ...`
            /// statement.
            ImportFinal,
        }

        let mut state = S::Start;
        // The token immediate before (or at) the cursor.
        let last = tokens.last()?;
        // A token corresponding to `import`, if found.
        let mut import: Option<&Token> = None;
        // A token corresponding to `from`, if found.
        let mut from: Option<&Token> = None;
        // Whether an initial dot was found right before the cursor,
        // or right before the name at the cursor.
        let mut initial_dot = false;
        // An incomplete import statement was found.
        // Usually either `from foo imp<CURSOR>`
        // or `import foo a<CURSOR>`.
        let mut incomplete_as_or_import = false;
        for token in tokens.iter().rev().take(Self::LIMIT) {
            if token.kind().is_trivia() {
                continue;
            }
            state = match (state, token.kind()) {
                // These cases handle our "initial" condition.
                // Basically, this is what detects how to drop us into
                // the full state machine below for parsing any kind of
                // import statement. There are also some cases we try
                // to detect here that indicate the user is probably
                // typing an `import` or `as` token. In effect, we
                // try to pluck off the initial name-like token that
                // represents where the cursor likely is. And then
                // we move on to try and detect the type of import
                // statement that we're dealing with.
                // (S::Start, TK::Newline) => S::Start,
                (S::Start, TK::Star) => S::FromStar,
                (S::Start, TK::Name) if typed.is_none() => S::AdjacentName,
                (S::Start, TK::Name) => S::FirstName,
                (S::Start | S::FirstName | S::AdjacentName, TK::Import) => S::Import,
                (S::Start | S::FirstName | S::AdjacentName, TK::Lpar) => S::FromLpar,
                (S::Start | S::FirstName | S::AdjacentName, TK::Comma) => S::NameList,
                (S::Start | S::FirstName | S::AdjacentName, TK::Dot) => S::InitialDot,
                (S::Start | S::FirstName | S::AdjacentName, TK::Ellipsis) => S::FromEllipsisName,
                (S::Start | S::FirstName, TK::As) => S::As,
                (S::Start | S::AdjacentName, TK::From) => S::From,
                (S::FirstName, TK::From) => S::From,
                (S::FirstName, TK::Name) => S::AdjacentName,

                // This handles the case where we see `.name`. Here,
                // we could be in `from .name`, `from ..name`, `from
                // ...name`, `from foo.name`, `import foo.name`,
                // `import bar, foo.name` and so on.
                (S::InitialDot, TK::Dot | TK::Ellipsis) => S::FromDots,
                (S::InitialDot, TK::Name) => S::InitialDotName,
                (S::InitialDot, TK::From) => S::From,
                (S::InitialDotName, TK::Dot) => S::InitialDottedName,
                (S::InitialDotName, TK::Ellipsis) => S::FromDots,
                (S::InitialDotName, TK::As) => S::AsDottedNameDot,
                (S::InitialDotName, TK::Comma) => S::AsDottedNameOrAlias,
                (S::InitialDotName, TK::Import) => S::ImportFinal,
                (S::InitialDotName, TK::From) => S::From,
                (S::InitialDottedName, TK::Dot | TK::Ellipsis) => S::FromDots,
                (S::InitialDottedName, TK::Name) => S::InitialDotName,
                (S::InitialDottedName, TK::From) => S::From,

                // This state machine parses `dotted_as_names` or
                // `import_from_as_names`. It has a carve out for when
                // it finds a dot, which indicates it must parse only
                // `dotted_as_names`.
                (S::NameList, TK::Name | TK::Unknown) => S::NameListNameOrAlias,
                (S::NameList, TK::Lpar) => S::FromLpar,
                (S::NameListNameOrAlias, TK::As) => S::As,
                (S::NameListNameOrAlias, TK::Comma) => S::NameList,
                (S::NameListNameOrAlias, TK::Import) => S::Import,
                (S::NameListNameOrAlias, TK::Lpar) => S::FromLpar,
                (S::NameListNameOrAlias, TK::Unknown) => S::NameListNameOrAlias,
                // This pops us out of generic name-list parsing
                // and puts us firmly into `dotted_as_names` in
                // the grammar.
                (S::NameListNameOrAlias, TK::Dot) => S::AsDottedNameDot,

                // This identifies aliasing via `as`. The main trick
                // here is that if we see a `.`, then we move to a
                // different set of states since we know we must be in
                // an `import` statement. Without a `.` though, we
                // could be in an `import` or a `from`. For example,
                // `import numpy as np` or
                // `from collections import defaultdict as dd`.
                (S::As, TK::Name) => S::AsName,
                (S::AsName, TK::Dot) => S::AsDottedNameDot,
                (S::AsName, TK::Import) => S::Import,
                (S::AsName, TK::Comma) => S::NameList,

                // This is the mini state machine for handling
                // `dotted_as_names`. We enter it when we see
                // `foo.bar as baz`. We therefore know this must
                // be an `import` statement and not a `from import`
                // statement.
                (S::AsDottedName, TK::Dot) => S::AsDottedNameDot,
                (S::AsDottedName, TK::Comma) => S::AsDottedNameComma,
                (S::AsDottedName, TK::Import) => S::ImportFinal,
                (S::AsDottedNameDot, TK::Name) => S::AsDottedName,
                (S::AsDottedNameComma, TK::Name) => S::AsDottedNameOrAlias,
                (S::AsDottedNameOrAlias, TK::Name) => S::AsDottedNameOrAliasName,
                (S::AsDottedNameOrAlias, TK::Dot) => S::AsDottedNameDot,
                (S::AsDottedNameOrAliasName, TK::Dot | TK::As) => S::AsDottedNameDot,
                (S::AsDottedNameOrAliasName, TK::Import) => S::ImportFinal,

                // A `*` and `(` immediately constrains what we're allowed to see.
                // We can jump right to expecting an `import` keyword.
                (S::FromStar | S::FromLpar, TK::Import) => S::Import,

                // The transitions below handle everything from `from`
                // to `import`. Basically, once we see an `import`
                // token or otherwise know we're parsing the module
                // section of a `from` import statement, we end up in
                // one of the transitions below.
                (S::Import, TK::Dot | TK::Ellipsis) => S::FromDots,
                (S::Import, TK::Name | TK::Unknown) => S::FromDottedName,
                (S::FromDottedName, TK::Dot) => S::FromDottedNameDot,
                (S::FromDottedName, TK::Ellipsis) => S::FromDots,
                (S::FromDottedNameDot, TK::Name) => S::FromDottedName,
                (S::FromDottedNameDot, TK::Dot | TK::Ellipsis) => S::FromDots,
                (S::FromEllipsisName | S::FromDots, TK::Dot | TK::Ellipsis) => S::FromDots,
                (
                    S::FromEllipsisName | S::FromDots | S::FromDottedName | S::FromDottedNameDot,
                    TK::From,
                ) => S::From,

                _ => break,
            };
            // If we transition into a few different special
            // states, we record the token.
            match state {
                S::Import | S::ImportFinal => {
                    import = Some(token);
                }
                S::From => {
                    from = Some(token);
                }
                S::AdjacentName => {
                    // We've seen two adjacent name-like tokens
                    // right before the cursor. At this point,
                    // we continue on to try to recognize a nearly
                    // valid import statement, and to figure out
                    // what kinds of completions we should offer
                    // (if any).
                    incomplete_as_or_import = true;
                }
                S::InitialDot | S::FromEllipsisName => {
                    initial_dot = true;
                }
                _ => {}
            }
        }

        // Now find a possibly dotted name up to where the current
        // cursor is. This could be an item inside a module, a module
        // name, a submodule name or even a relative module. The
        // point is that it is the thing that the end user is trying
        // to complete.
        let source = source_text(db, file);
        let mut to_complete = String::new();
        let end = last.range().end();
        let mut start = end;
        for token in tokens.iter().rev().take(Self::LIMIT) {
            match token.kind() {
                TK::Name | TK::Dot | TK::Ellipsis => {
                    start = token.range().start();
                }
                _ => break,
            }
        }
        to_complete.push_str(&source[TextRange::new(start, end)]);

        // If the typed text corresponds precisely to a keyword,
        // then as a special case, consider it "incomplete" for that
        // keyword. This occurs when the cursor is immediately at the
        // end of `import` or `as`, e.g., `import<CURSOR>`. So we
        // should provide it as a completion so that the end user can
        // confirm it as-is. We special case this because a complete
        // `import` or `as` gets special recognition as a special token
        // kind, and it isn't worth complicating the state machine
        // above to account for this.
        //
        // We also handle the more general "incomplete" cases here too.
        // Basically, `incomplete_as_or_import` is set to `true` when
        // we detect an "adjacent" name in an import statement. Some
        // examples:
        //
        //     from foo <CURSOR>
        //     from foo imp<CURSOR>
        //     from foo import bar <CURSOR>
        //     from foo import bar a<CURSOR>
        //     import foo <CURSOR>
        //     import foo a<CURSOR>
        //
        // Since there is a very limited number of cases, we can
        // suggest `import` when an `import` token isn't present. And
        // `as` when an `import` token *is* present. Notably, `as` can
        // only appear after an `import` keyword!
        if typed == Some("import") || (incomplete_as_or_import && import.is_none()) {
            return Some(ImportStatement::Incomplete(IncompleteImport::Import));
        } else if typed == Some("as") || (incomplete_as_or_import && import.is_some()) {
            return Some(ImportStatement::Incomplete(IncompleteImport::As));
        }
        match (from, import) {
            (None, None) => None,
            (None, Some(import)) => {
                let ast = find_ast_for_import(parsed, import)?;
                // If we found a dot near the cursor, then this
                // must be a request for submodule completions.
                let kind = if initial_dot {
                    let (parent, _) = to_complete.rsplit_once('.')?;
                    let module_name = ModuleName::new(parent)?;
                    ImportKind::Submodule {
                        parent: module_name,
                    }
                } else {
                    ImportKind::Module
                };
                Some(ImportStatement::Import(Import { ast, kind }))
            }
            (Some(from), import) => {
                let ast = find_ast_for_from_import(parsed, from)?;
                // If we saw an `import` keyword, then that means the
                // cursor must be *after* the `import`. And thus we
                // only ever need to offer completions for importable
                // elements from the module being imported.
                let kind = if import.is_some() {
                    FromImportKind::Attribute
                } else if !initial_dot {
                    FromImportKind::Module
                } else {
                    let to_complete_without_leading_dots = to_complete.trim_start_matches('.');

                    // When there aren't any leading dots to trim, then we
                    // have a regular absolute import. Otherwise, it's relative.
                    if to_complete == to_complete_without_leading_dots {
                        let (parent, _) = to_complete.rsplit_once('.')?;
                        let parent = ModuleName::new(parent)?;
                        FromImportKind::Submodule { parent }
                    } else {
                        let all_dots = to_complete.chars().all(|c| c == '.');
                        // We should suggest `import` in `from ...<CURSOR>`
                        // and `from ...imp<CURSOR>`.
                        let import_keyword_allowed =
                            all_dots || !to_complete_without_leading_dots.contains('.');
                        let parent = if all_dots {
                            ModuleName::from_import_statement(db, file, ast).ok()?
                        } else {
                            // We know `to_complete` is not all dots.
                            // But that it starts with a dot.
                            // So we must have one of `..foo`, `..foo.`
                            // or `..foo.bar`. We drop the leading dots,
                            // since those are captured by `ast.level`.
                            // From there, we can treat it like a normal
                            // module name. We want to list submodule
                            // completions, so we pop off the last element
                            // if there are any remaining dots.
                            let parent = to_complete_without_leading_dots
                                .rsplit_once('.')
                                .map(|(parent, _)| parent);
                            ModuleName::from_identifier_parts(db, file, parent, ast.level).ok()?
                        };
                        FromImportKind::Relative {
                            parent,
                            import_keyword_allowed,
                        }
                    }
                };
                Some(ImportStatement::FromImport(FromImport { ast, kind }))
            }
        }
    }

    /// Add completions, if any and if appropriate, based on the detected
    /// import statement.
    fn add_completions<'db>(
        &self,
        db: &'db dyn Db,
        file: File,
        completions: &mut Completions<'db>,
    ) {
        let model = SemanticModel::new(db, file);
        match *self {
            ImportStatement::Import(Import { ref kind, .. }) => match *kind {
                ImportKind::Module => {
                    completions.extend(model.import_completions());
                }
                ImportKind::Submodule { ref parent } => {
                    completions.extend(model.import_submodule_completions_for_name(parent));
                }
            },
            ImportStatement::FromImport(FromImport { ast, ref kind }) => match *kind {
                FromImportKind::Module => {
                    completions.extend(model.import_completions());
                }
                FromImportKind::Submodule { ref parent } => {
                    completions.extend(model.import_submodule_completions_for_name(parent));
                }
                FromImportKind::Relative {
                    ref parent,
                    import_keyword_allowed,
                } => {
                    completions.extend(model.import_submodule_completions_for_name(parent));
                    if import_keyword_allowed {
                        completions.try_add(Completion::keyword("import"));
                    }
                }
                FromImportKind::Attribute => {
                    completions.extend(model.from_import_completions(ast));
                }
            },
            ImportStatement::Incomplete(IncompleteImport::As) => {
                completions.try_add(Completion::keyword("as"));
            }
            ImportStatement::Incomplete(IncompleteImport::Import) => {
                completions.try_add(Completion::keyword("import"));
            }
        }
    }
}

/// Finds the AST node, if available, corresponding to the given `from`
/// token.
///
/// This always returns `None` when the `token` is not a `from` token.
fn find_ast_for_from_import<'p>(
    parsed: &'p ParsedModuleRef,
    token: &Token,
) -> Option<&'p ast::StmtImportFrom> {
    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find_first(|node| node.is_stmt_import_from())
        .ok()?;
    let ast::AnyNodeRef::StmtImportFrom(from_import) = covering_node.node() else {
        return None;
    };
    Some(from_import)
}

/// Finds the AST node, if available, corresponding to the given `import`
/// token.
///
/// This always returns `None` when the `token` is not a `import` token.
fn find_ast_for_import<'p>(
    parsed: &'p ParsedModuleRef,
    token: &Token,
) -> Option<&'p ast::StmtImport> {
    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find_first(|node| node.is_stmt_import())
        .ok()?;
    let ast::AnyNodeRef::StmtImport(import) = covering_node.node() else {
        return None;
    };
    Some(import)
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

/// Looks for the text typed immediately before the cursor offset
/// given.
///
/// If there isn't any typed text or it could not otherwise be found,
/// then `None` is returned.
///
/// When `Some` is returned, the string is guaranteed to be non-empty.
fn find_typed_text(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    offset: TextSize,
) -> Option<String> {
    let source = source_text(db, file);
    let tokens = tokens_start_before(parsed.tokens(), offset);
    let last = tokens.last()?;
    // It's odd to include `TokenKind::Import` here, but it
    // indicates that the user has typed `import`. This is
    // useful to know in some contexts. And this applies also
    // to the other keywords.
    if !matches!(last.kind(), TokenKind::Name) && !last.kind().is_keyword() {
        return None;
    }
    // This one's weird, but if the cursor is beyond
    // what is in the closest `Name` token, then it's
    // likely we can't infer anything about what has
    // been typed. This likely means there is whitespace
    // or something that isn't represented in the token
    // stream. So just give up.
    if last.end() < offset || last.range().is_empty() {
        return None;
    }
    let range = TextRange::new(last.start(), offset);
    Some(source[range].to_string())
}

/// Whether the last token is in a place where we should not provide completions.
fn is_in_no_completions_place(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    offset: TextSize,
    tokens: &[Token],
    typed: Option<&str>,
) -> bool {
    is_in_comment(tokens)
        || is_in_string(tokens)
        || is_in_definition_place(db, file, parsed, offset, tokens, typed)
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

/// Returns true when the tokens indicate that the definition of a new
/// name is being introduced at the end.
fn is_in_definition_place(
    db: &dyn Db,
    file: File,
    parsed: &ParsedModuleRef,
    offset: TextSize,
    tokens: &[Token],
    typed: Option<&str>,
) -> bool {
    fn is_definition_token(token: &Token) -> bool {
        matches!(
            token.kind(),
            TokenKind::Def | TokenKind::Class | TokenKind::Type | TokenKind::As | TokenKind::For
        )
    }

    let is_definition_keyword = |token: &Token| {
        if is_definition_token(token) {
            true
        } else if token.kind() == TokenKind::Name {
            let source = source_text(db, file);
            &source[token.range()] == "type"
        } else {
            false
        }
    };
    if match tokens {
        [.., penultimate, _] if typed.is_some() => is_definition_keyword(penultimate),
        [.., last] if typed.is_none() => is_definition_keyword(last),
        _ => false,
    } {
        return true;
    }
    // Analyze the AST if token matching is insufficient
    // to determine if we're inside a name definition.
    is_in_variable_binding(parsed, offset, typed)
}

/// Returns true when the cursor sits on a binding statement.
/// E.g. naming a parameter, type parameter, or `for` <name>).
fn is_in_variable_binding(parsed: &ParsedModuleRef, offset: TextSize, typed: Option<&str>) -> bool {
    let range = if let Some(typed) = typed {
        let start = offset.saturating_sub(typed.text_len());
        TextRange::new(start, offset)
    } else {
        TextRange::empty(offset)
    };

    let covering = covering_node(parsed.syntax().into(), range);
    covering.ancestors().any(|node| match node {
        ast::AnyNodeRef::Parameter(param) => param.name.range.contains_range(range),
        ast::AnyNodeRef::TypeParamTypeVar(type_param) => {
            type_param.name.range.contains_range(range)
        }
        ast::AnyNodeRef::StmtFor(stmt_for) => stmt_for.target.range().contains_range(range),
        // The AST does not produce `ast::AnyNodeRef::Parameter` nodes for keywords
        // or otherwise invalid syntax. Rather they are captured in a
        // `ast::AnyNodeRef::Parameters` node as "empty space". To ensure
        // we still suppress suggestions even when the syntax is technically
        // invalid we extract the token under the cursor and check if it makes
        // up that "empty space" inside the Parameters Node. If it does, we know
        // that we are still binding variables, just that the current state is
        // syntatically invalid. Hence we suppress autocomplete suggestons
        // also in those cases.
        ast::AnyNodeRef::Parameters(params) => {
            if !params.range.contains_range(range) {
                return false;
            }
            params
                .iter()
                .map(|param| param.range())
                .all(|r| !r.contains_range(range))
        }
        _ => false,
    })
}

/// Returns true when the cursor is after a `raise` keyword.
fn is_raising_exception(tokens: &[Token]) -> bool {
    /// The maximum number of tokens we're willing to
    /// look-behind to find a `raise` keyword.
    const LIMIT: usize = 10;

    // This only looks for things like `raise foo.bar.baz.qu<CURSOR>`.
    // Technically, any kind of expression is allowed after `raise`.
    // But we may not always want to treat it specially. So we're
    // rather conservative about what we consider "raising an
    // exception" to be for the purposes of completions. The failure
    // mode here is that we may wind up suggesting things that
    // shouldn't be raised. The benefit is that when this heuristic
    // does work, we won't suggest things that shouldn't be raised.
    for token in tokens.iter().rev().take(LIMIT) {
        match token.kind() {
            TokenKind::Name | TokenKind::Dot => continue,
            TokenKind::Raise => return true,
            _ => return false,
        }
    }
    false
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
    fn key<'a>(completion: &'a Completion) -> (bool, bool, bool, bool, NameKind, bool, &'a Name) {
        (
            // This is only true when we are both in a `raise` context
            // *and* we know this suggestion is definitively usable
            // in a `raise` context. So we should sort these before
            // anything else.
            !completion.is_definitively_raisable,
            // When `None`, a completion is for something in the
            // current module, which we should generally prefer over
            // something from outside the module.
            completion.module_name.is_some(),
            // At time of writing (2025-11-11), keyword completions
            // are classified as builtins, which makes them sort after
            // everything else. But we probably want keyword completions
            // to sort *before* anything else since they are so common.
            // Moreover, it seems VS Code forcefully does this sorting.
            // By doing it ourselves, we make our natural sorting match
            // VS Code's, and thus our completion evaluation framework
            // should be more representative of real world conditions.
            completion.kind != Some(CompletionKind::Keyword),
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
    use ruff_python_ast::token::{TokenKind, Tokens};
    use ruff_python_parser::{Mode, ParseOptions};
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
            test.skip_keywords().skip_builtins().build().snapshot(),
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
    fn keywords() {
        let test = completion_test_builder(
            "\
<CURSOR>
",
        );

        assert_snapshot!(
            test.skip_builtins().build().snapshot(),
            @r"
        and
        as
        assert
        async
        await
        break
        case
        class
        continue
        def
        del
        elif
        else
        except
        finally
        for
        from
        global
        if
        import
        in
        is
        lambda
        match
        nonlocal
        not
        or
        pass
        raise
        return
        try
        while
        with
        yield
        ",
        );
    }

    #[test]
    fn inside_token() {
        let test = completion_test_builder(
            "\
foo_bar_baz = 1
x = foo<CURSOR>bad
",
        );

        assert_snapshot!(
            test.skip_builtins().build().snapshot(),
            @"foo_bar_baz",
        );
    }

    #[test]
    fn type_keyword_dedup() {
        let test = completion_test_builder(
            "\
type<CURSOR>
",
        );

        assert_snapshot!(
            test.type_signatures().build().snapshot(),
            @r"
        TypeError :: <class 'TypeError'>
        type :: <class 'type'>
        ",
        );
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"re");
    }

    #[test]
    fn imports2() {
        let builder = completion_test_builder(
            "\
from os import path

<CURSOR>
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"path");
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix1() {
        let builder = completion_test_builder(
            "\
(lambda foo: (1 + f<CURSOR> + 2))(2)
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix2() {
        let builder = completion_test_builder(
            "\
(lambda foo: f<CURSOR> + 1)(2)
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix3() {
        let builder = completion_test_builder(
            "\
(lambda foo: (f<CURSOR> + 1))(2)
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_prefix4() {
        let builder = completion_test_builder(
            "\
(lambda foo: 1 + f<CURSOR>)(2)
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
    }

    #[test]
    fn lambda_blank1() {
        let builder = completion_test_builder(
            "\
(lambda foo: 1 + <CURSOR> + 2)(2)
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"bar");
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r###"
        meta_attr :: int
        mro :: bound method <class 'C'>.mro() -> list[type]
        __annotate__ :: (() -> dict[str, Any]) | None
        __annotations__ :: dict[str, Any]
        __base__ :: type | None
        __bases__ :: tuple[type, ...]
        __basicsize__ :: int
        __call__ :: bound method <class 'C'>.__call__(...) -> Any
        __class__ :: <class 'Meta'>
        __delattr__ :: def __delattr__(self, name: str, /) -> None
        __dict__ :: dict[str, Any]
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
        "###);
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
                assert_snapshot!(
                    builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r###"
        mro :: bound method <class 'Quux'>.mro() -> list[type]
        some_attribute :: int
        some_class_method :: bound method <class 'Quux'>.some_class_method() -> int
        some_method :: def some_method(self) -> int
        some_property :: property
        some_static_method :: def some_static_method(self) -> int
        __annotate__ :: (() -> dict[str, Any]) | None
        __annotations__ :: dict[str, Any]
        __base__ :: type | None
        __bases__ :: tuple[type, ...]
        __basicsize__ :: int
        __call__ :: bound method <class 'Quux'>.__call__(...) -> Any
        __class__ :: <class 'type'>
        __delattr__ :: def __delattr__(self, name: str, /) -> None
        __dict__ :: dict[str, Any]
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
        "###);
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
                assert_snapshot!(
                    builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r"
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
                __dict__ :: dict[str, Any]
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
                __members__ :: MappingProxyType[str, Answer]
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

    #[test]
    fn namedtuple_methods() {
        let builder = completion_test_builder(
            "\
from typing import NamedTuple

class Quux(NamedTuple):
    x: int
    y: str

quux = Quux()
quux.<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().type_signatures().build().snapshot(), @r"
        count :: bound method Quux.count(value: Any, /) -> int
        index :: bound method Quux.index(value: Any, start: SupportsIndex = Literal[0], stop: SupportsIndex = int, /) -> int
        x :: int
        y :: str
        __add__ :: Overload[(value: tuple[int | str, ...], /) -> tuple[int | str, ...], (value: tuple[_T@__add__, ...], /) -> tuple[int | str | _T@__add__, ...]]
        __annotations__ :: dict[str, Any]
        __class__ :: type[Quux]
        __class_getitem__ :: bound method type[Quux].__class_getitem__(item: Any, /) -> GenericAlias
        __contains__ :: bound method Quux.__contains__(key: object, /) -> bool
        __delattr__ :: bound method Quux.__delattr__(name: str, /) -> None
        __dict__ :: dict[str, Any]
        __dir__ :: bound method Quux.__dir__() -> Iterable[str]
        __doc__ :: str | None
        __eq__ :: bound method Quux.__eq__(value: object, /) -> bool
        __format__ :: bound method Quux.__format__(format_spec: str, /) -> str
        __ge__ :: bound method Quux.__ge__(value: tuple[int | str, ...], /) -> bool
        __getattribute__ :: bound method Quux.__getattribute__(name: str, /) -> Any
        __getitem__ :: Overload[(index: Literal[-2, 0], /) -> int, (index: Literal[-1, 1], /) -> str, (index: SupportsIndex, /) -> int | str, (index: slice[Any, Any, Any], /) -> tuple[int | str, ...]]
        __getstate__ :: bound method Quux.__getstate__() -> object
        __gt__ :: bound method Quux.__gt__(value: tuple[int | str, ...], /) -> bool
        __hash__ :: bound method Quux.__hash__() -> int
        __init__ :: bound method Quux.__init__() -> None
        __init_subclass__ :: bound method type[Quux].__init_subclass__() -> None
        __iter__ :: bound method Quux.__iter__() -> Iterator[int | str]
        __le__ :: bound method Quux.__le__(value: tuple[int | str, ...], /) -> bool
        __len__ :: () -> Literal[2]
        __lt__ :: bound method Quux.__lt__(value: tuple[int | str, ...], /) -> bool
        __module__ :: str
        __mul__ :: bound method Quux.__mul__(value: SupportsIndex, /) -> tuple[int | str, ...]
        __ne__ :: bound method Quux.__ne__(value: object, /) -> bool
        __new__ :: (x: int, y: str) -> None
        __orig_bases__ :: tuple[Any, ...]
        __reduce__ :: bound method Quux.__reduce__() -> str | tuple[Any, ...]
        __reduce_ex__ :: bound method Quux.__reduce_ex__(protocol: SupportsIndex, /) -> str | tuple[Any, ...]
        __replace__ :: bound method NamedTupleFallback.__replace__(**kwargs: Any) -> NamedTupleFallback
        __repr__ :: bound method Quux.__repr__() -> str
        __reversed__ :: bound method Quux.__reversed__() -> Iterator[int | str]
        __rmul__ :: bound method Quux.__rmul__(value: SupportsIndex, /) -> tuple[int | str, ...]
        __setattr__ :: bound method Quux.__setattr__(name: str, value: Any, /) -> None
        __sizeof__ :: bound method Quux.__sizeof__() -> int
        __str__ :: bound method Quux.__str__() -> str
        __subclasshook__ :: bound method type[Quux].__subclasshook__(subclass: type, /) -> bool
        _asdict :: bound method NamedTupleFallback._asdict() -> dict[str, Any]
        _field_defaults :: dict[str, Any]
        _fields :: tuple[str, ...]
        _make :: bound method type[NamedTupleFallback]._make(iterable: Iterable[Any]) -> NamedTupleFallback
        _replace :: bound method NamedTupleFallback._replace(**kwargs: Any) -> NamedTupleFallback
        ");
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"foo");
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @"C");
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
        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"classy_variable_name",
        );
    }

    #[test]
    fn identifier_keyword_clash2() {
        let builder = completion_test_builder(
            "\
some_symbol = 1

print(f\"{some<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"some_symbol",
        );
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn ellipsis2() {
        let builder = completion_test_builder(
            "\
....<CURSOR>
",
        );

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_function_identifier2() {
        let builder = completion_test_builder(
            "\
def m<CURSOR>(): pass
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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

        assert_snapshot!(builder.skip_keywords().skip_builtins().build().snapshot(), @r"m");
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_class_identifier1() {
        let builder = completion_test_builder(
            "\
class M<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_type_alias1() {
        let builder = completion_test_builder(
            "\
Fo<CURSOR> = float
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"Fo",
        );
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
        assert!(
            !builder
                .skip_keywords()
                .skip_builtins()
                .build()
                .completions()
                .is_empty()
        );
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
        //
        // ... some time passes ...
        //
        // Actually, this shouldn't offer any completions since
        // the context here is introducing a new name.
        assert!(
            builder
                .skip_keywords()
                .skip_builtins()
                .build()
                .completions()
                .is_empty()
        );
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
        assert!(
            !builder
                .skip_keywords()
                .skip_builtins()
                .build()
                .completions()
                .is_empty()
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_from_import2() {
        let builder = completion_test_builder(
            "\
from foo import wa<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    // Ref: https://github.com/astral-sh/ty/issues/572
    #[test]
    fn scope_id_missing_from_import3() {
        let builder = completion_test_builder(
            "\
from foo import wat as ba<CURSOR>
",
        );

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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
            builder.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found after filtering out completions>",
        );
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

        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @r"<No completions found>",
        );
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
        assert_snapshot!(
            builder.skip_keywords().skip_builtins().build().snapshot(),
            @r"<No completions found>",
        );
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
    fn import_multiple_betwixt() {
        let builder = completion_test_builder(
            "\
import re, c<CURSOR>, sys
",
        );
        builder.build().contains("collections");
    }

    #[test]
    fn import_multiple_end1() {
        let builder = completion_test_builder(
            "\
import collections.abc, unico<CURSOR>
",
        );
        builder.build().contains("unicodedata");
    }

    #[test]
    fn import_multiple_end2() {
        let builder = completion_test_builder(
            "\
import collections.abc, urllib.parse, bu<CURSOR>
",
        );
        builder.build().contains("builtins");
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
        Kadabra :: Literal[1] :: <no import required>
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
            test.skip_keywords().skip_builtins().build().snapshot(),
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

        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"zqzq<CURSOR>\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"\"\"Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(\"\"\"Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('''Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_string_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print('''zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print('''Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"zqzq<CURSOR>\")
        ",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"{Foo} and Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"zqzq<CURSOR>
        ",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'{Foo} and Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"\"\"{Foo} and Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f\"\"\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'''{Foo} and Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_fstring_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(f'''zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(f'''{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"zqzq<CURSOR>\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"{Foo} and Foo.zqzq<CURSOR>\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_incomplete_double_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'{Foo} and Foo.zqzq<CURSOR>')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_incomplete_single_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"\"\"zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"\"\"{Foo} and Foo.zqzq<CURSOR>\"\"\")
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_incomplete_double_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t\"\"\"zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t\"\"\"{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'''zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'''{Foo} and Foo.zqzq<CURSOR>''')
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_tstring_incomplete_single_triple_quote() {
        let test = completion_test_builder(
            "\
zqzqzq = 1
print(t'''zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );

        let test = completion_test_builder(
            "\
class Foo:
    zqzqzq = 1
print(t'''{Foo} and Foo.zqzq<CURSOR>
",
        );
        assert_snapshot!(
            test.skip_keywords().skip_builtins().build().snapshot(),
            @"<No completions found>",
        );
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
        assert!(builder.build().completions().is_empty());
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
        assert!(builder.build().completions().is_empty());
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
        assert!(builder.build().completions().is_empty());
    }

    #[test]
    fn no_completions_in_import_alias() {
        let builder = completion_test_builder(
            "\
foo = 1
import collections as f<CURSOR>
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_from_import_alias() {
        let builder = completion_test_builder(
            "\
foo = 1
from collections import defaultdict as f<CURSOR>
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn import_missing_alias_suggests_as_with_leading_char() {
        let builder = completion_test_builder(
            "\
import collections a<CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn import_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
import collections <CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn import_dotted_module_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
import collections.abc a<CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn import_multiple_modules_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
import collections.abc as c, typing a<CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn from_import_missing_alias_suggests_as_with_leading_char() {
        let builder = completion_test_builder(
            "\
from collections.abc import Mapping a<CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn from_import_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
from collections import defaultdict <CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn from_import_parenthesized_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
from typing import (
    NamedTuple a<CURSOR>
)
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn from_relative_import_missing_alias_suggests_as() {
        let builder = completion_test_builder(
            "\
from ...foo import bar a<CURSOR>
    ",
        );
        assert_snapshot!(builder.build().snapshot(), @"as");
    }

    #[test]
    fn no_completions_in_with_alias() {
        let builder = completion_test_builder(
            "\
foo = 1
with open('bar') as f<CURSOR>
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_except_alias() {
        let builder = completion_test_builder(
            "\
foo = 1
try:
    [][0]
except IndexError as f<CURSOR>
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_match_alias() {
        let builder = completion_test_builder(
            "\
foo = 1
status = 400
match status:
    case 400 as f<CURSOR>:
        return 'Bad request'
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );

        // Also check that completions are suppressed
        // when nothing has been typed.
        let builder = completion_test_builder(
            "\
foo = 1
status = 400
match status:
    case 400 as <CURSOR>:
        return 'Bad request'
    ",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_empty_for_variable_binding() {
        let builder = completion_test_builder(
            "\
for <CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_for_variable_binding() {
        let builder = completion_test_builder(
            "\
for foo<CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_for_tuple_variable_binding() {
        let builder = completion_test_builder(
            "\
for foo, bar<CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_function_param() {
        let builder = completion_test_builder(
            "\
def foo(p<CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_function_param_keyword() {
        let builder = completion_test_builder(
            "\
def foo(in<CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_function_param_multi_keyword() {
        let builder = completion_test_builder(
            "\
def foo(param, in<CURSOR>
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_function_param_multi_keyword_middle() {
        let builder = completion_test_builder(
            "\
def foo(param, in<CURSOR>, param_two
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn no_completions_in_function_type_param() {
        let builder = completion_test_builder(
            "\
def foo[T<CURSOR>]
",
        );
        assert_snapshot!(
            builder.build().snapshot(),
            @"<No completions found>",
        );
    }

    #[test]
    fn completions_in_function_type_param_bound() {
        completion_test_builder(
            "\
def foo[T: s<CURSOR>]
",
        )
        .build()
        .contains("str");
    }

    #[test]
    fn completions_in_function_param_type_annotation() {
        // Ensure that completions are no longer
        // suppressed when have left the name
        // definition block.
        completion_test_builder(
            "\
def foo(param: s<CURSOR>)
",
        )
        .build()
        .contains("str");
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
        long_nameb :: Literal[1] :: <no import required>
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

    #[test]
    fn from_import_i_suggests_import() {
        let builder = completion_test_builder("from typing i<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_import_suggests_import() {
        let builder = completion_test_builder("from typing import<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_importt_suggests_nothing() {
        let builder = completion_test_builder("from typing importt<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn from_import_space_suggests_import() {
        let builder = completion_test_builder("from typing <CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_no_space_not_suggests_import() {
        let builder = completion_test_builder("from typing<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"typing");
    }

    #[test]
    fn from_import_two_imports_suggests_import() {
        let builder = completion_test_builder(
            "from collections.abc import Sequence
            from typing i<CURSOR>",
        );
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_random_name_suggests_nothing() {
        let builder = completion_test_builder("from typing aa<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"<No completions found>");
    }

    #[test]
    fn from_import_dotted_name_suggests_import() {
        let builder = completion_test_builder("from collections.abc i<CURSOR>");
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_relative_import_suggests_import() {
        let builder = CursorTest::builder()
            .source("main.py", "from .foo i<CURSOR>")
            .source("foo.py", "")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_dotted_name_relative_import_suggests_import() {
        let builder = CursorTest::builder()
            .source("main.py", "from .foo.bar i<CURSOR>")
            .source("foo/bar.py", "")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_nested_dotted_name_relative_import_suggests_import() {
        let builder = CursorTest::builder()
            .source("src/main.py", "from ..foo i<CURSOR>")
            .source("foo.py", "")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_nested_very_dotted_name_relative_import_suggests_import() {
        let builder = CursorTest::builder()
            // N.B. the `...` tokenizes as `TokenKind::Ellipsis`
            .source("src/main.py", "from ...foo i<CURSOR>")
            .source("foo.py", "")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @"import");
    }

    #[test]
    fn from_import_only_dot() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source(
                "package/sub1/sub2/bar.py",
                "
import_zqzqzq = 1
from .<CURSOR>
",
            )
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        ");
    }

    #[test]
    fn from_import_only_dot_incomplete() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source(
                "package/sub1/sub2/bar.py",
                "
import_zqzqzq = 1
from .imp<CURSOR>
",
            )
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        ");
    }

    #[test]
    fn from_import_incomplete() {
        let builder = completion_test_builder(
            "from collections.abc i

             ZQZQZQ = 1
             ZQ<CURSOR>",
        );
        assert_snapshot!(builder.build().snapshot(), @"ZQZQZQ");
    }

    #[test]
    fn relative_import_module_after_dots1() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/sub1/sub2/bar.py", "from ...<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        foo
        ");
    }

    #[test]
    fn relative_import_module_after_dots2() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo/__init__.py", "")
            .source("package/foo/bar.py", "")
            .source("package/foo/baz.py", "")
            .source("package/sub1/sub2/bar.py", "from ...foo.<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        bar
        baz
        ");
    }

    #[test]
    fn relative_import_module_after_dots3() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/sub1/sub2/bar.py", "from.<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        ");
    }

    #[test]
    fn relative_import_module_after_dots4() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/sub1/bar.py", "from ..<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        foo
        ");
    }

    #[test]
    fn relative_import_module_after_typing1() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/sub1/sub2/bar.py", "from ...fo<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @"foo");
    }

    #[test]
    fn relative_import_module_after_typing2() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo/__init__.py", "")
            .source("package/foo/bar.py", "")
            .source("package/foo/baz.py", "")
            .source("package/sub1/sub2/bar.py", "from ...foo.ba<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        bar
        baz
        ");
    }

    #[test]
    fn relative_import_module_after_typing3() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "")
            .source("package/imposition.py", "")
            .source("package/sub1/sub2/bar.py", "from ...im<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        import
        imposition
        ");
    }

    #[test]
    fn relative_import_module_after_typing4() {
        let builder = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/sub1/__init__.py", "")
            .source("package/sub1/foo.py", "")
            .source("package/sub1/imposition.py", "")
            .source("package/sub1/bar.py", "from ..sub1.<CURSOR>")
            .completion_test_builder();
        assert_snapshot!(builder.build().snapshot(), @r"
        bar
        foo
        imposition
        ");
    }

    #[test]
    fn typing_extensions_excluded_from_import() {
        let builder = completion_test_builder("from typing<CURSOR>").module_names();
        assert_snapshot!(builder.build().snapshot(), @"typing :: <no import required>");
    }

    #[test]
    fn typing_extensions_excluded_from_auto_import() {
        let builder = completion_test_builder("deprecated<CURSOR>")
            .auto_import()
            .module_names();
        assert_snapshot!(builder.build().snapshot(), @"deprecated :: warnings");
    }

    #[test]
    fn typing_extensions_included_from_import() {
        let builder = CursorTest::builder()
            .source("typing_extensions.py", "deprecated = 1")
            .source("foo.py", "from typing<CURSOR>")
            .completion_test_builder()
            .module_names();
        assert_snapshot!(builder.build().snapshot(), @r"
        typing :: <no import required>
        typing_extensions :: <no import required>
        ");
    }

    #[test]
    fn typing_extensions_included_from_auto_import() {
        let builder = CursorTest::builder()
            .source("typing_extensions.py", "deprecated = 1")
            .source("foo.py", "deprecated<CURSOR>")
            .completion_test_builder()
            .auto_import()
            .module_names();
        assert_snapshot!(builder.build().snapshot(), @r"
        deprecated :: typing_extensions
        deprecated :: warnings
        ");
    }

    #[test]
    fn typing_extensions_included_from_import_in_stub() {
        let builder = CursorTest::builder()
            .source("foo.pyi", "from typing<CURSOR>")
            .completion_test_builder()
            .module_names();
        assert_snapshot!(builder.build().snapshot(), @r"
        typing :: <no import required>
        typing_extensions :: <no import required>
        ");
    }

    #[test]
    fn typing_extensions_included_from_auto_import_in_stub() {
        let builder = CursorTest::builder()
            .source("foo.pyi", "deprecated<CURSOR>")
            .completion_test_builder()
            .auto_import()
            .module_names();
        assert_snapshot!(builder.build().snapshot(), @r"
        deprecated :: typing_extensions
        deprecated :: warnings
        ");
    }

    #[test]
    fn reexport_simple_import_noauto() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
import foo
foo.ZQ<CURSOR>
"#,
            )
            .source("foo.py", r#"from bar import ZQZQ"#)
            .source("bar.py", r#"ZQZQ = 1"#)
            .completion_test_builder()
            .module_names()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @"ZQZQ :: <no import required>");
    }

    #[test]
    fn reexport_simple_import_auto() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
ZQ<CURSOR>
"#,
            )
            .source("foo.py", r#"from bar import ZQZQ"#)
            .source("bar.py", r#"ZQZQ = 1"#)
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        // We're specifically looking for `ZQZQ` in `bar`
        // here but *not* in `foo`. Namely, in `foo`,
        // `ZQZQ` is a "regular" import that is not by
        // convention considered a re-export.
        assert_snapshot!(snapshot, @"ZQZQ :: bar");
    }

    #[test]
    fn reexport_redundant_convention_import_noauto() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
import foo
foo.ZQ<CURSOR>
"#,
            )
            .source("foo.py", r#"from bar import ZQZQ as ZQZQ"#)
            .source("bar.py", r#"ZQZQ = 1"#)
            .completion_test_builder()
            .module_names()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @"ZQZQ :: <no import required>");
    }

    #[test]
    fn reexport_redundant_convention_import_auto() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
ZQ<CURSOR>
"#,
            )
            .source("foo.py", r#"from bar import ZQZQ as ZQZQ"#)
            .source("bar.py", r#"ZQZQ = 1"#)
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @r"
        ZQZQ :: bar
        ZQZQ :: foo
        ");
    }

    #[test]
    fn auto_import_respects_all() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
ZQ<CURSOR>
"#,
            )
            .source(
                "bar.py",
                r#"
                ZQZQ1 = 1
                ZQZQ2 = 1
                __all__ = ['ZQZQ1']
            "#,
            )
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        // We specifically do not want `ZQZQ2` here, since
        // it is not part of `__all__`.
        assert_snapshot!(snapshot, @r"
        ZQZQ1 :: bar
        ");
    }

    // This test confirms current behavior (as of 2025-12-04), but
    // it's not consistent with auto-import. That is, it doesn't
    // strictly respect `__all__` on `bar`, but perhaps it should.
    //
    // See: https://github.com/astral-sh/ty/issues/1757
    #[test]
    fn object_attr_ignores_all() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
import bar
bar.ZQ<CURSOR>
"#,
            )
            .source(
                "bar.py",
                r#"
                ZQZQ1 = 1
                ZQZQ2 = 1
                __all__ = ['ZQZQ1']
            "#,
            )
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        // We specifically do not want `ZQZQ2` here, since
        // it is not part of `__all__`.
        assert_snapshot!(snapshot, @r"
        ZQZQ1 :: <no import required>
        ZQZQ2 :: <no import required>
        ");
    }

    #[test]
    fn auto_import_ignores_modules_with_leading_underscore() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
Quitter<CURSOR>
"#,
            )
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        // There is a `Quitter` in `_sitebuiltins` in the standard
        // library. But this is skipped by auto-import because it's
        // 1) not first party and 2) starts with an `_`.
        assert_snapshot!(snapshot, @"<No completions found>");
    }

    #[test]
    fn auto_import_includes_modules_with_leading_underscore_in_first_party() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
ZQ<CURSOR>
"#,
            )
            .source(
                "bar.py",
                r#"
                ZQZQ1 = 1
            "#,
            )
            .source(
                "_foo.py",
                r#"
                ZQZQ1 = 1
            "#,
            )
            .completion_test_builder()
            .auto_import()
            .module_names()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @r"
        ZQZQ1 :: _foo
        ZQZQ1 :: bar
        ");
    }

    #[test]
    fn auto_import_includes_stdlib_modules_as_suggestions() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
multiprocess<CURSOR>
"#,
            )
            .completion_test_builder()
            .auto_import()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @r"
        multiprocessing
        multiprocessing.connection
        multiprocessing.context
        multiprocessing.dummy
        multiprocessing.dummy.connection
        multiprocessing.forkserver
        multiprocessing.heap
        multiprocessing.managers
        multiprocessing.pool
        multiprocessing.popen_fork
        multiprocessing.popen_forkserver
        multiprocessing.popen_spawn_posix
        multiprocessing.popen_spawn_win32
        multiprocessing.process
        multiprocessing.queues
        multiprocessing.reduction
        multiprocessing.resource_sharer
        multiprocessing.resource_tracker
        multiprocessing.shared_memory
        multiprocessing.sharedctypes
        multiprocessing.spawn
        multiprocessing.synchronize
        multiprocessing.util
        ");
    }

    #[test]
    fn auto_import_includes_first_party_modules_as_suggestions() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
zqzqzq<CURSOR>
"#,
            )
            .source("zqzqzqzqzq.py", "")
            .completion_test_builder()
            .auto_import()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @"zqzqzqzqzq");
    }

    #[test]
    fn auto_import_includes_sub_modules_as_suggestions() {
        let snapshot = CursorTest::builder()
            .source(
                "main.py",
                r#"
collabc<CURSOR>
"#,
            )
            .completion_test_builder()
            .auto_import()
            .build()
            .snapshot();
        assert_snapshot!(snapshot, @"collections.abc");
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
    #[expect(clippy::struct_excessive_bools)] // free the bools!
    struct CompletionTestBuilder {
        cursor_test: CursorTest,
        settings: CompletionSettings,
        skip_builtins: bool,
        skip_keywords: bool,
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
                .filter(|c| !self.skip_keywords || c.kind != Some(CompletionKind::Keyword))
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

        /// When set, keywords from completions are skipped. This
        /// is useful in tests to reduce noise for scope based
        /// completions.
        ///
        /// Not enabled by default.
        ///
        /// Note that, at time of writing (2025-11-11), keywords are
        /// *also* considered builtins. So `skip_builtins()` will also
        /// skip keywords. But this may not always be true. And one
        /// might want to skip keywords but *not* builtins.
        fn skip_keywords(mut self) -> CompletionTestBuilder {
            self.skip_keywords = true;
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
                            .unwrap_or("<no import required>");
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
                skip_keywords: false,
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
