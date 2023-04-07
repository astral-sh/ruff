use std::cell::RefCell;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use log::error;
use path_absolutize::Absolutize;
use ruff::message::{Location, Message};
use ruff::settings::{flags, AllSettings, Settings};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::{DiagnosticKind, Fix};
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::SourceFileBuilder;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Vec storing all source files. The tuple is (filename, source code).
type Files<'a> = Vec<(&'a str, Option<&'a str>)>;
type FilesBuf = Vec<(String, Option<String>)>;

struct CheckResultRef<'a> {
    imports: &'a ImportMap,
    messages: &'a [Message],
}

impl Serialize for CheckResultRef<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("CheckResultRef", 3)?;

        s.serialize_field("imports", &self.imports)?;

        let serialize_messages = SerializeMessages {
            messages: self.messages,
            files: RefCell::default(),
        };

        s.serialize_field("messages", &serialize_messages)?;

        let files = serialize_messages.files.take();

        s.serialize_field("files", &files)?;

        s.end()
    }
}

struct SerializeMessages<'a> {
    messages: &'a [Message],
    files: RefCell<Files<'a>>,
}

impl Serialize for SerializeMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.messages.len()))?;
        let mut files = self.files.borrow_mut();

        for message in self.messages {
            // Using a Vec instead of a HashMap because the cache is per file and the large majority of
            // files have exactly one source file.
            let file_id = if let Some(position) = files
                .iter()
                .position(|(filename, _)| *filename == message.filename())
            {
                position
            } else {
                let index = files.len();
                files.push((message.filename(), message.file.source_text()));
                index
            };

            s.serialize_element(&SerializeMessage { message, file_id })?;
        }

        s.end()
    }
}

struct SerializeMessage<'a> {
    message: &'a Message,
    file_id: usize,
}

impl Serialize for SerializeMessage<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let Message {
            kind,
            location,
            end_location,
            fix,
            // Serialized manually for all files
            file: _,
            noqa_row,
        } = self.message;

        let mut s = serializer.serialize_struct("Message", 6)?;

        s.serialize_field("kind", &kind)?;
        s.serialize_field("location", &location)?;
        s.serialize_field("end_location", &end_location)?;
        s.serialize_field("fix", &fix)?;
        s.serialize_field("file_id", &self.file_id)?;
        s.serialize_field("noqa_row", &noqa_row)?;

        s.end()
    }
}

#[derive(Deserialize)]
struct MessageHeader {
    kind: DiagnosticKind,
    location: Location,
    end_location: Location,
    fix: Fix,
    file_id: usize,
    noqa_row: usize,
}

#[derive(Deserialize)]
struct CheckResult {
    imports: ImportMap,
    messages: Vec<MessageHeader>,
    files: FilesBuf,
}

fn content_dir() -> &'static Path {
    Path::new("content")
}

fn cache_key(
    path: &Path,
    package: Option<&Path>,
    metadata: &fs::Metadata,
    settings: &Settings,
    autofix: flags::Autofix,
) -> u64 {
    let mut hasher = CacheKeyHasher::new();
    CARGO_PKG_VERSION.cache_key(&mut hasher);
    path.absolutize().unwrap().cache_key(&mut hasher);
    package
        .as_ref()
        .map(|path| path.absolutize().unwrap())
        .cache_key(&mut hasher);
    FileTime::from_last_modification_time(metadata).cache_key(&mut hasher);
    #[cfg(unix)]
    metadata.permissions().mode().cache_key(&mut hasher);
    settings.cache_key(&mut hasher);
    autofix.cache_key(&mut hasher);
    hasher.finish()
}

#[allow(dead_code)]
/// Initialize the cache at the specified `Path`.
pub fn init(path: &Path) -> Result<()> {
    // Create the cache directories.
    fs::create_dir_all(path.join(content_dir()))?;

    // Add the CACHEDIR.TAG.
    if !cachedir::is_tagged(path)? {
        cachedir::add_tag(path)?;
    }

    // Add the .gitignore.
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        let mut file = fs::File::create(gitignore_path)?;
        file.write_all(b"*")?;
    }

    Ok(())
}

fn write_sync(cache_dir: &Path, key: u64, value: &[u8]) -> Result<(), std::io::Error> {
    fs::write(
        cache_dir.join(content_dir()).join(format!("{key:x}")),
        value,
    )
}

fn read_sync(cache_dir: &Path, key: u64) -> Result<Vec<u8>, std::io::Error> {
    fs::read(cache_dir.join(content_dir()).join(format!("{key:x}")))
}

fn del_sync(cache_dir: &Path, key: u64) -> Result<(), std::io::Error> {
    fs::remove_file(cache_dir.join(content_dir()).join(format!("{key:x}")))
}

/// Get a value from the cache.
pub fn get(
    path: &Path,
    package: Option<&Path>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
) -> Option<(Vec<Message>, ImportMap)> {
    let encoded = read_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
    )
    .ok()?;
    match bincode::deserialize::<CheckResult>(&encoded[..]) {
        Ok(CheckResult {
            messages: headers,
            imports,
            files: sources,
        }) => {
            let mut messages = Vec::with_capacity(headers.len());

            let source_files: Vec<_> = sources
                .into_iter()
                .map(|(filename, text)| {
                    let mut builder = SourceFileBuilder::from_string(filename);

                    if let Some(text) = text {
                        builder.set_source_text_string(text);
                    }

                    builder.finish()
                })
                .collect();

            for header in headers {
                let Some(source_file) = source_files.get(header.file_id) else {
                    error!("Failed to retrieve source file for cached entry");
                    return None;
                };

                messages.push(Message {
                    kind: header.kind,
                    location: header.location,
                    end_location: header.end_location,
                    fix: header.fix,
                    file: source_file.clone(),
                    noqa_row: header.noqa_row,
                });
            }

            Some((messages, imports))
        }
        Err(e) => {
            error!("Failed to deserialize encoded cache entry: {e:?}");
            None
        }
    }
}

/// Set a value in the cache.
pub fn set(
    path: &Path,
    package: Option<&Path>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
    messages: &[Message],
    imports: &ImportMap,
) {
    let check_result = CheckResultRef { imports, messages };
    if let Err(e) = write_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
        &bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}");
    }
}

/// Delete a value from the cache.
pub fn del(
    path: &Path,
    package: Option<&Path>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
) {
    drop(del_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
    ));
}
