#![allow(warnings)]

/*!
An abstraction for adding new imports to a single Python source file.

This importer is based on a similar abstraction in `ruff_linter::importer`.
Both of them use the lower-level `ruff_python_importer::Insertion` primitive.
The main differences here are:

1. This works with ty's semantic model instead of ruff's.
2. This owns the task of visiting AST to extract imports. This
   design was chosen because it's currently only used for inserting
   imports for unimported completion suggestions. If it needs to be
   used more broadly, it might make sense to roll construction of an
   `Importer` into ty's `SemanticIndex`.
3. It doesn't have as many facilities as `ruff_linter`'s importer.
*/

use rustc_hash::FxHashMap;

use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::source_text;
use ruff_diagnostics::Edit;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal, walk_stmt};
use ruff_python_codegen::Stylist;
use ruff_python_importer::Insertion;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_module_resolver::ModuleName;
use ty_project::Db;
use ty_python_semantic::semantic_index::definition::DefinitionKind;
use ty_python_semantic::types::Type;
use ty_python_semantic::{MemberDefinition, SemanticModel};

pub(crate) struct Importer<'a> {
    /// The ty Salsa database.
    db: &'a dyn Db,
    /// The file corresponding to the module that
    /// we want to insert an import statement into.
    file: File,
    /// The parsed module ref.
    parsed: &'a ParsedModuleRef,
    /// The tokens representing the Python AST.
    tokens: &'a Tokens,
    /// The source code for `file`.
    source: &'a str,
    /// The [`Stylist`] for the Python AST.
    stylist: &'a Stylist<'a>,
    /// The list of visited, top-level runtime imports in the Python AST.
    imports: Vec<AstImport<'a>>,
}

impl<'a> Importer<'a> {
    /// Create a new importer.
    ///
    /// The [`Stylist`] dictates the code formatting options of any code
    /// edit (if any) produced by this importer.
    ///
    /// The `file` given should correspond to the module that we want
    /// to insert an import statement into.
    ///
    /// The `source` is used to get access to the original source
    /// text for `file`, which is used to help produce code edits (if
    /// any).
    ///
    /// The AST given (corresponding to the contents of `file`) is
    /// traversed and top-level imports are extracted from it. This
    /// permits adding imports in a way that is harmonious with
    /// existing imports.
    pub(crate) fn new(
        db: &'a dyn Db,
        stylist: &'a Stylist<'a>,
        file: File,
        source: &'a str,
        parsed: &'a ParsedModuleRef,
    ) -> Self {
        let imports = TopLevelImports::find(parsed.syntax());

        Self {
            db,
            file,
            parsed,
            tokens: parsed.tokens(),
            source,
            stylist,
            imports,
        }
    }

