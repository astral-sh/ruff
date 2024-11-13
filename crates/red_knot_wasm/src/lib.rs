use std::any::Any;

use js_sys::Error;
use wasm_bindgen::prelude::*;

use red_knot_workspace::db::{Db, RootDatabase};
use red_knot_workspace::workspace::settings::Configuration;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    DirectoryEntry, GlobError, MemoryFileSystem, Metadata, PatternError, System, SystemPath,
    SystemPathBuf, SystemVirtualPath,
};
use ruff_notebook::Notebook;

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
    db: RootDatabase,
    system: WasmSystem,
}

#[wasm_bindgen]
impl Workspace {
    #[wasm_bindgen(constructor)]
    pub fn new(root: &str, settings: &Settings) -> Result<Workspace, Error> {
        let system = WasmSystem::new(SystemPath::new(root));
        let workspace = WorkspaceMetadata::discover(
            SystemPath::new(root),
            &system,
            Some(&Configuration {
                target_version: Some(settings.target_version.into()),
                ..Configuration::default()
            }),
        )
        .map_err(into_error)?;

        let db = RootDatabase::new(workspace, system.clone()).map_err(into_error)?;

        Ok(Self { db, system })
    }

    #[wasm_bindgen(js_name = "openFile")]
    pub fn open_file(&mut self, path: &str, contents: &str) -> Result<FileHandle, Error> {
        self.system
            .fs
            .write_file(path, contents)
            .map_err(into_error)?;

        let file = system_path_to_file(&self.db, path).expect("File to exist");
        file.sync(&mut self.db);

        self.db.workspace().open_file(&mut self.db, file);

        Ok(FileHandle {
            file,
            path: SystemPath::new(path).to_path_buf(),
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

        file_id.file.sync(&mut self.db);

        Ok(())
    }

    #[wasm_bindgen(js_name = "closeFile")]
    pub fn close_file(&mut self, file_id: &FileHandle) -> Result<(), Error> {
        let file = file_id.file;

        self.db.workspace().close_file(&mut self.db, file);
        self.system
            .fs
            .remove_file(&file_id.path)
            .map_err(into_error)?;

        file.sync(&mut self.db);

        Ok(())
    }

    /// Checks a single file.
    #[wasm_bindgen(js_name = "checkFile")]
    pub fn check_file(&self, file_id: &FileHandle) -> Result<Vec<String>, Error> {
        let result = self.db.check_file(file_id.file).map_err(into_error)?;

        Ok(result
            .into_iter()
            .map(|diagnostic| diagnostic.display(&self.db).to_string())
            .collect())
    }

    /// Checks all open files
    pub fn check(&self) -> Result<Vec<String>, Error> {
        let result = self.db.check().map_err(into_error)?;

        Ok(result
            .into_iter()
            .map(|diagnostic| diagnostic.display(&self.db).to_string())
            .collect())
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
    pub target_version: TargetVersion,
}
#[wasm_bindgen]
impl Settings {
    #[wasm_bindgen(constructor)]
    pub fn new(target_version: TargetVersion) -> Self {
        Self { target_version }
    }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TargetVersion {
    Py37,
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl From<TargetVersion> for red_knot_python_semantic::PythonVersion {
    fn from(value: TargetVersion) -> Self {
        match value {
            TargetVersion::Py37 => Self::PY37,
            TargetVersion::Py38 => Self::PY38,
            TargetVersion::Py39 => Self::PY39,
            TargetVersion::Py310 => Self::PY310,
            TargetVersion::Py311 => Self::PY311,
            TargetVersion::Py312 => Self::PY312,
            TargetVersion::Py313 => Self::PY313,
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
        Ok(self.fs.canonicalize(path))
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
    ) -> Result<ruff_notebook::Notebook, ruff_notebook::NotebookError> {
        Err(ruff_notebook::NotebookError::Io(not_found()))
    }

    fn current_directory(&self) -> &SystemPath {
        self.fs.current_directory()
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}
