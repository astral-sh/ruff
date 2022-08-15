use anyhow::Result;
use clap::{Parser, ValueHint};
use lazy_static::lazy_static;
use log::debug;
use rustpython_parser::ast::{Stmt, StmtKind, Suite};
use rustpython_parser::parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::DirEntry;

use ::rust_python_linter::fs;
use ::rust_python_linter::logging::set_up_logging;
use ::rust_python_linter::visitor::{walk_stmt, Visitor};

#[derive(Debug, Parser)]
struct Cli {
    #[clap(parse(from_os_str), value_hint = ValueHint::DirPath, required = true)]
    files: Vec<PathBuf>,
    #[clap(short, long, action)]
    verbose: bool,
}

lazy_static! {
    // The output of: `sys.stdlib_module_names` (on Python 3.10.2)
    static ref STDLIB_MODULE_NAMES: HashSet<&'static str> = vec![
        "__future__",
        "_abc",
        "_aix_support",
        "_ast",
        "_asyncio",
        "_bisect",
        "_blake2",
        "_bootsubprocess",
        "_bz2",
        "_codecs",
        "_codecs_cn",
        "_codecs_hk",
        "_codecs_iso2022",
        "_codecs_jp",
        "_codecs_kr",
        "_codecs_tw",
        "_collections",
        "_collections_abc",
        "_compat_pickle",
        "_compression",
        "_contextvars",
        "_crypt",
        "_csv",
        "_ctypes",
        "_curses",
        "_curses_panel",
        "_datetime",
        "_dbm",
        "_decimal",
        "_elementtree",
        "_frozen_importlib",
        "_frozen_importlib_external",
        "_functools",
        "_gdbm",
        "_hashlib",
        "_heapq",
        "_imp",
        "_io",
        "_json",
        "_locale",
        "_lsprof",
        "_lzma",
        "_markupbase",
        "_md5",
        "_msi",
        "_multibytecodec",
        "_multiprocessing",
        "_opcode",
        "_operator",
        "_osx_support",
        "_overlapped",
        "_pickle",
        "_posixshmem",
        "_posixsubprocess",
        "_py_abc",
        "_pydecimal",
        "_pyio",
        "_queue",
        "_random",
        "_scproxy",
        "_sha1",
        "_sha256",
        "_sha3",
        "_sha512",
        "_signal",
        "_sitebuiltins",
        "_socket",
        "_sqlite3",
        "_sre",
        "_ssl",
        "_stat",
        "_statistics",
        "_string",
        "_strptime",
        "_struct",
        "_symtable",
        "_thread",
        "_threading_local",
        "_tkinter",
        "_tracemalloc",
        "_uuid",
        "_warnings",
        "_weakref",
        "_weakrefset",
        "_winapi",
        "_zoneinfo",
        "abc",
        "aifc",
        "antigravity",
        "argparse",
        "array",
        "ast",
        "asynchat",
        "asyncio",
        "asyncore",
        "atexit",
        "audioop",
        "base64",
        "bdb",
        "binascii",
        "binhex",
        "bisect",
        "builtins",
        "bz2",
        "cProfile",
        "calendar",
        "cgi",
        "cgitb",
        "chunk",
        "cmath",
        "cmd",
        "code",
        "codecs",
        "codeop",
        "collections",
        "colorsys",
        "compileall",
        "concurrent",
        "configparser",
        "contextlib",
        "contextvars",
        "copy",
        "copyreg",
        "crypt",
        "csv",
        "ctypes",
        "curses",
        "dataclasses",
        "datetime",
        "dbm",
        "decimal",
        "difflib",
        "dis",
        "distutils",
        "doctest",
        "email",
        "encodings",
        "ensurepip",
        "enum",
        "errno",
        "faulthandler",
        "fcntl",
        "filecmp",
        "fileinput",
        "fnmatch",
        "fractions",
        "ftplib",
        "functools",
        "gc",
        "genericpath",
        "getopt",
        "getpass",
        "gettext",
        "glob",
        "graphlib",
        "grp",
        "gzip",
        "hashlib",
        "heapq",
        "hmac",
        "html",
        "http",
        "idlelib",
        "imaplib",
        "imghdr",
        "imp",
        "importlib",
        "inspect",
        "io",
        "ipaddress",
        "itertools",
        "json",
        "keyword",
        "lib2to3",
        "linecache",
        "locale",
        "logging",
        "lzma",
        "mailbox",
        "mailcap",
        "marshal",
        "math",
        "mimetypes",
        "mmap",
        "modulefinder",
        "msilib",
        "msvcrt",
        "multiprocessing",
        "netrc",
        "nis",
        "nntplib",
        "nt",
        "ntpath",
        "nturl2path",
        "numbers",
        "opcode",
        "operator",
        "optparse",
        "os",
        "ossaudiodev",
        "pathlib",
        "pdb",
        "pickle",
        "pickletools",
        "pipes",
        "pkgutil",
        "platform",
        "plistlib",
        "poplib",
        "posix",
        "posixpath",
        "pprint",
        "profile",
        "pstats",
        "pty",
        "pwd",
        "py_compile",
        "pyclbr",
        "pydoc",
        "pydoc_data",
        "pyexpat",
        "queue",
        "quopri",
        "random",
        "re",
        "readline",
        "reprlib",
        "resource",
        "rlcompleter",
        "runpy",
        "sched",
        "secrets",
        "select",
        "selectors",
        "shelve",
        "shlex",
        "shutil",
        "signal",
        "site",
        "smtpd",
        "smtplib",
        "sndhdr",
        "socket",
        "socketserver",
        "spwd",
        "sqlite3",
        "sre_compile",
        "sre_constants",
        "sre_parse",
        "ssl",
        "stat",
        "statistics",
        "string",
        "stringprep",
        "struct",
        "subprocess",
        "sunau",
        "symtable",
        "sys",
        "sysconfig",
        "syslog",
        "tabnanny",
        "tarfile",
        "telnetlib",
        "tempfile",
        "termios",
        "textwrap",
        "this",
        "threading",
        "time",
        "timeit",
        "tkinter",
        "token",
        "tokenize",
        "trace",
        "traceback",
        "tracemalloc",
        "tty",
        "turtle",
        "turtledemo",
        "types",
        "typing",
        "unicodedata",
        "unittest",
        "urllib",
        "uu",
        "uuid",
        "venv",
        "warnings",
        "wave",
        "weakref",
        "webbrowser",
        "winreg",
        "winsound",
        "wsgiref",
        "xdrlib",
        "xml",
        "xmlrpc",
        "zipapp",
        "zipfile",
        "zipimport",
        "zlib",
        "zoneinfo",
    ]
    .into_iter()
    .collect();
}

