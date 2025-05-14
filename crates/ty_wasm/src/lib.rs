use std::any::Any;

use js_sys::{Error, JsString};
use ruff_db::diagnostic::{self, DisplayDiagnosticConfig};
use ruff_db::files::{system_path_to_file, File, FileRange};
use ruff_db::source::{line_index, source_text};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    CaseSensitivity, DirectoryEntry, GlobError, MemoryFileSystem, Metadata, PatternError, System,
    SystemPath, SystemPathBuf, SystemVirtualPath,
};
use ruff_db::Upcast;
use ruff_notebook::Notebook;
use ruff_python_formatter::formatted_file;
use ruff_source_file::{LineIndex, OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextSize};
use ty_ide::{goto_type_definition, hover, inlay_hints, MarkupKind};
use ty_project::metadata::options::Options;
use ty_project::metadata::value::ValueSource;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};
use ty_project::ProjectMetadata;
use ty_project::{Db, ProjectDatabase};
use ty_python_semantic::Program;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run() {
    use log::Level;

    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    console_log::init_with_level(Level::Debug).expect("Initializing logger went wrong.");
}

#[wasm_bindgen]
pub struct Workspace {
    db: ProjectDatabase,
    position_encoding: PositionEncoding,
    system: WasmSystem,
}

#[wasm_bindgen]
impl Workspace {
    #[wasm_bindgen(constructor)]
    pub fn new(
        root: &str,
        position_encoding: PositionEncoding,
        options: JsValue,
    ) -> Result<Workspace, Error> {
        let options = Options::deserialize_with(
            ValueSource::Cli,
            serde_wasm_bindgen::Deserializer::from(options),
        )
        .map_err(into_error)?;

        let system = WasmSystem::new(SystemPath::new(root));

        let project = ProjectMetadata::from_options(options, SystemPathBuf::from(root), None)
            .map_err(into_error)?;

        let db = ProjectDatabase::new(project, system.clone()).map_err(into_error)?;

        Ok(Self {
            db,
            position_encoding,
            system,
        })
    }

    #[wasm_bindgen(js_name = "updateOptions")]
    pub fn update_options(&mut self, options: JsValue) -> Result<(), Error> {
        let options = Options::deserialize_with(
            ValueSource::Cli,
            serde_wasm_bindgen::Deserializer::from(options),
        )
        .map_err(into_error)?;

        let project = ProjectMetadata::from_options(
            options,
            self.db.project().root(&self.db).to_path_buf(),
            None,
        )
        .map_err(into_error)?;

        let program_settings = project.to_program_settings(&self.system);
        Program::get(&self.db)
            .update_from_settings(&mut self.db, program_settings)
            .map_err(into_error)?;

        self.db.project().reload(&mut self.db, project);

        Ok(())
    }

    #[wasm_bindgen(js_name = "openFile")]
    pub fn open_file(&mut self, path: &str, contents: &str) -> Result<FileHandle, Error> {
        let path = SystemPath::absolute(path, self.db.project().root(&self.db));

        self.system
            .fs
            .write_file_all(&path, contents)
            .map_err(into_error)?;

        self.db.apply_changes(
            vec![ChangeEvent::Created {
                path: path.clone(),
                kind: CreatedKind::File,
            }],
            None,
        );

        let file = system_path_to_file(&self.db, &path).expect("File to exist");

        self.db.project().open_file(&mut self.db, file);

        Ok(FileHandle { path, file })
    }

    #[wasm_bindgen(js_name = "updateFile")]
    pub fn update_file(&mut self, file_id: &FileHandle, contents: &str) -> Result<(), Error> {
        if !self.system.fs.exists(&file_id.path) {
            return Err(Error::new("File does not exist"));
        }

        self.system
            .fs
            .write_file(&file_id.path, contents)
            .map_err(into_error)?;

        self.db.apply_changes(
            vec![
                ChangeEvent::Changed {
                    path: file_id.path.to_path_buf(),
                    kind: ChangedKind::FileContent,
                },
                ChangeEvent::Changed {
                    path: file_id.path.to_path_buf(),
                    kind: ChangedKind::FileMetadata,
                },
            ],
            None,
        );

        Ok(())
    }