    /// Builds a set of members in scope at the given AST node and position.
    ///
    /// Callers should use this routine to build "in scope members" to be used
    /// with repeated calls to `Importer::import`. This does some work up-front
    /// to avoid doing it for every call to `Importer::import`.
    ///
    /// In general, `at` should be equivalent to `node.start()` (from the
    /// [`ruff_text_size::Ranged`] trait). However, in some cases, identifying
    /// a good AST node for where the cursor is can be difficult, where as
    /// knowing the precise position of the cursor is easy. The AST node in
    /// that circumstance may be a very poor approximation that may still
    /// result in good auto-import results.
    ///
    /// This API is designed with completions in mind. That is, we might have
    /// many possible candidates to add as an import while the position we want
    /// to insert them remains invariant.
    pub fn members_in_scope_at(
        &self,
        node: ast::AnyNodeRef<'_>,
        at: TextSize,
    ) -> MembersInScope<'a> {
        MembersInScope::new(self.db, self.file, self.parsed, node, at)
    }

    /// Imports a symbol into this importer's module.
    ///
    /// The given request is assumed to be valid. That is, the module
    /// is assumed to be importable and the member is assumed to be a
    /// valid thing to import from the given module.
    ///
    /// When possible (particularly when there is no existing import
    /// statement to satisfy the given request), the import style on
    /// the request is respected. When there is an existing import,
    /// then the existing style is always respected instead.
    ///
    /// `members` should be a map of symbols in scope at the position
    /// where the imported symbol should be available. This is used
    /// to craft import statements in a way that doesn't conflict with
    /// symbols in scope. If it's not feasible to provide this map, then
    /// providing an empty map is generally fine. But it does mean that
    /// the resulting import may shadow (or be shadowed by) some other
    /// symbol.
    ///
    /// The "import action" returned includes an edit for inserting
    /// the actual import (if necessary) along with the symbol text
    /// that should be used to refer to the imported symbol. While
    /// the symbol text may be expected to just be equivalent to the
    /// request's `member`, it can be different. For example, there
    /// might be an alias, or the corresponding module might already be
    /// imported in a qualified way.
    pub(crate) fn import(
        &self,
        request: ImportRequest<'_>,
        members: &MembersInScope,
    ) -> ImportAction {
        let request = request.avoid_conflicts(self.db, self.file, members);
        let mut symbol_text: Box<str> = request.member.unwrap_or(request.module).into();
        let Some(response) = self.find(&request, members.at) else {
            let insertion = if let Some(future) = self.find_last_future_import(members.at) {
                Insertion::end_of_statement(future.stmt, self.source, self.stylist)
            } else {
                let range = source_text(self.db, self.file)
                    .as_notebook()
                    .and_then(|notebook| notebook.cell_offsets().containing_range(members.at));

                Insertion::start_of_file(self.parsed.suite(), self.source, self.stylist, range)
            };
            let import = insertion.into_edit(&request.to_string());
            if let Some(member) = request.member
                && matches!(request.style, ImportStyle::Import)
            {
                symbol_text = format!("{}.{}", request.module, member).into();
            }
            return ImportAction {
                import: Some(import),
                symbol_text,
            };
        };

        // When we just have a request to import a module (and not
        // any members from that module), then the only way we can be
        // here is if we found a pre-existing import that definitively
        // satisfies the request. So we're done.
        let Some(member) = request.member else {
            return ImportAction {
                import: None,
                symbol_text,
            };
        };
        match response.kind {
            ImportResponseKind::Unqualified { ast, alias } => {
                let member = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                // As long as it's not a wildcard import, we use whatever name
                // the member is imported as when inserting the symbol.
                if member != "*" {
                    symbol_text = member.into();
                }
                ImportAction {
                    import: None,
                    symbol_text,
                }
            }
            ImportResponseKind::Qualified { ast, alias } => {
                let module = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                ImportAction {
                    import: None,
                    symbol_text: format!("{module}.{symbol_text}").into(),
                }
            }
            ImportResponseKind::Partial(ast) => {
                let import = if let Some(insertion) =
                    Insertion::existing_import(response.import.stmt, self.tokens)
                {
                    insertion.into_edit(member)
                } else {
                    Insertion::end_of_statement(response.import.stmt, self.source, self.stylist)
                        .into_edit(&format!("from {} import {member}", request.module))
                };
                ImportAction {
                    import: Some(import),
                    symbol_text,
                }
            }
        }
    }

    /// Look for an import already in this importer's module that
    /// satisfies the given request. If found, the corresponding
    /// import is returned along with the way in which the import
    /// satisfies the request.
    fn find<'importer>(
        &'importer self,
        request: &ImportRequest<'_>,
        available_at: TextSize,
    ) -> Option<ImportResponse<'importer, 'a>> {
        let mut choice = None;
        let source = source_text(self.db, self.file);
        let notebook = source.as_notebook();

        for import in &self.imports {
            // If the import statement comes after the spot where we
            // need the symbol, then we conservatively assume that
            // the import statement does not satisfy the request. It
            // is possible the import statement *could* satisfy the
            // request. For example, if `available_at` is inside a
            // function defined before the import statement. But this
            // only works if the function is known to be called *after*
            // the import statement executes. So... it's complicated.
            // In the worst case, we'll end up inserting a superfluous
            // import statement at the top of the module.
            //
            // Also, we can stop here since our import statements are
            // sorted by their start location in the source.
            if import.stmt.start() >= available_at {
                return choice;
            }

            if let Some(response) = import.satisfies(self.db, self.file, request) {
                let partial = matches!(response.kind, ImportResponseKind::Partial { .. });

                // The LSP doesn't support edits across cell boundaries.
                // Skip over imports that only partially satisfy the import
                // because they would require changes to the import (across cell boundaries).
                if partial
                    && let Some(notebook) = notebook
                    && notebook
                        .cell_offsets()
                        .has_cell_boundary(TextRange::new(import.stmt.start(), available_at))
                {
                    continue;
                }

                if choice
                    .as_ref()
                    .is_none_or(|c| !c.kind.is_prioritized_over(&response.kind))
                {
                    let is_top_priority =
                        matches!(response.kind, ImportResponseKind::Unqualified { .. });
                    choice = Some(response);
                    // When we find an unqualified import, it's (currently)
                    // impossible for any later import to override it in
                    // priority. So we can just quit here.
                    if is_top_priority {
                        return choice;
                    }
                }
            }
        }
        choice
    }

    /// Find the last `from __future__` import statement in the AST.
    fn find_last_future_import(&self, at: TextSize) -> Option<&'a AstImport> {
        let source = source_text(self.db, self.file);
        let notebook = source.as_notebook();

        self.imports
            .iter()
            .take_while(|import| import.stmt.start() <= at)
            // Skip over imports from other cells.
            .skip_while(|import| {
                notebook.is_some_and(|notebook| {
                    notebook
                        .cell_offsets()
                        .has_cell_boundary(TextRange::new(import.stmt.start(), at))
                })
            })
            .take_while(|import| {
                import
                    .stmt
                    .as_import_from_stmt()
                    .is_some_and(|import_from| import_from.module.as_deref() == Some("__future__"))
            })
            .last()
    }
}