#[allow(dead_code)]
#[derive(Debug)]
struct ModuleImport {
    module_name: String,
    remote_name: Option<String>,
    local_name: Option<String>,
    lineno: usize,
    level: usize,
}

#[derive(Default)]
struct ImportVisitor {
    imports: Vec<ModuleImport>,
}

/// https://github.com/blais/snakefood/blob/f902c9a099f7c5bb75154a747bf098259211025d/lib/python/snakefood/find.py#L130
impl Visitor for ImportVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::Import { names } => {
                for alias in names {
                    self.imports.push(ModuleImport {
                        module_name: alias.name.clone(),
                        remote_name: None,
                        local_name: Some(
                            alias.asname.clone().unwrap_or_else(|| alias.name.clone()),
                        ),
                        lineno: stmt.location.row(),
                        level: 0,
                    })
                }
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                if let Some(module_name) = module {
                    if module_name == "__future__" {
                        return;
                    }
                    for alias in names {
                        if alias.name == "*" {
                            self.imports.push(ModuleImport {
                                module_name: module_name.clone(),
                                remote_name: None,
                                local_name: None,
                                lineno: stmt.location.row(),
                                level: *level,
                            })
                        } else {
                            self.imports.push(ModuleImport {
                                module_name: module_name.clone(),
                                remote_name: Some(alias.name.clone()),
                                local_name: Some(
                                    alias.asname.clone().unwrap_or_else(|| alias.name.clone()),
                                ),
                                lineno: stmt.location.row(),
                                level: *level,
                            })
                        }
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

fn collect_imports(python_ast: &Suite) -> Vec<ModuleImport> {
    python_ast
        .iter()
        .flat_map(|stmt| {
            let mut visitor: ImportVisitor = Default::default();
            visitor.visit_stmt(stmt);
            visitor.imports
        })
        .collect()
}

/// https://github.com/blais/snakefood/blob/f902c9a099f7c5bb75154a747bf098259211025d/lib/python/snakefood/find.py#L315
fn find_dotted_module(
    module_name: &str,
    remote_name: &Option<String>,
    source_roots: &[PathBuf],
    level: &usize,
) -> Result<Option<PathBuf>> {
    // Check for standard library and built-in imports.
    if STDLIB_MODULE_NAMES.contains(module_name.split('.').next().unwrap()) {
        debug!("Skipping standard library import: {module_name}");
        return Ok(None);
    }

    for source_root in source_roots {
        // Handle relative imports.
        let mut parent_dir: &Path = source_root;
        for _ in 0..*level {
            parent_dir = parent_dir
                .parent()
                .ok_or(anyhow::anyhow!("Unable to find parent directory."))?;
        }

        let names: Vec<&str> = module_name.split('.').into_iter().collect();

        // If we can't find the imported module, return None.
        // This could be a third-party library, part of the standard library, etc.
        match find_dotted(&names, parent_dir)? {
            Some(filename) => {
                // If this is an `import from`, check if the target symbol is itself a module.
                if let Some(remote_name) = remote_name {
                    if let Some(filename) = find_dotted(
                        &[remote_name],
                        &filename
                            .parent()
                            .ok_or(anyhow::anyhow!("Unable to find parent directory."))?,
                    )? {
                        return Ok(Some(filename));
                    }
                }

                return Ok(Some(filename));
            }
            None => {}
        }
    }

    // Most likely a third-party module.
    debug!("Unable to find import for: {module_name}");
    Ok(None)
}

/// https://github.com/python/cpython/blob/main/Lib/imp.py#L255
fn find_module(name: &str, parent_dir: &Path) -> Option<PathBuf> {
    let package_directory = parent_dir.join(name);
    let package_file_name = "__init__.py";
    let file_path = package_directory.join(package_file_name);
    if file_path.exists() {
        return Some(file_path);
    }

    let file_name = format!("{}.py", name);
    let file_path = parent_dir.join(file_name);
    if file_path.exists() {
        return Some(file_path);
    }

    None
}

/// https://github.com/blais/snakefood/blob/f902c9a099f7c5bb75154a747bf098259211025d/lib/python/snakefood/find.py#L371
fn find_dotted(names: &[&str], parent_dir: &Path) -> Result<Option<PathBuf>> {
    let mut filename: Option<PathBuf> = None;

    for name in names {
        let dirname: PathBuf = if let Some(filename) = filename {
            filename
                .clone()
                .parent()
                .ok_or(anyhow::anyhow!("Unable to find parent directory."))?
                .to_path_buf()
        } else {
            parent_dir.to_path_buf()
        };
        match find_module(name, dirname.as_path()) {
            Some(module) => {
                filename = Some(module.clone());
            }
            None => return Ok(None),
        }
    }

    Ok(filename)
}

/// https://github.com/blais/snakefood/blob/f902c9a099f7c5bb75154a747bf098259211025d/lib/python/snakefood/find.py#L30
fn map_imports_to_files(
    imports: &[ModuleImport],
    source_roots: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = vec![];
    let mut seen: HashSet<(&String, &Option<String>)> = HashSet::new();
    for import in imports {
        let sig = (&import.module_name, &import.remote_name);
        if seen.contains(&sig) {
            continue;
        }
        seen.insert(sig);

        match find_dotted_module(
            &import.module_name,
            &import.remote_name,
            source_roots,
            &import.level,
        )? {
            Some(module_filename) => files.push(std::fs::canonicalize(module_filename)?),
            None => {}
        }
    }

    Ok(files)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    let source_roots: &[PathBuf] = &cli.files;
    let files: Vec<DirEntry> = source_roots
        .iter()
        .flat_map(fs::iter_python_files)
        .collect();
    for entry in files.iter().take(10) {
        println!("--- {} ---", entry.path().to_string_lossy());

        // Read the file from disk.
        let contents = fs::read_file(entry.path())?;

        // Run the parser.
        let python_ast = parser::parse_program(&contents)?;

        // Collect imports.
        let imports = collect_imports(&python_ast);
        for import in &imports {
            println!("{} imports: {:?}", entry.path().to_string_lossy(), import)
        }

        // Map from imports to first-party dependencies.
        let dependencies = map_imports_to_files(&imports, source_roots)?;
        for dependency in dependencies {
            println!(
                "{} depends on: {:?}",
                entry.path().to_string_lossy(),
                dependency.to_string_lossy(),
            )
        }
    }

    Ok(())
}