    #[wasm_bindgen(js_name = "closeFile")]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "It's intentional that the file handle is consumed because it is no longer valid after closing"
    )]
    pub fn close_file(&mut self, file_id: FileHandle) -> Result<(), Error> {
        let file = file_id.file;

        self.db.project().close_file(&mut self.db, file);
        self.system
            .fs
            .remove_file(&file_id.path)
            .map_err(into_error)?;

        self.db.apply_changes(
            vec![ChangeEvent::Deleted {
                path: file_id.path.to_path_buf(),
                kind: DeletedKind::File,
            }],
            None,
        );

        Ok(())
    }

    /// Checks a single file.
    #[wasm_bindgen(js_name = "checkFile")]
    pub fn check_file(&self, file_id: &FileHandle) -> Result<Vec<Diagnostic>, Error> {
        let result = self.db.check_file(file_id.file).map_err(into_error)?;

        Ok(result.into_iter().map(Diagnostic::wrap).collect())
    }

    /// Checks all open files
    pub fn check(&self) -> Result<Vec<Diagnostic>, Error> {
        let result = self.db.check().map_err(into_error)?;

        Ok(result.into_iter().map(Diagnostic::wrap).collect())
    }

    /// Returns the parsed AST for `path`
    pub fn parsed(&self, file_id: &FileHandle) -> Result<String, Error> {
        let parsed = ruff_db::parsed::parsed_module(&self.db, file_id.file);

        Ok(format!("{:#?}", parsed.syntax()))
    }

    pub fn format(&self, file_id: &FileHandle) -> Result<Option<String>, Error> {
        formatted_file(&self.db, file_id.file).map_err(into_error)
    }

    /// Returns the token stream for `path` serialized as a string.
    pub fn tokens(&self, file_id: &FileHandle) -> Result<String, Error> {
        let parsed = ruff_db::parsed::parsed_module(&self.db, file_id.file);

        Ok(format!("{:#?}", parsed.tokens()))
    }

    #[wasm_bindgen(js_name = "sourceText")]
    pub fn source_text(&self, file_id: &FileHandle) -> Result<String, Error> {
        let source_text = ruff_db::source::source_text(&self.db, file_id.file);

        Ok(source_text.to_string())
    }

    #[wasm_bindgen(js_name = "gotoTypeDefinition")]
    pub fn goto_type_definition(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<LocationLink>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(targets) = goto_type_definition(&self.db, file_id.file, offset) else {
            return Ok(Vec::new());
        };

        let source_range = Range::from_text_range(
            targets.file_range().range(),
            &index,
            &source,
            self.position_encoding,
        );

        let links: Vec<_> = targets
            .into_iter()
            .map(|target| LocationLink {
                path: target.file().path(&self.db).to_string(),
                full_range: Range::from_file_range(
                    &self.db,
                    FileRange::new(target.file(), target.full_range()),
                    self.position_encoding,
                ),
                selection_range: Some(Range::from_file_range(
                    &self.db,
                    FileRange::new(target.file(), target.focus_range()),
                    self.position_encoding,
                )),
                origin_selection_range: Some(source_range),
            })
            .collect();

        Ok(links)
    }

    #[wasm_bindgen]
    pub fn hover(&self, file_id: &FileHandle, position: Position) -> Result<Option<Hover>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(range_info) = hover(&self.db, file_id.file, offset) else {
            return Ok(None);
        };

        let source_range = Range::from_text_range(
            range_info.file_range().range(),
            &index,
            &source,
            self.position_encoding,
        );

        Ok(Some(Hover {
            markdown: range_info
                .display(&self.db, MarkupKind::Markdown)
                .to_string(),
            range: source_range,
        }))
    }

    #[wasm_bindgen(js_name = "inlayHints")]
    pub fn inlay_hints(&self, file_id: &FileHandle, range: Range) -> Result<Vec<InlayHint>, Error> {
        let index = line_index(&self.db, file_id.file);
        let source = source_text(&self.db, file_id.file);

        let result = inlay_hints(
            &self.db,
            file_id.file,
            range.to_text_range(&index, &source, self.position_encoding)?,
        );

        Ok(result
            .into_iter()
            .map(|hint| InlayHint {
                markdown: hint.display(&self.db).to_string(),
                position: Position::from_text_size(
                    hint.position,
                    &index,
                    &source,
                    self.position_encoding,
                ),
            })
            .collect())
    }
}

