//! Data model, state management, and configuration resolution.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, Position, TextDocumentContentChangeEvent, Url};

use red_knot_python_semantic::semantic_index::{semantic_index, SemanticIndex};
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_db::system::SystemPath;
use ruff_db::Db;
use ruff_python_ast::{
    Arguments, BoolOp, Comprehension, Decorator, DictItem, ElifElseClause, ExceptHandler, Expr,
    ExprAttribute, ExprAwait, ExprBinOp, ExprBoolOp, ExprBooleanLiteral, ExprBytesLiteral,
    ExprCall, ExprCompare, ExprDict, ExprDictComp, ExprEllipsisLiteral, ExprFString, ExprGenerator,
    ExprIf, ExprLambda, ExprList, ExprListComp, ExprName, ExprNamed, ExprNumberLiteral, ExprSet,
    ExprSetComp, ExprSlice, ExprStarred, ExprStringLiteral, ExprSubscript, ExprTuple, ExprUnaryOp,
    ExprYield, ExprYieldFrom, FString, FStringExpressionElement, FStringPart, FStringValue,
    Identifier, Keyword, MatchCase, ModModule, Parameter, ParameterWithDefault, Parameters, Stmt,
    StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtDelete, StmtExpr,
    StmtFor, StmtFunctionDef, StmtGlobal, StmtIf, StmtImport, StmtImportFrom, StmtMatch,
    StmtNonlocal, StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWhile, StmtWith, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, TypeParams, WithItem,
};
use ruff_source_file::OneIndexed;
use ruff_text_size::{Ranged, TextRange};

use crate::edit::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::system::{url_to_any_system_path, AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::DocumentQuery;
pub(crate) use self::settings::AllSettings;
pub use self::settings::ClientSettings;

mod capabilities;
pub(crate) mod index;
mod settings;

// TODO(dhruvmanila): In general, the server shouldn't use any salsa queries directly and instead
// should use methods on `RootDatabase`.

/// The global state for the LSP
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    ///
    /// This will be [`None`] when a mutable reference is held to the index via [`index_mut`]
    /// to prevent the index from being accessed while it is being modified. It will be restored
    /// when the mutable reference ([`MutIndexGuard`]) is dropped.
    ///
    /// [`index_mut`]: Session::index_mut
    index: Option<Arc<index::Index>>,

    /// Maps workspace root paths to their respective databases.
    workspaces: BTreeMap<PathBuf, RootDatabase>,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
}

impl Session {
    pub fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspace_folders: &[(Url, ClientSettings)],
    ) -> crate::Result<Self> {
        let mut workspaces = BTreeMap::new();
        let index = Arc::new(index::Index::new(global_settings));

        for (url, _) in workspace_folders {
            let path = url
                .to_file_path()
                .map_err(|()| anyhow!("Workspace URL is not a file or directory: {:?}", url))?;
            let system_path = SystemPath::from_std_path(&path)
                .ok_or_else(|| anyhow!("Workspace path is not a valid UTF-8 path: {:?}", path))?;
            let system = LSPSystem::new(index.clone());

            // TODO(dhruvmanila): Get the values from the client settings
            let metadata = WorkspaceMetadata::from_path(system_path, &system, None)?;
            // TODO(micha): Handle the case where the program settings are incorrect more gracefully.
            workspaces.insert(path, RootDatabase::new(metadata, system)?);
        }

        Ok(Self {
            position_encoding,
            workspaces,
            index: Some(index),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        })
    }

    // TODO(dhruvmanila): Ideally, we should have a single method for `workspace_db_for_path_mut`
    // and `default_workspace_db_mut` but the borrow checker doesn't allow that.
    // https://github.com/astral-sh/ruff/pull/13041#discussion_r1726725437

    /// Returns a reference to the workspace [`RootDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn workspace_db_for_path(&self, path: impl AsRef<Path>) -> Option<&RootDatabase> {
        self.workspaces
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a mutable reference to the workspace [`RootDatabase`] corresponding to the given
    /// path, if any.
    pub(crate) fn workspace_db_for_path_mut(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Option<&mut RootDatabase> {
        self.workspaces
            .range_mut(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a reference to the default workspace [`RootDatabase`]. The default workspace is the
    /// minimum root path in the workspace map.
    pub(crate) fn default_workspace_db(&self) -> &RootDatabase {
        // SAFETY: Currently, red knot only support a single workspace.
        self.workspaces.values().next().unwrap()
    }

    /// Returns a mutable reference to the default workspace [`RootDatabase`].
    pub(crate) fn default_workspace_db_mut(&mut self) -> &mut RootDatabase {
        // SAFETY: Currently, red knot only support a single workspace.
        self.workspaces.values_mut().next().unwrap()
    }

    pub fn key_from_url(&self, url: Url) -> DocumentKey {
        self.index().key_from_url(url)
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let key = self.key_from_url(url);
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            document_ref: self.index().make_document_ref(key)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Registers a notebook document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub fn open_notebook_document(&mut self, url: Url, document: NotebookDocument) {
        self.index_mut().open_notebook_document(url, document);
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, url: Url, document: TextDocument) {
        self.index_mut().open_text_document(url, document);
    }

    /// Updates a text document at the associated `key`.
    ///
    /// The document key must point to a text document, or this will throw an error.
    pub(crate) fn update_text_document(
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
    ) -> crate::Result<()> {
        let position_encoding = self.position_encoding;
        self.index_mut()
            .update_text_document(key, content_changes, new_version, position_encoding)
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        self.index_mut().close_document(key)?;
        Ok(())
    }

    /// Returns a reference to the index.
    ///
    /// # Panics
    ///
    /// Panics if there's a mutable reference to the index via [`index_mut`].
    ///
    /// [`index_mut`]: Session::index_mut
    fn index(&self) -> &index::Index {
        self.index.as_ref().unwrap()
    }

    /// Returns a mutable reference to the index.
    ///
    /// This method drops all references to the index and returns a guard that will restore the
    /// references when dropped. This guard holds the only reference to the index and allows
    /// modifying it.
    fn index_mut(&mut self) -> MutIndexGuard {
        let index = self.index.take().unwrap();

        for db in self.workspaces.values_mut() {
            // Remove the `index` from each database. This drops the count of `Arc<Index>` down to 1
            db.system_mut()
                .as_any_mut()
                .downcast_mut::<LSPSystem>()
                .unwrap()
                .take_index();
        }

        // There should now be exactly one reference to index which is self.index.
        let index = Arc::into_inner(index);

        MutIndexGuard {
            session: self,
            index,
        }
    }
}

/// A guard that holds the only reference to the index and allows modifying it.
///
/// When dropped, this guard restores all references to the index.
struct MutIndexGuard<'a> {
    session: &'a mut Session,
    index: Option<index::Index>,
}

impl Deref for MutIndexGuard<'_> {
    type Target = index::Index;

    fn deref(&self) -> &Self::Target {
        self.index.as_ref().unwrap()
    }
}

impl DerefMut for MutIndexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.index.as_mut().unwrap()
    }
}

