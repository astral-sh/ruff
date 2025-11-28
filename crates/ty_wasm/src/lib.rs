use std::any::Any;

use js_sys::{Error, JsString};
use ruff_db::Db as _;
use ruff_db::diagnostic::{self, DisplayDiagnosticConfig};
use ruff_db::files::{File, FilePath, FileRange, system_path_to_file, vendored_path_to_file};
use ruff_db::source::{SourceText, line_index, source_text};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    CaseSensitivity, DirectoryEntry, GlobError, MemoryFileSystem, Metadata, PatternError, System,
    SystemPath, SystemPathBuf, SystemVirtualPath, WritableSystem,
};
use ruff_db::vendored::VendoredPath;
use ruff_diagnostics::{Applicability, Edit};
use ruff_notebook::Notebook;
use ruff_python_formatter::formatted_file;
use ruff_source_file::{LineIndex, OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextSize};
use ty_ide::{
    InlayHintSettings, MarkupKind, RangedValue, document_highlights, find_references,
    goto_declaration, goto_definition, goto_type_definition, hover, inlay_hints,
};
use ty_ide::{NavigationTarget, NavigationTargets, signature_help};
use ty_project::metadata::options::Options;
use ty_project::metadata::value::ValueSource;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};
use ty_project::{CheckMode, ProjectMetadata};
use ty_project::{Db, ProjectDatabase};
use ty_python_semantic::Program;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn version() -> String {
    option_env!("TY_WASM_COMMIT_SHORT_HASH")
        .or_else(|| option_env!("CARGO_PKG_VERSION"))
        .unwrap_or("unknown")
        .to_string()
}