/// A map of symbols in scope at a particular location in a module.
///
/// Users of an `Importer` must create this map via
/// [`Importer::members_in_scope_at`] in order to use the [`Importer::import`]
/// API. This map provides quick access to symbols in scope to help ensure that
/// the imports inserted are correct and do not conflict with existing symbols.
///
/// Note that this isn't perfect. At time of writing (2025-09-16), the importer
/// makes the trade-off that it's better to insert an incorrect import than to
/// silently do nothing. Perhaps in the future we can find a way to prompt end
/// users for a decision. This behavior is modeled after rust-analyzer, which
/// does the same thing for auto-import on unimported completions.
#[derive(Debug)]
pub struct MembersInScope<'ast> {
    at: TextSize,
    map: FxHashMap<Name, MemberInScope<'ast>>,
}

impl<'ast> MembersInScope<'ast> {
    fn new(
        db: &'ast dyn Db,
        file: File,
        parsed: &'ast ParsedModuleRef,
        node: ast::AnyNodeRef<'_>,
        at: TextSize,
    ) -> MembersInScope<'ast> {
        let model = SemanticModel::new(db, file);
        let map = model
            .members_in_scope_at(node)
            .into_iter()
            .map(|(name, memberdef)| {
                let def = memberdef.first_reachable_definition;
                let kind = match *def.kind(db) {
                    DefinitionKind::Import(ref kind) => {
                        MemberImportKind::Imported(AstImportKind::Import(kind.import(parsed)))
                    }
                    DefinitionKind::ImportFrom(ref kind) => {
                        MemberImportKind::Imported(AstImportKind::ImportFrom(kind.import(parsed)))
                    }
                    DefinitionKind::StarImport(ref kind) => {
                        MemberImportKind::Imported(AstImportKind::ImportFrom(kind.import(parsed)))
                    }
                    _ => MemberImportKind::Other,
                };
                (
                    name,
                    MemberInScope {
                        ty: memberdef.ty,
                        kind,
                    },
                )
            })
            .collect();
        MembersInScope { at, map }
    }
}

#[derive(Debug)]
struct MemberInScope<'ast> {
    ty: Type<'ast>,
    kind: MemberImportKind<'ast>,
}

impl<'ast> MemberInScope<'ast> {
    /// Returns a member with the given type and "irrelevant"
    /// definition site. That is, the only definition sites
    /// we currently care about are import statements.
    fn other(ty: Type<'ast>) -> MemberInScope<'ast> {
        MemberInScope {
            ty,
            kind: MemberImportKind::Other,
        }
    }

    /// Returns true if this symbol satisfies the given import request. This
    /// attempts to take the definition site of the symbol into account.
    fn satisfies(&self, db: &dyn Db, importing_file: File, request: &ImportRequest<'_>) -> bool {
        let MemberImportKind::Imported(ref ast_import) = self.kind else {
            return false;
        };
        ast_import.satisfies(db, importing_file, request).is_some()
    }
}

/// A type describing how a symbol was defined.
#[derive(Debug)]
enum MemberImportKind<'ast> {
    /// A symbol was introduced through an import statement.
    Imported(AstImportKind<'ast>),
    /// A symbol was introduced through something other
    /// than an import statement.
    Other,
}

/// The edits needed to insert the import statement.
///
/// While this is usually just an edit to add an import statement (or
/// modify an existing one), it can also sometimes just be a change
/// to the text that should be inserted for a particular symbol. For
/// example, if one were to ask for `search` from the `re` module, and
/// `re` was already imported, then we'd return no edits for import
/// statements and the text `re.search` to use for the symbol.
#[derive(Debug)]
pub(crate) struct ImportAction {
    import: Option<Edit>,
    symbol_text: Box<str>,
}

impl ImportAction {
    /// Returns an edit to insert an import statement.
    pub(crate) fn import(&self) -> Option<&Edit> {
        self.import.as_ref()
    }

    /// Returns the symbol text that should be used.
    ///
    /// Usually this is identical to the symbol text given to the corresponding
    /// [`ImportRequest`], but this may sometimes be fully qualified based on
    /// existing imports or import preferences.
    pub(crate) fn symbol_text(&self) -> &str {
        &*self.symbol_text
    }
}

/// A borrowed AST of a Python import statement.
#[derive(Debug)]
struct AstImport<'ast> {
    /// The original AST statement containing the import.
    stmt: &'ast ast::Stmt,
    /// The specific type of import.
    ///
    /// Storing this means we can do exhaustive case analysis
    /// on the type of the import without needing to constantly
    /// unwrap it from a more general `Stmt`. Still, we keep the
    /// `Stmt` around because some APIs want that.
    kind: AstImportKind<'ast>,
}

impl<'ast> AstImport<'ast> {
    /// Returns whether this import satisfies the given request.
    ///
    /// If it does, then this returns *how* the import satisfies
    /// the request.
    fn satisfies<'importer>(
        &'importer self,
        db: &'_ dyn Db,
        importing_file: File,
        request: &ImportRequest<'_>,
    ) -> Option<ImportResponse<'importer, 'ast>> {
        self.kind
            .satisfies(db, importing_file, request)
            .map(|kind| ImportResponse { import: self, kind })
    }
}