pub(crate) fn into_error<E: std::fmt::Display>(err: E) -> Error {
    Error::new(&err.to_string())
}

#[derive(Debug, Eq, PartialEq)]
#[wasm_bindgen(inspectable)]
pub struct FileHandle {
    path: SystemPathBuf,
    file: File,
}

#[wasm_bindgen]
impl FileHandle {
    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        format!("file(id: {:?}, path: {})", self.file, self.path)
    }

    pub fn path(&self) -> String {
        self.path.to_string()
    }
}

#[wasm_bindgen]
pub struct Diagnostic {
    #[wasm_bindgen(readonly)]
    inner: diagnostic::Diagnostic,
}

#[wasm_bindgen]
impl Diagnostic {
    fn wrap(diagnostic: diagnostic::Diagnostic) -> Self {
        Self { inner: diagnostic }
    }

    #[wasm_bindgen]
    pub fn message(&self) -> JsString {
        JsString::from(self.inner.concise_message().to_string())
    }

    #[wasm_bindgen]
    pub fn id(&self) -> JsString {
        JsString::from(self.inner.id().to_string())
    }

    #[wasm_bindgen]
    pub fn severity(&self) -> Severity {
        Severity::from(self.inner.severity())
    }

    #[wasm_bindgen(js_name = "textRange")]
    pub fn text_range(&self) -> Option<TextRange> {
        self.inner
            .primary_span()
            .and_then(|span| Some(TextRange::from(span.range()?)))
    }

    #[wasm_bindgen(js_name = "toRange")]
    pub fn to_range(&self, workspace: &Workspace) -> Option<Range> {
        self.inner.primary_span().and_then(|span| {
            Some(Range::from_file_range(
                &workspace.db,
                FileRange::new(span.expect_ty_file(), span.range()?),
                workspace.position_encoding,
            ))
        })
    }

    #[wasm_bindgen]
    pub fn display(&self, workspace: &Workspace) -> JsString {
        let config = DisplayDiagnosticConfig::default().color(false);
        self.inner
            .display(&workspace.db.upcast(), &config)
            .to_string()
            .into()
    }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[wasm_bindgen]
impl Range {
    #[wasm_bindgen(constructor)]
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

impl Range {
    fn from_file_range(
        db: &dyn Db,
        file_range: FileRange,
        position_encoding: PositionEncoding,
    ) -> Self {
        let index = line_index(db.upcast(), file_range.file());
        let source = source_text(db.upcast(), file_range.file());

        Self::from_text_range(file_range.range(), &index, &source, position_encoding)
    }

    fn from_text_range(
        text_range: ruff_text_size::TextRange,
        line_index: &LineIndex,
        source: &str,
        position_encoding: PositionEncoding,
    ) -> Self {
        Self {
            start: Position::from_text_size(
                text_range.start(),
                line_index,
                source,
                position_encoding,
            ),
            end: Position::from_text_size(text_range.end(), line_index, source, position_encoding),
        }
    }

    fn to_text_range(
        self,
        line_index: &LineIndex,
        source: &str,
        position_encoding: PositionEncoding,
    ) -> Result<ruff_text_size::TextRange, Error> {
        let start = self
            .start
            .to_text_size(source, line_index, position_encoding)?;
        let end = self
            .end
            .to_text_size(source, line_index, position_encoding)?;

        Ok(ruff_text_size::TextRange::new(start, end))
    }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Position {
    /// One indexed line number
    pub line: usize,

    /// One indexed column number (the nth character on the line)
    pub column: usize,
}

#[wasm_bindgen]
impl Position {
    #[wasm_bindgen(constructor)]
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

impl Position {
    fn to_text_size(
        self,
        text: &str,
        index: &LineIndex,
        position_encoding: PositionEncoding,
    ) -> Result<TextSize, Error> {
        let text_size = index.offset(
            SourceLocation {
                line: OneIndexed::new(self.line).ok_or_else(|| {
                    Error::new(
                        "Invalid value `0` for `position.line`. The line index is 1-indexed.",
                    )
                })?,
                character_offset: OneIndexed::new(self.column).ok_or_else(|| {
                    Error::new(
                        "Invalid value `0` for `position.column`. The column index is 1-indexed.",
                    )
                })?,
            },
            text,
            position_encoding.into(),
        );

        Ok(text_size)
    }