/// Perform global constructor initialization.
#[cfg(target_family = "wasm")]
#[expect(unsafe_code)]
pub fn before_main() {
    unsafe extern "C" {
        fn __wasm_call_ctors();
    }

    // Salsa uses the `inventory` crate, which registers global constructors that may need to be
    // called explicitly on WASM. See <https://github.com/dtolnay/inventory/blob/master/src/lib.rs#L105>
    // for details.
    unsafe {
        __wasm_call_ctors();
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn before_main() {}

#[wasm_bindgen(start)]
pub fn run() {
    use log::Level;

    before_main();

    ruff_db::set_program_version(version()).unwrap();

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

        let mut db = ProjectDatabase::new(project, system.clone()).map_err(into_error)?;

        // By default, it will check all files in the project but we only want to check the open
        // files in the playground.
        db.set_check_mode(CheckMode::OpenFiles);

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

        let program_settings = project
            .to_program_settings(&self.system, self.db.vendored())
            .map_err(into_error)?;
        Program::get(&self.db).update_from_settings(&mut self.db, program_settings);

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

        Ok(FileHandle {
            path: path.into(),
            file,
        })
    }

    #[wasm_bindgen(js_name = "updateFile")]
    pub fn update_file(&mut self, file_id: &FileHandle, contents: &str) -> Result<(), Error> {
        let system_path = file_id.path.as_system_path().ok_or_else(|| {
            Error::new("Cannot update non-system files (vendored files are read-only)")
        })?;

        if !self.system.fs.exists(system_path) {
            return Err(Error::new("File does not exist"));
        }

        self.system
            .fs
            .write_file(system_path, contents)
            .map_err(into_error)?;

        self.db.apply_changes(
            vec![
                ChangeEvent::Changed {
                    path: system_path.to_path_buf(),
                    kind: ChangedKind::FileContent,
                },
                ChangeEvent::Changed {
                    path: system_path.to_path_buf(),
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

        // Only close system files (vendored files can't be closed/deleted)
        if let Some(system_path) = file_id.path.as_system_path() {
            self.system
                .fs
                .remove_file(system_path)
                .map_err(into_error)?;

            self.db.apply_changes(
                vec![ChangeEvent::Deleted {
                    path: system_path.to_path_buf(),
                    kind: DeletedKind::File,
                }],
                None,
            );
        }

        Ok(())
    }

    /// Checks a single file.
    #[wasm_bindgen(js_name = "checkFile")]
    pub fn check_file(&self, file_id: &FileHandle) -> Result<Vec<Diagnostic>, Error> {
        let result = self.db.check_file(file_id.file);

        Ok(result.into_iter().map(Diagnostic::wrap).collect())
    }

    /// Checks all open files
    pub fn check(&self) -> Result<Vec<Diagnostic>, Error> {
        let result = self.db.check();

        Ok(result.into_iter().map(Diagnostic::wrap).collect())
    }

    /// Returns the parsed AST for `path`
    pub fn parsed(&self, file_id: &FileHandle) -> Result<String, Error> {
        let parsed = ruff_db::parsed::parsed_module(&self.db, file_id.file).load(&self.db);

        Ok(format!("{:#?}", parsed.syntax()))
    }

    pub fn format(&self, file_id: &FileHandle) -> Result<Option<String>, Error> {
        formatted_file(&self.db, file_id.file).map_err(into_error)
    }

    /// Returns the token stream for `path` serialized as a string.
    pub fn tokens(&self, file_id: &FileHandle) -> Result<String, Error> {
        let parsed = ruff_db::parsed::parsed_module(&self.db, file_id.file).load(&self.db);

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

        Ok(map_targets_to_links(
            &self.db,
            targets,
            &source,
            &index,
            self.position_encoding,
        ))
    }

    #[wasm_bindgen(js_name = "gotoDeclaration")]
    pub fn goto_declaration(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<LocationLink>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(targets) = goto_declaration(&self.db, file_id.file, offset) else {
            return Ok(Vec::new());
        };

        Ok(map_targets_to_links(
            &self.db,
            targets,
            &source,
            &index,
            self.position_encoding,
        ))
    }

    #[wasm_bindgen(js_name = "gotoDefinition")]
    pub fn goto_definition(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<LocationLink>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(targets) = goto_definition(&self.db, file_id.file, offset) else {
            return Ok(Vec::new());
        };

        Ok(map_targets_to_links(
            &self.db,
            targets,
            &source,
            &index,
            self.position_encoding,
        ))
    }

    #[wasm_bindgen(js_name = "gotoReferences")]
    pub fn goto_references(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<LocationLink>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(targets) = find_references(&self.db, file_id.file, offset, true) else {
            return Ok(Vec::new());
        };

        Ok(targets
            .into_iter()
            .map(|target| LocationLink {
                path: target.file().path(&self.db).to_string(),
                full_range: Range::from_file_range(
                    &self.db,
                    target.file_range(),
                    self.position_encoding,
                ),
                selection_range: Some(Range::from_file_range(
                    &self.db,
                    target.file_range(),
                    self.position_encoding,
                )),
                origin_selection_range: Some(Range::from_text_range(
                    ruff_text_size::TextRange::new(offset, offset),
                    &index,
                    &source,
                    self.position_encoding,
                )),
            })
            .collect())
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

    #[wasm_bindgen]
    pub fn completions(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<Completion>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let settings = ty_ide::CompletionSettings { auto_import: true };
        let completions = ty_ide::completion(&self.db, &settings, file_id.file, offset);

        Ok(completions
            .into_iter()
            .map(|comp| {
                let kind = comp.kind(&self.db).map(CompletionKind::from);
                let type_display = comp.ty.map(|ty| ty.display(&self.db).to_string());
                let import_edit = comp.import.as_ref().map(|edit| {
                    let range = Range::from_text_range(
                        edit.range(),
                        &index,
                        &source,
                        self.position_encoding,
                    );
                    TextEdit {
                        range,
                        new_text: edit.content().map(ToString::to_string).unwrap_or_default(),
                    }
                });
                Completion {
                    name: comp.name.into(),
                    kind,
                    detail: type_display,
                    module_name: comp.module_name.map(ToString::to_string),
                    insert_text: comp.insert.map(String::from),
                    additional_text_edits: import_edit.map(|edit| vec![edit]),
                    documentation: comp
                        .documentation
                        .map(|docstring| docstring.render_plaintext()),
                }
            })
            .collect())
    }

    #[wasm_bindgen(js_name = "inlayHints")]
    pub fn inlay_hints(&self, file_id: &FileHandle, range: Range) -> Result<Vec<InlayHint>, Error> {
        let index = line_index(&self.db, file_id.file);
        let source = source_text(&self.db, file_id.file);

        let result = inlay_hints(
            &self.db,
            file_id.file,
            range.to_text_range(&index, &source, self.position_encoding)?,
            // TODO: Provide a way to configure this
            &InlayHintSettings {
                variable_types: true,
                call_argument_names: true,
            },
        );

        Ok(result
            .into_iter()
            .map(|hint| InlayHint {
                label: hint
                    .label
                    .into_parts()
                    .into_iter()
                    .map(|part| InlayHintLabelPart {
                        location: part.target().map(|target| {
                            location_link_from_navigation_target(
                                target,
                                &self.db,
                                self.position_encoding,
                                None,
                            )
                        }),
                        label: part.into_text(),
                    })
                    .collect(),
                position: Position::from_text_size(
                    hint.position,
                    &index,
                    &source,
                    self.position_encoding,
                ),
                kind: hint.kind.into(),
                text_edits: hint
                    .text_edits
                    .into_iter()
                    .map(|edit| TextEdit {
                        range: Range::from_text_range(
                            edit.range,
                            &index,
                            &source,
                            self.position_encoding,
                        ),
                        new_text: edit.new_text,
                    })
                    .collect(),
            })
            .collect())
    }

    #[wasm_bindgen(js_name = "semanticTokens")]
    pub fn semantic_tokens(&self, file_id: &FileHandle) -> Result<Vec<SemanticToken>, Error> {
        let index = line_index(&self.db, file_id.file);
        let source = source_text(&self.db, file_id.file);

        let semantic_token = ty_ide::semantic_tokens(&self.db, file_id.file, None);

        let result = semantic_token
            .iter()
            .map(|token| SemanticToken {
                kind: token.token_type.into(),
                modifiers: token.modifiers.bits(),
                range: Range::from_text_range(token.range, &index, &source, self.position_encoding),
            })
            .collect::<Vec<_>>();

        Ok(result)
    }

    #[wasm_bindgen(js_name = "semanticTokensInRange")]
    pub fn semantic_tokens_in_range(
        &self,
        file_id: &FileHandle,
        range: Range,
    ) -> Result<Vec<SemanticToken>, Error> {
        let index = line_index(&self.db, file_id.file);
        let source = source_text(&self.db, file_id.file);

        let semantic_token = ty_ide::semantic_tokens(
            &self.db,
            file_id.file,
            Some(range.to_text_range(&index, &source, self.position_encoding)?),
        );

        let result = semantic_token
            .iter()
            .map(|token| SemanticToken {
                kind: token.token_type.into(),
                modifiers: token.modifiers.bits(),
                range: Range::from_text_range(token.range, &index, &source, self.position_encoding),
            })
            .collect::<Vec<_>>();

        Ok(result)
    }

    #[wasm_bindgen(js_name = "codeActions")]
    pub fn code_actions(
        &self,
        file_id: &FileHandle,
        diagnostic: &Diagnostic,
    ) -> Option<Vec<CodeAction>> {
        // If the diagnostic includes fixes, offer those up as options.
        let mut actions = Vec::new();
        if let Some(action) = diagnostic.code_action(self) {
            actions.push(action);
        }

        // Try to find other applicable actions.
        //
        // This is only for actions that are messy to compute at the time of the diagnostic.
        // For instance, suggesting imports requires finding symbols for the entire project,
        // which is dubious when you're in the middle of resolving symbols.
        if let Some(range) = diagnostic.inner.range() {
            actions.extend(
                ty_ide::code_actions(
                    &self.db,
                    file_id.file,
                    range,
                    diagnostic.inner.id().as_str(),
                )
                .into_iter()
                .map(|action| CodeAction {
                    title: action.title,
                    preferred: action.preferred,
                    edits: action
                        .edits
                        .into_iter()
                        .map(|edit| edit_to_text_edit(self, file_id.file, &edit))
                        .collect(),
                }),
            );
        }

        if actions.is_empty() {
            None
        } else {
            Some(actions)
        }
    }

    #[wasm_bindgen(js_name = "signatureHelp")]
    pub fn signature_help(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Option<SignatureHelp>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(signature_help_info) = signature_help(&self.db, file_id.file, offset) else {
            return Ok(None);
        };

        let signatures = signature_help_info
            .signatures
            .into_iter()
            .map(|sig| {
                let parameters = sig
                    .parameters
                    .into_iter()
                    .map(|param| ParameterInformation {
                        label: param.label,
                        documentation: param.documentation,
                    })
                    .collect();

                SignatureInformation {
                    label: sig.label,
                    documentation: sig
                        .documentation
                        .map(|docstring| docstring.render_plaintext()),
                    parameters,
                    active_parameter: sig.active_parameter.and_then(|p| u32::try_from(p).ok()),
                }
            })
            .collect();

        Ok(Some(SignatureHelp {
            signatures,
            active_signature: signature_help_info
                .active_signature
                .and_then(|s| u32::try_from(s).ok()),
        }))
    }

    #[wasm_bindgen(js_name = "documentHighlights")]
    pub fn document_highlights(
        &self,
        file_id: &FileHandle,
        position: Position,
    ) -> Result<Vec<DocumentHighlight>, Error> {
        let source = source_text(&self.db, file_id.file);
        let index = line_index(&self.db, file_id.file);

        let offset = position.to_text_size(&source, &index, self.position_encoding)?;

        let Some(targets) = document_highlights(&self.db, file_id.file, offset) else {
            return Ok(Vec::new());
        };

        Ok(targets
            .into_iter()
            .map(|target| DocumentHighlight {
                range: Range::from_file_range(
                    &self.db,
                    target.file_range(),
                    self.position_encoding,
                ),
                kind: target.kind().into(),
            })
            .collect())
    }

    /// Gets a file handle for a vendored file by its path.
    /// This allows vendored files to participate in LSP features like hover, completions, etc.
    #[wasm_bindgen(js_name = "getVendoredFile")]
    pub fn get_vendored_file(&self, path: &str) -> Result<FileHandle, Error> {
        let vendored_path = VendoredPath::new(path);

        // Try to get the vendored file as a File
        let file = vendored_path_to_file(&self.db, vendored_path)
            .map_err(|err| Error::new(&format!("Vendored file not found: {path}: {err}")))?;

        Ok(FileHandle {
            file,
            path: vendored_path.to_path_buf().into(),
        })
    }
}

pub(crate) fn into_error<E: std::fmt::Display>(err: E) -> Error {
    Error::new(&err.to_string())
}

fn map_targets_to_links(
    db: &dyn Db,
    targets: RangedValue<NavigationTargets>,
    source: &SourceText,
    index: &LineIndex,
    position_encoding: PositionEncoding,
) -> Vec<LocationLink> {
    let source_range = Range::from_text_range(
        targets.file_range().range(),
        index,
        source,
        position_encoding,
    );

    targets
        .into_iter()
        .map(|target| {
            location_link_from_navigation_target(&target, db, position_encoding, Some(source_range))
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
#[wasm_bindgen(inspectable)]
pub struct FileHandle {
    path: FilePath,
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
            .display(&workspace.db, &config)
            .to_string()
            .into()
    }

    /// Returns the code action for this diagnostic, if it has a fix.
    #[wasm_bindgen(js_name = "codeAction")]
    pub fn code_action(&self, workspace: &Workspace) -> Option<CodeAction> {
        let fix = self
            .inner
            .fix()
            .filter(|fix| fix.applies(Applicability::Unsafe))?;

        let primary_span = self.inner.primary_span()?;
        let file = primary_span.expect_ty_file();

        let edits: Vec<TextEdit> = fix
            .edits()
            .iter()
            .map(|edit| edit_to_text_edit(workspace, file, edit))
            .collect();

        let title = self
            .inner
            .first_help_text()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Fix {}", self.inner.id()));

        Some(CodeAction {
            title,
            edits,
            preferred: true,
        })
    }
}

fn edit_to_text_edit(workspace: &Workspace, file: File, edit: &Edit) -> TextEdit {
    let source = source_text(&workspace.db, file);
    let index = line_index(&workspace.db, file);

    TextEdit {
        range: Range::from_text_range(edit.range(), &index, &source, workspace.position_encoding),
        new_text: edit.content().unwrap_or_default().to_string(),
    }
}

/// A code action that can be applied to fix a diagnostic.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAction {
    #[wasm_bindgen(getter_with_clone)]
    pub title: String,
    #[wasm_bindgen(getter_with_clone)]
    pub edits: Vec<TextEdit>,
    pub preferred: bool,
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
        let index = line_index(db, file_range.file());
        let source = source_text(db, file_range.file());

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
#[derive(Clone)]
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

fn location_link_from_navigation_target(
    target: &NavigationTarget,
    db: &dyn Db,
    position_encoding: PositionEncoding,
    source_range: Option<Range>,
) -> LocationLink {
    LocationLink {
        path: target.file().path(db).to_string(),
        full_range: Range::from_file_range(db, target.full_file_range(), position_encoding),
        selection_range: Some(Range::from_file_range(
            db,
            FileRange::new(target.file(), target.focus_range()),
            position_encoding,
        )),
        origin_selection_range: source_range,
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    #[wasm_bindgen(getter_with_clone)]
    pub markdown: String,

    pub range: Range,
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    pub kind: Option<CompletionKind>,
    #[wasm_bindgen(getter_with_clone)]
    pub insert_text: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub additional_text_edits: Option<Vec<TextEdit>>,
    #[wasm_bindgen(getter_with_clone)]
    pub documentation: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub detail: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub module_name: Option<String>,
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl From<ty_ide::CompletionKind> for CompletionKind {
    fn from(value: ty_ide::CompletionKind) -> Self {
        match value {
            ty_ide::CompletionKind::Text => Self::Text,
            ty_ide::CompletionKind::Method => Self::Method,
            ty_ide::CompletionKind::Function => Self::Function,
            ty_ide::CompletionKind::Constructor => Self::Constructor,
            ty_ide::CompletionKind::Field => Self::Field,
            ty_ide::CompletionKind::Variable => Self::Variable,
            ty_ide::CompletionKind::Class => Self::Class,
            ty_ide::CompletionKind::Interface => Self::Interface,
            ty_ide::CompletionKind::Module => Self::Module,
            ty_ide::CompletionKind::Property => Self::Property,
            ty_ide::CompletionKind::Unit => Self::Unit,
            ty_ide::CompletionKind::Value => Self::Value,
            ty_ide::CompletionKind::Enum => Self::Enum,
            ty_ide::CompletionKind::Keyword => Self::Keyword,
            ty_ide::CompletionKind::Snippet => Self::Snippet,
            ty_ide::CompletionKind::Color => Self::Color,
            ty_ide::CompletionKind::File => Self::File,
            ty_ide::CompletionKind::Reference => Self::Reference,
            ty_ide::CompletionKind::Folder => Self::Folder,
            ty_ide::CompletionKind::EnumMember => Self::EnumMember,
            ty_ide::CompletionKind::Constant => Self::Constant,
            ty_ide::CompletionKind::Struct => Self::Struct,
            ty_ide::CompletionKind::Event => Self::Event,
            ty_ide::CompletionKind::Operator => Self::Operator,
            ty_ide::CompletionKind::TypeParameter => Self::TypeParameter,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range,
    #[wasm_bindgen(getter_with_clone)]
    pub new_text: String,
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum InlayHintKind {
    Type,
    Parameter,
}

impl From<ty_ide::InlayHintKind> for InlayHintKind {
    fn from(kind: ty_ide::InlayHintKind) -> Self {
        match kind {
            ty_ide::InlayHintKind::Type => Self::Type,
            ty_ide::InlayHintKind::CallArgumentName => Self::Parameter,
        }
    }
}

#[wasm_bindgen]
pub struct InlayHint {
    #[wasm_bindgen(getter_with_clone)]
    pub label: Vec<InlayHintLabelPart>,

    pub position: Position,

    pub kind: InlayHintKind,

    #[wasm_bindgen(getter_with_clone)]
    pub text_edits: Vec<TextEdit>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct InlayHintLabelPart {
    #[wasm_bindgen(getter_with_clone)]
    pub label: String,

    #[wasm_bindgen(getter_with_clone)]
    pub location: Option<LocationLink>,
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub kind: SemanticTokenKind,
    pub modifiers: u32,
    pub range: Range,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct SignatureHelp {
    #[wasm_bindgen(getter_with_clone)]
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: Option<u32>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct SignatureInformation {
    #[wasm_bindgen(getter_with_clone)]
    pub label: String,
    #[wasm_bindgen(getter_with_clone)]
    pub documentation: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub parameters: Vec<ParameterInformation>,
    pub active_parameter: Option<u32>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct ParameterInformation {
    #[wasm_bindgen(getter_with_clone)]
    pub label: String,
    #[wasm_bindgen(getter_with_clone)]
    pub documentation: Option<String>,
}

#[wasm_bindgen]
pub struct DocumentHighlight {
    #[wasm_bindgen(readonly)]
    pub range: Range,

    #[wasm_bindgen(readonly)]
    pub kind: DocumentHighlightKind,
}

#[wasm_bindgen]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DocumentHighlightKind {
    Text = 1,
    Read = 2,
    Write = 3,
}

impl From<ty_ide::ReferenceKind> for DocumentHighlightKind {
    fn from(kind: ty_ide::ReferenceKind) -> Self {
        match kind {
            ty_ide::ReferenceKind::Read => DocumentHighlightKind::Read,
            ty_ide::ReferenceKind::Write => DocumentHighlightKind::Write,
            ty_ide::ReferenceKind::Other => DocumentHighlightKind::Text,
        }
    }
}

#[wasm_bindgen]
impl SemanticToken {
    pub fn kinds() -> Vec<String> {
        ty_ide::SemanticTokenType::all()
            .iter()
            .map(|ty| ty.as_lsp_concept().to_string())
            .collect()
    }

    pub fn modifiers() -> Vec<String> {
        ty_ide::SemanticTokenModifier::all_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }
}

#[wasm_bindgen]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum SemanticTokenKind {
    Namespace,
    Class,
    Parameter,
    SelfParameter,
    ClsParameter,
    Variable,
    Property,
    Function,
    Method,
    Keyword,
    String,
    Number,
    Decorator,
    BuiltinConstant,
    TypeParameter,
}

impl From<ty_ide::SemanticTokenType> for SemanticTokenKind {
    fn from(value: ty_ide::SemanticTokenType) -> Self {
        match value {
            ty_ide::SemanticTokenType::Namespace => Self::Namespace,
            ty_ide::SemanticTokenType::Class => Self::Class,
            ty_ide::SemanticTokenType::Parameter => Self::Parameter,
            ty_ide::SemanticTokenType::SelfParameter => Self::SelfParameter,
            ty_ide::SemanticTokenType::ClsParameter => Self::ClsParameter,
            ty_ide::SemanticTokenType::Variable => Self::Variable,
            ty_ide::SemanticTokenType::Property => Self::Property,
            ty_ide::SemanticTokenType::Function => Self::Function,
            ty_ide::SemanticTokenType::Method => Self::Method,
            ty_ide::SemanticTokenType::Keyword => Self::Keyword,
            ty_ide::SemanticTokenType::String => Self::String,
            ty_ide::SemanticTokenType::Number => Self::Number,
            ty_ide::SemanticTokenType::Decorator => Self::Decorator,
            ty_ide::SemanticTokenType::BuiltinConstant => Self::BuiltinConstant,
            ty_ide::SemanticTokenType::TypeParameter => Self::TypeParameter,
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

    fn cache_dir(&self) -> Option<SystemPathBuf> {
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
    ) -> Result<Box<dyn Iterator<Item = Result<SystemPathBuf, GlobError>> + '_>, PatternError> {
        Ok(Box::new(self.fs.glob(pattern)?))
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}