/// The specific kind of import.
#[derive(Debug)]
enum AstImportKind<'ast> {
    Import(&'ast ast::StmtImport),
    ImportFrom(&'ast ast::StmtImportFrom),
}

impl<'ast> AstImportKind<'ast> {
    /// Returns whether this import satisfies the given request.
    ///
    /// If it does, then this returns *how* the import satisfies
    /// the request.
    fn satisfies<'importer>(
        &'importer self,
        db: &'_ dyn Db,
        importing_file: File,
        request: &ImportRequest<'_>,
    ) -> Option<ImportResponseKind<'ast>> {
        match *self {
            AstImportKind::Import(ast) => {
                if request.force_style && !matches!(request.style, ImportStyle::Import) {
                    return None;
                }
                let alias = ast
                    .names
                    .iter()
                    .find(|alias| alias.name.as_str() == request.module)?;
                Some(ImportResponseKind::Qualified { ast, alias })
            }
            AstImportKind::ImportFrom(ast) => {
                // If the request is for a module itself, then we
                // assume that it can never be satisfies by a
                // `from ... import ...` statement. For example, a
                // `request for collections.abc` needs an
                // `import collections.abc`. Now, there could be a
                // `from collections import abc`, and we could
                // plausibly consider that a match and return a
                // symbol text of `abc`. But it's not clear if that's
                // the right choice or not.
                let member = request.member?;

                if request.force_style && !matches!(request.style, ImportStyle::ImportFrom) {
                    return None;
                }

                let module = ModuleName::from_import_statement(db, importing_file, ast).ok()?;
                if module.as_str() != request.module {
                    return None;
                }
                let kind = ast
                    .names
                    .iter()
                    .find(|alias| alias.name.as_str() == "*" || alias.name.as_str() == member)
                    .map(|alias| ImportResponseKind::Unqualified { ast, alias })
                    .unwrap_or_else(|| ImportResponseKind::Partial(ast));
                Some(kind)
            }
        }
    }
}

/// A request to import a module into the global scope of a Python module.
#[derive(Debug)]
pub(crate) struct ImportRequest<'a> {
    /// The module from which the symbol should be imported (e.g.,
    /// `foo`, in `from foo import bar`).
    module: &'a str,
    /// The member to import (e.g., `bar`, in `from foo import bar`).
    ///
    /// When `member` is absent, then this request reflects an import
    /// of the module itself. i.e., `import module`.
    member: Option<&'a str>,
    /// The preferred style to use when importing the symbol (e.g.,
    /// `import foo` or `from foo import bar`).
    ///
    /// This style isn't respected if the `module` already has
    /// an import statement. In that case, the existing style is
    /// respected.
    style: ImportStyle,
    /// Whether the import style ought to be forced for correctness
    /// reasons. For example, to avoid shadowing or introducing a
    /// conflicting name.
    force_style: bool,
}

impl<'a> ImportRequest<'a> {
    /// Create a new [`ImportRequest`] from a `module` and `member`.
    ///
    /// If `module` has no existing imports, the symbol should be
    /// imported using the `import` statement.
    pub(crate) fn import(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member: Some(member),
            style: ImportStyle::Import,
            force_style: false,
        }
    }

    /// Create a new [`ImportRequest`] from a module and member.
    ///
    /// If `module` has no existing imports, the symbol should be
    /// imported using the `import from` statement.
    pub(crate) fn import_from(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member: Some(member),
            style: ImportStyle::ImportFrom,
            force_style: false,
        }
    }

    /// Create a new [`ImportRequest`] for bringing the given module
    /// into scope.
    ///
    /// This is for just importing the module itself, always via an
    /// `import` statement.
    pub(crate) fn module(module: &'a str) -> Self {
        Self {
            module,
            member: None,
            style: ImportStyle::Import,
            force_style: false,
        }
    }

    /// Causes this request to become a command. This will force the
    /// requested import style, even if another style would be more
    /// appropriate generally.
    pub(crate) fn force(mut self) -> Self {
        Self {
            force_style: true,
            ..self
        }
    }

    /// Attempts to change the import request style so that the chances
    /// of an import conflict are minimized (although not always reduced
    /// to zero).
    fn avoid_conflicts(self, db: &dyn Db, importing_file: File, members: &MembersInScope) -> Self {
        let Some(member) = self.member else {
            return Self {
                style: ImportStyle::Import,
                ..self
            };
        };
        match (members.map.get(self.module), members.map.get(member)) {
            // Neither symbol exists, so we can just proceed as
            // normal.
            (None, None) => self,
            // The symbol we want to import already exists but
            // the module symbol does not, so we can import the
            // symbol in a qualified way safely.
            (None, Some(member)) => {
                // ... unless the symbol we want is already
                // imported, then leave it as-is.
                if member.satisfies(db, importing_file, &self) {
                    return self;
                }
                Self {
                    style: ImportStyle::Import,
                    force_style: true,
                    ..self
                }
            }
            // The symbol we want to import doesn't exist but
            // the module does. So we can import the symbol we
            // want *unqualified* safely.
            //
            // ... unless the module symbol we found here is
            // actually a module symbol.
            (
                Some(&MemberInScope {
                    ty: Type::ModuleLiteral(_),
                    ..
                }),
                None,
            ) => self,
            (Some(_), None) => Self {
                style: ImportStyle::ImportFrom,
                force_style: true,
                ..self
            },
            // Both the module and the member symbols are in
            // scope. We *assume* that the module symbol is in
            // scope because it is imported. Since the member
            // symbol is definitively in scope, we attempt a
            // qualified import.
            //
            // This could lead to a situation where we add an
            // `import` that is shadowed by some other symbol.
            // This is unfortunate, but it's not clear what we
            // should do instead. rust-analyzer will still add
            // the conflicting import. I think that's the wiser
            // choice, instead of silently doing nothing or
            // silently omitting the symbol from completions.
            // (I suppose the best choice would be to ask the
            // user for an alias for the import or something.)
            (Some(_), Some(_)) => Self {
                style: ImportStyle::Import,
                force_style: false,
                ..self
            },
        }
    }
}