impl Drop for MutIndexGuard<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            let index = Arc::new(index);
            for db in self.session.workspaces.values_mut() {
                db.system_mut()
                    .as_any_mut()
                    .downcast_mut::<LSPSystem>()
                    .unwrap()
                    .set_index(index.clone());
            }

            self.session.index = Some(index);
        }
    }
}

/// An immutable snapshot of `Session` that references
/// a specific document.
#[derive(Debug)]
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn file(&self, db: &RootDatabase) -> Option<File> {
        match url_to_any_system_path(self.document_ref.file_url()).ok()? {
            AnySystemPath::System(path) => system_path_to_file(db, path).ok(),
            AnySystemPath::SystemVirtual(virtual_path) => db
                .files()
                .try_virtual_file(&virtual_path)
                .map(|virtual_file| virtual_file.file()),
        }
    }

    pub(crate) fn definition_at_location(
        &self,
        location: Position,
        db: &RootDatabase,
    ) -> Option<DefLocation> {
        let Some(file) = self.file(db) else {
            return None;
        };

        let index = semantic_index(db, file);
        // let's try and look up the relevant AST node
        let module = parsed_module(db, file);

        // let's figure out the CPosition
        let source = source_text(db, file);
        let li = line_index(db, file);

        let text_size = li.offset(
            // XXX bad
            OneIndexed::from_zero_indexed(location.line as usize),
            OneIndexed::from_zero_indexed(location.character as usize),
            &source,
        );

        let cpos = CPosition(text_size.to_u32().into());
        eprintln!("Looking at offset {}", cpos.0);

        let found_dlike =
            module
                .syntax()
                .locate_def(&CPosition(text_size.to_u32().into()), index, db, file);
        eprintln!("FOUND DLIKE {:?}", found_dlike);
        match found_dlike {
            None => None::<Option<DefLocation>>,
            Some(dl) => {
                // TODO figure out the rest of this
                return None;
            }
        };
        log_err_msg!("Tried lookup");
        todo!();
    }
}

#[derive(Debug)]
pub(crate) enum DefLocation {
    Location { file: File, pos: Position },
    Todo { s: String },
}

pub(crate) enum DefinitionLike {
    Name(Identifier),
}

// this is a position as number of characters from the start
pub struct CPosition(u64);

impl From<Position> for CPosition {
    fn from(_value: Position) -> Self {
        todo!()
    }
}

