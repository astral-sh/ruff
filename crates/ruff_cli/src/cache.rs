use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use itertools::Itertools;
use log::error;
use path_absolutize::Absolutize;
use ruff::message::{Location, Message};
use ruff::settings::{flags, AllSettings, Settings};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::{DiagnosticKind, Fix};
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::{LineIndex, SourceCodeBuf};
use rustc_hash::FxHashMap;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct CheckResultRef<'a> {
    #[serde(serialize_with = "serialize_messages")]
    messages: &'a [Message],
    imports: &'a ImportMap,
    sources: Vec<(&'a str, &'a str)>,
}

fn serialize_messages<S>(messages: &[Message], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut s = serializer.serialize_seq(Some(messages.len()))?;

    for message in messages {
        s.serialize_element(&SerializeMessage(message))?;
    }

    s.end()
}

struct SerializeMessage<'a>(&'a Message);

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
            filename,
            // Serialized manually for all files
            source: _source,
            noqa_row,
        } = self.0;

        let mut s = serializer.serialize_struct("Message", 6)?;

        s.serialize_field("kind", &kind)?;
        s.serialize_field("location", &location)?;
        s.serialize_field("end_location", &end_location)?;
        s.serialize_field("fix", &fix)?;
        s.serialize_field("filename", &filename)?;
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
    filename: String,
    noqa_row: usize,
}

#[derive(Deserialize)]
struct CheckResult {
    messages: Vec<MessageHeader>,
    imports: ImportMap,
    sources: Vec<(String, String)>,
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
            sources,
        }) => {
            let mut messages = Vec::with_capacity(headers.len());
            let sources: FxHashMap<_, _> = sources
                .into_iter()
                .map(|(filename, content)| {
                    let index = LineIndex::from_source_text(&content);
                    (filename, SourceCodeBuf::new(&content, index))
                })
                .collect();

            for header in headers {
                messages.push(Message {
                    kind: header.kind,
                    location: header.location,
                    end_location: header.end_location,
                    fix: header.fix,
                    source: sources.get(&header.filename).cloned(),
                    filename: header.filename,
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
    // Store the content of the source files, assuming that all files with the same name have the same content
    let sources: Vec<_> = messages
        .iter()
        .filter_map(|message| {
            message
                .source
                .as_ref()
                .map(|source| (&*message.filename, source.text()))
        })
        .unique_by(|(filename, _)| *filename)
        .collect();

    let check_result = CheckResultRef {
        messages,
        imports,
        sources,
    };
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