impl std::fmt::Display for ImportRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.style {
            ImportStyle::Import => write!(f, "import {}", self.module),
            ImportStyle::ImportFrom => match self.member {
                None => write!(f, "import {}", self.module),
                Some(member) => write!(f, "from {} import {member}", self.module),
            },
        }
    }
}

/// The response to an import request.
#[derive(Debug)]
struct ImportResponse<'importer, 'ast> {
    import: &'importer AstImport<'ast>,
    kind: ImportResponseKind<'ast>,
}

/// The kind of response to an import request.
///
/// This encodes the answer to the question: how does a given import
/// statement satisfy an [`ImportRequest`]? This encodes the different
/// degrees to the request is satisfied.
#[derive(Debug)]
enum ImportResponseKind<'ast> {
    /// The import satisfies the request as-is. The symbol is already
    /// imported directly and may be used unqualified.
    ///
    /// This always corresponds to a `from <...> import <...>`
    /// statement. Note that `<...>` may be a wildcard import!
    Unqualified {
        /// The AST of the import that satisfied the request.
        ast: &'ast ast::StmtImportFrom,
        /// The specific alias in the `from <...> import <...>`
        /// statement that satisfied the request's `member`.
        alias: &'ast ast::Alias,
    },
    /// The necessary module is imported, but the symbol itself is not
    /// in scope. The symbol can be used via `module.symbol`.
    ///
    /// This always corresponds to a `import <...>` statement.
    Qualified {
        /// The AST of the import that satisfied the request.
        ast: &'ast ast::StmtImport,
        /// The specific alias in the import statement that
        /// satisfied the request's `module`.
        alias: &'ast ast::Alias,
    },
    /// The necessary module is imported via `from module import ...`,
    /// but the desired symbol is not listed in `...`.
    ///
    /// This always corresponds to a `from <...> import <...>`
    /// statement.
    ///
    /// It is guaranteed that this never contains a wildcard import.
    /// (otherwise, this import wouldn't be partial).
    Partial(&'ast ast::StmtImportFrom),
}

impl ImportResponseKind<'_> {
    /// Returns true if this import statement kind should be
    /// prioritized over the one given.
    ///
    /// This assumes that `self` occurs before `other` in the source
    /// code.
    fn is_prioritized_over(&self, other: &ImportResponseKind<'_>) -> bool {
        self.priority() <= other.priority()
    }

    /// Returns an integer reflecting the "priority" of this
    /// import kind relative to other import statements.
    ///
    /// Lower values indicate higher priority.
    fn priority(&self) -> usize {
        match *self {
            ImportResponseKind::Unqualified { .. } => 0,
            ImportResponseKind::Partial(_) => 1,
            // N.B. When given the choice between adding a
            // name to an existing `from ... import ...`
            // statement and using an existing `import ...`
            // in a qualified manner, we currently choose
            // the former. Originally we preferred qualification,
            // but there is some evidence that this violates
            // expectations.
            //
            // Ref: https://github.com/astral-sh/ty/issues/1274#issuecomment-3352233790
            ImportResponseKind::Qualified { .. } => 2,
        }
    }
}

/// The style of a Python import statement.
#[derive(Debug)]
enum ImportStyle {
    /// Import the symbol using the `import` statement (e.g. `import
    /// foo; foo.bar`).
    Import,
    /// Import the symbol using the `from` statement (e.g. `from foo
    /// import bar; bar`).
    ImportFrom,
}

/// An error that can occur when trying to add an import.
#[derive(Debug)]
pub(crate) enum AddImportError {
    /// The symbol can't be imported, because another symbol is bound to the
    /// same name.
    ConflictingName(String),
}

impl std::fmt::Display for AddImportError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddImportError::ConflictingName(binding) => std::write!(
                fmt,
                "Unable to insert `{binding}` into scope due to name conflict"
            ),
        }
    }
}

impl std::error::Error for AddImportError {}

/// An AST visitor for extracting top-level imports.
#[derive(Debug, Default)]
struct TopLevelImports<'ast> {
    level: u64,
    imports: Vec<AstImport<'ast>>,
}