impl CPosition {
    fn in_range(&self, range: &TextRange) -> bool {
        return (u64::from(range.start().to_u32()) <= self.0)
            && (u64::from(range.end().to_u32()) >= self.0);
    }
}
trait CanLocate<'db> {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db RootDatabase,
        file: File,
    ) -> Option<DefLocation>;
}

impl<'db> CanLocate<'db> for Stmt {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            Stmt::FunctionDef(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::ClassDef(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Return(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Delete(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Assign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::AugAssign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::AnnAssign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::TypeAlias(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::For(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::While(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::If(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::With(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Match(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Raise(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Try(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Assert(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Import(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::ImportFrom(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Global(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Nonlocal(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Expr(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => None,
        }
    }
}

impl<'db, T> CanLocate<'db> for Vec<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for item in self {
            let lookup = item.locate_def(cpos, index, db, file);
            if lookup.is_some() {
                return lookup;
            }
        }
        return None;
    }
}
// XXX can merge Vec and [T] into something else?
impl<'db, T> CanLocate<'db> for [T]
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for item in self {
            let lookup = item.locate_def(cpos, index, db, file);
            if lookup.is_some() {
                return lookup;
            }
        }
        return None;
    }
}

impl<'db, T> CanLocate<'db> for Box<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        self.as_ref().locate_def(cpos, index, db, file)
    }
}
impl<'db, T> CanLocate<'db> for Option<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            None => None,
            Some(elt) => elt.locate_def(cpos, index, db, file),
        }
    }
}

impl<'db> CanLocate<'db> for Expr {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            Expr::BoolOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Named(inner) => inner.locate_def(cpos, index, db, file),
            Expr::BinOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::UnaryOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Lambda(inner) => inner.locate_def(cpos, index, db, file),
            Expr::If(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Dict(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Set(inner) => inner.locate_def(cpos, index, db, file),
            Expr::ListComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::SetComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::DictComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Generator(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Await(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Yield(inner) => inner.locate_def(cpos, index, db, file),
            Expr::YieldFrom(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Compare(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Call(inner) => inner.locate_def(cpos, index, db, file),
            Expr::FString(inner) => inner.locate_def(cpos, index, db, file),
            Expr::StringLiteral(_) => None,
            Expr::BytesLiteral(_) => None,
            Expr::NumberLiteral(_) => None,
            Expr::BooleanLiteral(_) => None,
            Expr::NoneLiteral(_) => None,
            Expr::EllipsisLiteral(_) => None,
            Expr::Attribute(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Subscript(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Starred(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Name(inner) => inner.locate_def(cpos, index, db, file),
            Expr::List(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Tuple(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Slice(inner) => inner.locate_def(cpos, index, db, file),
            Expr::IpyEscapeCommand(_) => None,
        }
    }
}
macro_rules! impl_can_locate {
    ($type:ty, ranged, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, cpos: &CPosition, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<DefLocation> {
                if !cpos.in_range(&self.range) {
                    return None;
                }
                None
                    $(.or_else(|| self.$field.locate_def(cpos, index, db, file)))+
            }
        }
    };
    // Case where `locate_def` directly forwards to a field.
    ($type:ty, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, cpos: &CPosition, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<DefLocation> {
                None
                    $(.or_else(|| self.$field.locate_def(cpos, index, db, file)))+
            }
        }
    };


}
macro_rules! locate_todo {
    ($type: ty) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(
                &self,
                _cpos: &CPosition,
                _index: &SemanticIndex<'db>,
                _db: &'db dyn Db,
                _file: File,
            ) -> Option<DefLocation> {
                None
            }
        }
    };
}
impl_can_locate!(StmtFor, ranged, target, iter, body, orelse);
impl_can_locate!(StmtDelete, ranged, targets);
impl_can_locate!(DictItem, value);
impl_can_locate!(ModModule, ranged, body);
impl_can_locate!(StmtFunctionDef, ranged, decorator_list, returns, body);
impl_can_locate!(StmtClassDef, ranged, decorator_list, arguments, body);
impl_can_locate!(StmtReturn, ranged, value);
impl_can_locate!(StmtGlobal, ranged, names);
impl_can_locate!(StmtNonlocal, ranged, names);
impl_can_locate!(Arguments, ranged, args, keywords);
impl_can_locate!(Keyword, value);
impl_can_locate!(Decorator, ranged, expression);
impl_can_locate!(ExprBoolOp, values);
impl_can_locate!(ExprNamed, value);
impl_can_locate!(ExprBinOp, left, right);
impl_can_locate!(ExprUnaryOp, ranged, operand);
impl_can_locate!(ExprLambda, ranged, parameters, body);
impl_can_locate!(ExprIf, ranged, test, body, orelse);
impl_can_locate!(ExprDict, ranged, items);
impl_can_locate!(ExprSet, ranged, elts);
impl_can_locate!(ExprListComp, ranged, elt, generators);
impl_can_locate!(ExprSetComp, ranged, elt, generators);
impl_can_locate!(ExprDictComp, ranged, key, value, generators);
impl_can_locate!(ExprGenerator, ranged, elt, generators);
impl_can_locate!(ExprAwait, ranged, value);
impl_can_locate!(ExprYield, ranged, value);
impl_can_locate!(ExprYieldFrom, ranged, value);
impl_can_locate!(ExprCompare, ranged, left, comparators);
impl_can_locate!(ExprCall, ranged, func, arguments);
impl_can_locate!(ExprFString, ranged, value);
impl_can_locate!(FStringExpressionElement, ranged, expression);
impl_can_locate!(Comprehension, ranged, target, iter, ifs);
impl_can_locate!(StmtWhile, ranged, test, body, orelse);
impl_can_locate!(StmtIf, ranged, test, body, elif_else_clauses);
impl_can_locate!(ElifElseClause, ranged, test, body);
impl_can_locate!(StmtWith, ranged, items, body);
impl_can_locate!(WithItem, ranged, context_expr, optional_vars);
impl_can_locate!(StmtMatch, ranged, subject, cases);
impl_can_locate!(StmtAssign, ranged, targets, value);
impl_can_locate!(StmtAugAssign, ranged, target, value);
impl_can_locate!(StmtAnnAssign, ranged, target, annotation, value);
impl_can_locate!(StmtTypeAlias, ranged, name, type_params, value);
impl_can_locate!(TypeParams, ranged, type_params);
impl_can_locate!(MatchCase, ranged, guard, body);
impl_can_locate!(StmtRaise, ranged, exc, cause);
impl_can_locate!(StmtTry, ranged, body, handlers, orelse, finalbody);
impl_can_locate!(StmtAssert, ranged, test, msg);
impl_can_locate!(
    Parameters,
    ranged,
    posonlyargs,
    args,
    vararg,
    kwonlyargs,
    kwarg
);
impl_can_locate!(ParameterWithDefault, ranged, parameter, default);
impl_can_locate!(Parameter, ranged, annotation);
locate_todo!(StmtImport);
locate_todo!(StmtImportFrom);
impl_can_locate!(StmtExpr, ranged, value);
locate_todo!(ExceptHandler);
impl<'db> CanLocate<'db> for TypeParam {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            TypeParam::TypeVar(inner) => inner.locate_def(cpos, index, db, file),
            TypeParam::ParamSpec(inner) => inner.locate_def(cpos, index, db, file),
            TypeParam::TypeVarTuple(inner) => inner.locate_def(cpos, index, db, file),
        }
    }
}

impl_can_locate!(TypeParamTypeVar, ranged, bound, default);
impl_can_locate!(TypeParamParamSpec, ranged, default);
impl_can_locate!(TypeParamTypeVarTuple, ranged, default);
impl<'db> CanLocate<'db> for FStringValue {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for part in self.iter() {
            let result = part.locate_def(cpos, index, db, file);
            if result.is_some() {
                return result;
            }
        }
        return None;
    }
}

impl<'db> CanLocate<'db> for FStringPart {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            FStringPart::Literal(_) => None,
            FStringPart::FString(FString { elements, .. }) => {
                for expression in elements.expressions() {
                    let result = expression.locate_def(cpos, index, db, file);
                    if result.is_some() {
                        return result;
                    }
                }
                None
            }
        }
    }
}

impl<'db> CanLocate<'db> for ExprAttribute {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        Some(DefLocation::Todo {
            s: "Attribute Access!".to_string(),
        })
    }
}

impl<'db> CanLocate<'db> for ExprSubscript {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        Some(DefLocation::Todo {
            s: "Subscript Access!".to_string(),
        })
    }
}

impl<'db> CanLocate<'db> for ExprStarred {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        Some(DefLocation::Todo {
            s: "Startred Access!".to_string(),
        })
    }
}

impl<'db> CanLocate<'db> for ExprName {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        eprintln!("IN EXPRNAME");
        eprintln!("LOOKING AT {:?}", self);

        // now I have the name, let's find everything
        // first, I need a scope.
        let scope_id = index
            .expression_scope_id(&Expr::Name(self.clone()))
            .to_scope_id(db.upcast(), file);

        let inference = todo!(); // infer_scope_types(
                                 //     ,
                                 //     scope_id,
                                 // );
                                 // inference;
        Some(DefLocation::Todo {
            s: "Name Access!".to_string(),
        })
    }
}

impl_can_locate!(ExprList, ranged, elts);
impl_can_locate!(ExprTuple, ranged, elts);
impl_can_locate!(ExprSlice, ranged, lower, upper, step);

impl<'db> CanLocate<'db> for Identifier {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        /// TODO figure this one out
        None
    }
}
