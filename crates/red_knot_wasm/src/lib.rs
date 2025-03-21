use std::any::Any;

use js_sys::{Error, JsString};
use red_knot_project::metadata::options::{EnvironmentOptions, Options};
use red_knot_project::metadata::value::RangedValue;
use red_knot_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};
use red_knot_project::ProjectMetadata;
use red_knot_project::{Db, ProjectDatabase};
use ruff_db::diagnostic::{DisplayDiagnosticConfig, OldDiagnosticTrait};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::source::{line_index, source_text};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    CaseSensitivity, DirectoryEntry, GlobError, MemoryFileSystem, Metadata, PatternError, System,
    SystemPath, SystemPathBuf, SystemVirtualPath,
};
use ruff_db::Upcast;
use ruff_notebook::Notebook;
use ruff_source_file::SourceLocation;
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
    system: WasmSystem,
    options: Options,
}

#[wasm_bindgen]
impl Workspace {
    #[wasm_bindgen(constructor)]
    pub fn new(root: &str, settings: &Settings) -> Result<Workspace, Error> {
        let system = WasmSystem::new(SystemPath::new(root));

        let mut workspace =
            ProjectMetadata::discover(SystemPath::new(root), &system).map_err(into_error)?;

        let options = Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(settings.python_version.into())),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        };

        workspace.apply_cli_options(options.clone());

        let db = ProjectDatabase::new(workspace, system.clone()).map_err(into_error)?;

        Ok(Self {
            db,
            system,
            options,
        })
    }

    #[wasm_bindgen(js_name = "openFile")]
    pub fn open_file(&mut self, path: &str, contents: &str) -> Result<FileHandle, Error> {
        let path = SystemPath::new(path);

        self.system
            .fs
            .write_file_all(path, contents)
            .map_err(into_error)?;

        self.db.apply_changes(
            vec![ChangeEvent::Created {
                path: path.to_path_buf(),
                kind: CreatedKind::File,
            }],
            Some(&self.options),
        );

        let file = system_path_to_file(&self.db, path).expect("File to exist");

        self.db.project().open_file(&mut self.db, file);

        Ok(FileHandle {
            file,
            path: path.to_path_buf(),
        })
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
            Some(&self.options),
        );

        Ok(())
    }

    #[wasm_bindgen(js_name = "closeFile")]
    pub fn close_file(&mut self, file_id: &FileHandle) -> Result<(), Error> {
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
            Some(&self.options),
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
}

#[wasm_bindgen]
pub struct Settings {
    pub python_version: PythonVersion,
}
#[wasm_bindgen]
impl Settings {
    #[wasm_bindgen(constructor)]
    pub fn new(python_version: PythonVersion) -> Self {
        Self { python_version }
    }
}

#[wasm_bindgen]
pub struct Diagnostic {
    #[wasm_bindgen(readonly)]
    inner: Box<dyn OldDiagnosticTrait>,
}

#[wasm_bindgen]
impl Diagnostic {
    fn wrap(diagnostic: Box<dyn OldDiagnosticTrait>) -> Self {
        Self { inner: diagnostic }
    }

    #[wasm_bindgen]
    pub fn message(&self) -> JsString {
        JsString::from(&*self.inner.message())
    }

    #[wasm_bindgen]
    pub fn id(&self) -> JsString {
        JsString::from(self.inner.id().to_string())
    }

    #[wasm_bindgen]
    pub fn severity(&self) -> Severity {
        Severity::from(self.inner.severity())
    }

    #[wasm_bindgen]
    pub fn text_range(&self) -> Option<TextRange> {
        self.inner
            .span()
            .and_then(|span| Some(TextRange::from(span.range()?)))
    }

    #[wasm_bindgen]
    pub fn to_range(&self, workspace: &Workspace) -> Option<Range> {
        self.inner.span().and_then(|span| {
            let line_index = line_index(workspace.db.upcast(), span.file());
            let source = source_text(workspace.db.upcast(), span.file());
            let text_range = span.range()?;

            Some(Range {
                start: line_index
                    .source_location(text_range.start(), &source)
                    .into(),
                end: line_index.source_location(text_range.end(), &source).into(),
            })
        })
    }

    #[wasm_bindgen]
    pub fn display(&self, workspace: &Workspace) -> JsString {
        let config = DisplayDiagnosticConfig::default().color(false);
        self.inner
            .display(workspace.db.upcast(), &config)
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

impl From<SourceLocation> for Position {
    fn from(location: SourceLocation) -> Self {
        Self {
            line: location.row.to_zero_indexed(),
            character: location.column.to_zero_indexed(),
        }
    }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

#[wasm_bindgen]
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl From<ruff_db::diagnostic::Severity> for Severity {
    fn from(value: ruff_db::diagnostic::Severity) -> Self {
        match value {
            ruff_db::diagnostic::Severity::Info => Self::Info,
            ruff_db::diagnostic::Severity::Warning => Self::Warning,
            ruff_db::diagnostic::Severity::Error => Self::Error,
            ruff_db::diagnostic::Severity::Fatal => Self::Fatal,
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

#[wasm_bindgen]
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum PythonVersion {
    Py37,
    Py38,
    #[default]
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl From<PythonVersion> for ruff_python_ast::PythonVersion {
    fn from(value: PythonVersion) -> Self {
        match value {
            PythonVersion::Py37 => Self::PY37,
            PythonVersion::Py38 => Self::PY38,
            PythonVersion::Py39 => Self::PY39,
            PythonVersion::Py310 => Self::PY310,
            PythonVersion::Py311 => Self::PY311,
            PythonVersion::Py312 => Self::PY312,
            PythonVersion::Py313 => Self::PY313,
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::PythonVersion;

    #[test]
    fn same_default_as_python_version() {
        assert_eq!(
            ruff_python_ast::PythonVersion::from(PythonVersion::default()),
            ruff_python_ast::PythonVersion::default()
        );
    }
}