impl<'ast> TopLevelImports<'ast> {
    /// Find all top-level imports from the given AST of a Python module.
    fn find(module: &'ast ast::ModModule) -> Vec<AstImport<'ast>> {
        let mut visitor = TopLevelImports::default();
        visitor.visit_body(&module.body);
        visitor.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for TopLevelImports<'ast> {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match *stmt {
            ast::Stmt::Import(ref node) => {
                if self.level == 0 {
                    let kind = AstImportKind::Import(node);
                    self.imports.push(AstImport { stmt, kind });
                }
            }
            ast::Stmt::ImportFrom(ref node) => {
                if self.level == 0 {
                    let kind = AstImportKind::ImportFrom(node);
                    self.imports.push(AstImport { stmt, kind });
                }
            }
            _ => {
                // OK because it's not practical for the source code
                // depth of a Python to exceed a u64.
                //
                // Also, it is perhaps a bit too eager to increment
                // this for every non-import statement, particularly
                // compared to the more refined scope tracking in the
                // semantic index builder. However, I don't think
                // we need anything more refined here. We only care
                // about top-level imports. So as soon as we get into
                // something nested, we can bail out.
                //
                // Although, this does mean, e.g.,
                //
                //     if predicate:
                //         import whatever
                //
                // at the module scope is not caught here. If we
                // need those imports, I think we'll just want some
                // more case analysis with more careful `level`
                // incrementing.
                self.level = self.level.checked_add(1).unwrap();
                walk_stmt(self, stmt);
                // Always OK because we can only be here after
                // a successful +1 from above.
                self.level = self.level.checked_sub(1).unwrap();
            }
        }
    }

    #[inline]
    fn enter_node(&mut self, node: ast::AnyNodeRef<'ast>) -> TraversalSignal {
        if node.is_statement() {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Component;
    use insta::assert_snapshot;
    use insta::internals::SettingsBindDropGuard;

    use crate::tests::{CursorTest, CursorTestBuilder, cursor_test};
    use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};
    use ruff_db::files::{File, FileRootKind, system_path_to_file};
    use ruff_db::parsed::parsed_module;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_db::{Db, system};
    use ruff_python_ast::find_node::covering_node;
    use ruff_python_codegen::Stylist;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::TextSize;
    use ty_module_resolver::SearchPathSettings;
    use ty_project::ProjectMetadata;
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPlatform, PythonVersionWithSource, SemanticModel,
    };

    use super::*;

    impl CursorTest {
        fn import(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import(module, member))
        }