    fn from_text_size(
        offset: TextSize,
        line_index: &LineIndex,
        source: &str,
        position_encoding: PositionEncoding,
    ) -> Self {
        let location = line_index.source_location(offset, source, position_encoding.into());
        Self {
            line: location.line.get(),
            column: location.character_offset.get(),
        }
    }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl From<diagnostic::Severity> for Severity {
    fn from(value: diagnostic::Severity) -> Self {
        match value {
            diagnostic::Severity::Info => Self::Info,
            diagnostic::Severity::Warning => Self::Warning,
            diagnostic::Severity::Error => Self::Error,
            diagnostic::Severity::Fatal => Self::Fatal,
        }
    }
}

#[wasm_bindgen]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

impl From<ruff_text_size::TextRange> for TextRange {
    fn from(value: ruff_text_size::TextRange) -> Self {
        Self {
            start: value.start().into(),
            end: value.end().into(),
        }
    }
}

#[derive(Default, Copy, Clone)]
#[wasm_bindgen]
pub enum PositionEncoding {
    #[default]
    Utf8,
    Utf16,
    Utf32,
}

impl From<PositionEncoding> for ruff_source_file::PositionEncoding {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::Utf8 => Self::Utf8,
            PositionEncoding::Utf16 => Self::Utf16,
            PositionEncoding::Utf32 => Self::Utf32,
        }
    }
}

#[wasm_bindgen]
pub struct LocationLink {
    /// The target file path
    #[wasm_bindgen(getter_with_clone)]
    pub path: String,

    /// The full range of the target
    pub full_range: Range,
    /// The target's range that should be selected/highlighted
    pub selection_range: Option<Range>,
    /// The range of the origin.
    pub origin_selection_range: Option<Range>,
}

#[wasm_bindgen]
pub struct Hover {
    #[wasm_bindgen(getter_with_clone)]
    pub markdown: String,

    pub range: Range,
}

#[wasm_bindgen]
pub struct InlayHint {
    #[wasm_bindgen(getter_with_clone)]
    pub markdown: String,

    pub position: Position,
}

#[derive(Debug, Clone)]
struct WasmSystem {
    fs: MemoryFileSystem,
}

impl WasmSystem {
    fn new(root: &SystemPath) -> Self {
        Self {
            fs: MemoryFileSystem::with_current_directory(root),
        }
    }
}

impl System for WasmSystem {
    fn path_metadata(&self, path: &SystemPath) -> ruff_db::system::Result<Metadata> {
        self.fs.metadata(path)
    }

    fn canonicalize_path(&self, path: &SystemPath) -> ruff_db::system::Result<SystemPathBuf> {
        self.fs.canonicalize(path)
    }

    fn read_to_string(&self, path: &SystemPath) -> ruff_db::system::Result<String> {
        self.fs.read_to_string(path)
    }

    fn read_to_notebook(
        &self,
        path: &SystemPath,
    ) -> Result<ruff_notebook::Notebook, ruff_notebook::NotebookError> {
        let content = self.read_to_string(path)?;
        Notebook::from_source_code(&content)
    }

    fn read_virtual_path_to_string(
        &self,
        _path: &SystemVirtualPath,
    ) -> ruff_db::system::Result<String> {
        Err(not_found())
    }

    fn read_virtual_path_to_notebook(
        &self,
        _path: &SystemVirtualPath,
    ) -> Result<Notebook, ruff_notebook::NotebookError> {
        Err(ruff_notebook::NotebookError::Io(not_found()))
    }

    fn path_exists_case_sensitive(&self, path: &SystemPath, _prefix: &SystemPath) -> bool {
        self.path_exists(path)
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        CaseSensitivity::CaseSensitive
    }

    fn current_directory(&self) -> &SystemPath {
        self.fs.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        None
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> ruff_db::system::Result<
        Box<dyn Iterator<Item = ruff_db::system::Result<DirectoryEntry>> + 'a>,
    > {
        Ok(Box::new(self.fs.read_directory(path)?))
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.fs.walk_directory(path)
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<SystemPathBuf, GlobError>>>, PatternError> {
        Ok(Box::new(self.fs.glob(pattern)?))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}