        fn import_from(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import_from(module, member))
        }

        fn module(&self, module: &str) -> String {
            self.add(ImportRequest::module(module))
        }

        fn add(&self, request: ImportRequest<'_>) -> String {
            let node = covering_node(
                self.cursor.parsed.syntax().into(),
                TextRange::empty(self.cursor.offset),
            )
            .node();
            let importer = self.importer();
            let members = importer.members_in_scope_at(node, self.cursor.offset);
            let resp = importer.import(request, &members);

            // We attempt to emulate what an LSP client would
            // do here and "insert" the import into the original
            // source document. I'm not 100% sure this models
            // reality correctly, but in particular, we are
            // careful to insert the symbol name first since
            // it *should* come after the import.
            let mut source = self.cursor.source.to_string();
            source.insert_str(self.cursor.offset.to_usize(), &resp.symbol_text);
            if let Some(edit) = resp.import() {
                assert!(
                    edit.range().start() <= self.cursor.offset,
                    "import edit must come at or before <CURSOR>, \
                     but <CURSOR> starts at {} and the import \
                     edit is at {}..{}",
                    self.cursor.offset.to_usize(),
                    edit.range().start().to_usize(),
                    edit.range().end().to_usize(),
                );
                source.replace_range(edit.range().to_std_range(), edit.content().unwrap_or(""));
            }
            source
        }

        fn importer(&self) -> Importer<'_> {
            Importer::new(
                &self.db,
                &self.cursor.stylist,
                self.cursor.file,
                self.cursor.source.as_str(),
                &self.cursor.parsed,
            )
        }
    }

    #[test]
    fn empty_source_qualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn empty_source_unqualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified() {
        let test = cursor_test(
            "\
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified() {
        let test = cursor_test(
            "\
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_glob() {
        let test = cursor_test(
            "\
from collections import *
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import *
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified_aliased() {
        let test = cursor_test(
            "\
import collections as c
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections as c
        c.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified_aliased() {
        let test = cursor_test(
            "\
from collections import defaultdict as ddict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict as ddict
        ddict
        ");
    }

    #[test]
    fn import_partially_exists_single() {
        let test = cursor_test(
            "\
from collections import Counter
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_aliased_single() {
        let test = cursor_test(
            "\
from collections import Counter as C
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter as C, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_multi() {
        let test = cursor_test(
            "\
from collections import Counter, OrderedDict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter, OrderedDict, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_aliased_multi() {
        let test = cursor_test(
            "\
from collections import Counter as C, OrderedDict as OD
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter as C, OrderedDict as OD, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_semi_colon() {
        let test = cursor_test(
            "\
from collections import Counter;
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter, defaultdict;
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_continuation() {
        let test = cursor_test(
            "\
from collections import Counter, \\
  OrderedDict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter, \
          OrderedDict, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_single() {
        let test = cursor_test(
            "\
from collections import (Counter)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import (Counter, defaultdict)
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (Counter,)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import (Counter, defaultdict,)
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_multi_line_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (
    Counter,
    OrderedDict,
)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import (
            Counter,
            OrderedDict, defaultdict,
        )
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_multi_line_no_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (
    Counter,
    OrderedDict
)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import (
            Counter,
            OrderedDict, defaultdict
        )
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_relative() {
        let test = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "Foo = 1\nBar = 2\n")
            .source(
                "package/sub1/sub2/quux.py",
                "from ...foo import Foo\n<CURSOR>\n",
            )
            .build();
        assert_snapshot!(
            test.import("package.foo", "Bar"), @r"
        from ...foo import Foo, Bar
        Bar
        ");
    }

    #[test]
    fn import_partially_exists_incomplete() {
        let test = cursor_test(
            "\
from collections import
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_incomplete_parentheses1() {
        let test = cursor_test(
            "\
from collections import ()
<CURSOR>
        ",
        );
        // In this case, because of the `()` being an
        // invalid AST, our importer gives up and just
        // adds a new line. We could add more heuristics
        // to make this case work, but I think there will
        // always be some cases like this that won't make
        // sense.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import ()
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_unqualified() {
        let test = cursor_test(
            "\
from collections import defaultdict
import re
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        import re
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_unqualified_between() {
        let test = cursor_test(
            "\
from collections import defaultdict
import re
<CURSOR>
from collections import defaultdict
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        import re
        defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_qualified() {
        let test = cursor_test(
            "\
import collections
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_partial() {
        let test = cursor_test(
            "\
from collections import OrderedDict
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import OrderedDict
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_qualified_over_partial() {
        let test = cursor_test(
            "\
from collections import OrderedDict
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import OrderedDict, defaultdict
        import collections
        defaultdict
        ");
    }

    #[test]
    fn out_of_scope_ordering_top_level() {
        let test = cursor_test(
            "\
<CURSOR>
from collections import defaultdict
        ",
        );
        // Since the import came after the cursor,
        // we add another import at the top-level
        // of the module.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections.defaultdict
        from collections import defaultdict
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn out_of_scope_ordering_within_function_add_import() {
        let test = cursor_test(
            "\
def foo():
    <CURSOR>
from collections import defaultdict
        ",
        );
        // Since the import came after the cursor,
        // we add another import at the top-level
        // of the module.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        def foo():
            collections.defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn in_scope_ordering_within_function() {
        let test = cursor_test(
            "\
from collections import defaultdict

def foo():
    <CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict

        def foo():
            defaultdict
        ");
    }

    #[test]
    fn existing_future_import() {
        let test = cursor_test(
            "\
from __future__ import annotations

<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("typing", "TypeVar"), @r"
        from __future__ import annotations
        import typing

        typing.TypeVar
        ");
    }

    #[test]
    fn existing_future_import_after_docstring() {
        let test = cursor_test(
            r#"
"This is a module level docstring"
from __future__ import annotations

<CURSOR>
        "#,
        );
        assert_snapshot!(
            test.import("typing", "TypeVar"), @r#"
        "This is a module level docstring"
        from __future__ import annotations
        import typing

        typing.TypeVar
        "#);
    }

    #[test]
    fn qualify_symbol_to_avoid_overwriting_other_symbol_in_scope() {
        let test = cursor_test(
            "\
defaultdict = 1
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        defaultdict = 1
        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        defaultdict = 1
        (collections.defaultdict)
        ");
    }

    #[test]
    fn unqualify_symbol_to_avoid_overwriting_other_symbol_in_scope() {
        let test = cursor_test(
            "\
collections = 1
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        collections = 1
        (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        collections = 1
        (defaultdict)
        ");
    }

    /// Tests a failure scenario where both the module
    /// name and the member name are in scope and defined
    /// as something other than a module. In this case,
    /// it's very difficult to auto-insert an import in a
    /// way that is correct.
    ///
    /// At time of writing (2025-09-15), we just insert a
    /// qualified import anyway, even though this will result
    /// in what is likely incorrect code. This seems better
    /// than some alternatives:
    ///
    /// 1. Silently do nothing.
    /// 2. Silently omit the symbol from completions.
    /// 3. Come up with an alias for the symbol.
    ///
    /// I think it would perhaps be ideal if we could somehow
    /// prompt the user for what they want to do. But I think
    /// this is okay for now. ---AG
    #[test]
    fn import_results_in_conflict() {
        let test = cursor_test(
            "\
collections = 1
defaultdict = 2
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections = 1
        defaultdict = 2
        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        collections = 1
        defaultdict = 2
        (collections.defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_simple() {
        let test = cursor_test(
            "\
def foo():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        def foo():
            (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        def foo():
            (defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_member_conflict() {
        let test = cursor_test(
            "\
def defaultdict():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        def defaultdict():
            (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        def defaultdict():
            (collections.defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_module_conflict() {
        let test = cursor_test(
            "\
def collections():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        def collections():
            (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        def collections():
            (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_import() {
        let test = cursor_test(
            "\
import defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        import defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        import defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn module_conflict_with_other_import() {
        let test = cursor_test(
            "\
from foo import collections

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        from foo import collections

        (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        from foo import collections

        (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_member_import() {
        let test = cursor_test(
            "\
from othermodule import defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        from othermodule import defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        from othermodule import defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_module_import_alias() {
        let test = cursor_test(
            "\
import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_member_import_alias() {
        let test = cursor_test(
            "\
from othermodule import something as defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        from othermodule import something as defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        import collections
        from othermodule import something as defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn no_conflict_alias_module() {
        let test = cursor_test(
            "\
import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn no_conflict_alias_member() {
        let test = cursor_test(
            "\
from foo import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        from foo import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        from foo import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn multiple_import_blocks_std() {
        let test = cursor_test(
            "\
import json
import re

from whenever import ZonedDateTime
import numpy as np

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (defaultdict)
        ");
    }

    #[test]
    fn multiple_import_blocks_other() {
        let test = CursorTest::builder()
            .source("foo.py", "Foo = 1\nBar = 2\n")
            .source(
                "main.py",
                "\
import json
import re

from whenever import ZonedDateTime
import numpy as np

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "Bar"), @r"
        import foo
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (foo.Bar)
        ");
        assert_snapshot!(
            test.import_from("foo", "Bar"), @r"
        from foo import Bar
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (Bar)
        ");
    }

    #[test]
    fn conditional_imports_new_import() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("quux", "MAGIC"), @r#"
        import quux
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (quux.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("quux", "MAGIC"), @r#"
        import quux
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (quux.MAGIC)
        "#);
    }

    // FIXME: This test (and the one below it) aren't
    // quite right. Namely, because we aren't handling
    // multiple binding sites correctly, we don't see the
    // existing `MAGIC` symbol.
    #[test]
    fn conditional_imports_existing_import1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @r#"
        import foo
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (foo.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @r#"
        from foo import MAGIC
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (MAGIC)
        "#);
    }

    #[test]
    fn conditional_imports_existing_import2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (bar.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (bar.MAGIC)
        "#);
    }

    // FIXME: This test (and the one below it) aren't quite right. We
    // don't recognize the multiple declaration sites for `fubar`.
    //
    // In this case, it's not totally clear what we should do. Since we
    // are trying to import `MAGIC` from `foo`, we could add a `from
    // foo import MAGIC` within the first `if` block. Or we could try
    // and "infer" something about the code assuming that we know
    // `MAGIC` is in both `foo` and `bar`.
    #[test]
    fn conditional_imports_existing_module1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    import foo as fubar
else:
    import bar as fubar

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @r#"
        import foo
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (foo.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @r#"
        from foo import MAGIC
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (MAGIC)
        "#);
    }

    #[test]
    fn conditional_imports_existing_module2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    import foo as fubar
else:
    import bar as fubar

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (bar.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @r#"
        from bar import MAGIC
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (MAGIC)
        "#);
    }

    #[test]
    fn try_imports_new_import() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("quux", "MAGIC"), @r"
        import quux
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (quux.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("quux", "MAGIC"), @r"
        import quux
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (quux.MAGIC)
        ");
    }

    // FIXME: This test (and the one below it) aren't
    // quite right. Namely, because we aren't handling
    // multiple binding sites correctly, we don't see the
    // existing `MAGIC` symbol.
    #[test]
    fn try_imports_existing_import1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @r"
        import foo
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (foo.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @r"
        from foo import MAGIC
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (MAGIC)
        ");
    }

    #[test]
    fn try_imports_existing_import2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @r"
        import bar
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (bar.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @r"
        import bar
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (bar.MAGIC)
        ");
    }

    #[test]
    fn import_module_blank() {
        let test = cursor_test(
            "\
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @r"
        import collections
        collections
        ");
    }

    #[test]
    fn import_module_exists() {
        let test = cursor_test(
            "\
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @r"
        import collections
        collections
        ");
    }

    #[test]
    fn import_module_from_exists() {
        let test = cursor_test(
            "\
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @r"
        import collections
        from collections import defaultdict
        collections
        ");
    }

    // This test is working as intended. That is,
    // `abc` is already in scope, so requesting an
    // import for `collections.abc` could feasibly
    // reuse the import and rewrite the symbol text
    // to just `abc`. But for now it seems better
    // to respect what has been written and add the
    // `import collections.abc`. This behavior could
    // plausibly be changed.
    #[test]
    fn import_module_from_via_member_exists() {
        let test = cursor_test(
            "\
from collections import abc
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections.abc"), @r"
        import collections.abc
        from collections import abc
        collections.abc
        ");
    }
}
