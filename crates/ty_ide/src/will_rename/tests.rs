use super::*;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use std::collections::BTreeSet;
use ty_project::{ProjectMetadata, TestDb};

#[test]
fn file_rename_contract() {
    let db = test_db(&[
        ("/pkg/__init__.py", ""),
        ("/pkg/old.py", "class C: ...\n"),
        ("/old.py", "class C: ...\n"),
        (
            "/direct.py",
            "import pkg.old\nvalue: 'pkg.old.C'\nprint(pkg.old.C)\n",
        ),
        (
            "/from.py",
            "from typing import Literal\nfrom pkg import old\nvalue: 'old.C'\nbad: \"old.C[\"\nliteral: Literal[\"old.C[\"]\nother: 'threshold['\nroot: 'pkg.Unrelated['\ndef f(): return old.C\nclass C: value = old.C\ntotal = 0\ntotal += old.C.value\ndef assigned(flag):\n if flag: old = 1\n return old\ndef comp(): return [old for old in ()]\n",
        ),
        ("/alias.py", "from pkg import old as old\nprint(old.C)\n"),
        ("/facade.py", "from pkg import old\n"),
        (
            "/loop.py",
            "import pkg.old\nfor old in [pkg.old.C]: print(old)\n",
        ),
        (
            "/e.py",
            "old\nif x: from pkg import old\nelse: import old\n",
        ),
        (
            "/targets.py",
            "from pkg import old\n(old := old)\nfor old in [1]: pass\nvalues = [0 for old in [1]]\n",
        ),
        (
            "/types.py",
            "import pkg.old\ntype Alias[T: pkg.old.C] = list[pkg.old.C]\n",
        ),
        (
            "/comment.py",
            "from pkg import old\nx = None  # type: old.C\n",
        ),
        (
            "/invalid.py",
            "from typing import Callable, Concatenate\nfrom pkg import old\nx: \"old.C[\"\ny: list[\"old.C[\"]\nz: Callable[\"old.C[\", int]\nw: Callable[Concatenate[int, \"old.C[\"], int]\n",
        ),
        (
            "/exports.py",
            "from pkg import old\n__all__ = ['old']\ncomputed = old.__name__\n",
        ),
        (
            "/class_fallback.py",
            "def f():\n from pkg import old\n class C:\n  print(old.C)\n  old = 1\n old = 2\n",
        ),
    ]);
    assert_success(
        &db,
        &[
            file("/pkg/old.py", "/pkg/new.py"),
            file("/old.py", "/other.py"),
        ],
        &[
            (
                "/direct.py",
                "import pkg.new\nvalue: 'pkg.new.C'\nprint(pkg.new.C)\n",
            ),
            (
                "/from.py",
                "from typing import Literal\nfrom pkg import new\nvalue: 'new.C'\nbad: \"old.C[\"\nliteral: Literal[\"old.C[\"]\nother: 'threshold['\nroot: 'pkg.Unrelated['\ndef f(): return new.C\nclass C: value = new.C\ntotal = 0\ntotal += new.C.value\ndef assigned(flag):\n if flag: old = 1\n return old\ndef comp(): return [old for old in ()]\n",
            ),
            ("/alias.py", "from pkg import new as old\nprint(old.C)\n"),
            ("/facade.py", "from pkg import new\n"),
            (
                "/loop.py",
                "import pkg.new\nfor old in [pkg.new.C]: print(old)\n",
            ),
            (
                "/e.py",
                "old\nif x: from pkg import new\nelse: import other\n",
            ),
            (
                "/targets.py",
                "from pkg import new\n(old := new)\nfor old in [1]: pass\nvalues = [0 for old in [1]]\n",
            ),
            (
                "/types.py",
                "import pkg.new\ntype Alias[T: pkg.new.C] = list[pkg.new.C]\n",
            ),
            (
                "/comment.py",
                "from pkg import new\nx = None  # type: old.C\n",
            ),
            (
                "/invalid.py",
                "from typing import Callable, Concatenate\nfrom pkg import new\nx: \"old.C[\"\ny: list[\"old.C[\"]\nz: Callable[\"old.C[\", int]\nw: Callable[Concatenate[int, \"old.C[\"], int]\n",
            ),
            (
                "/exports.py",
                "from pkg import new\n__all__ = ['old']\ncomputed = new.__name__\n",
            ),
            (
                "/class_fallback.py",
                "def f():\n from pkg import new\n class C:\n  print(old.C)\n  old = 1\n old = 2\n",
            ),
        ],
    );
}

#[test]
fn unicode_identifier_prefilter() {
    let db = test_db(&[
        ("/K·b.py", "class C: ...\n"),
        ("/use.py", "import \u{212a}·b\nprint(\u{212a}·b.C)\n"),
    ]);
    assert_success(
        &db,
        &[file("/K·b.py", "/new.py")],
        &[("/use.py", "import new\nprint(new.C)\n")],
    );
}

#[test]
fn aliases_and_declarations_remain_stable() {
    let mut db = test_db(&[
        ("/pkg/__init__.py", "from . import old as old\n"),
        ("/pkg/old.py", "class C: ...\n"),
        (
            "/use.py",
            "import pkg, decl\nprint(pkg.old.C, decl.old.C)\n",
        ),
        (
            "/decl.py",
            "import pkg.old as source\nold = source\ndef outer():\n from pkg import old\n print(old.C)\n",
        ),
    ]);
    assert_success(
        &db,
        &[file("/pkg/old.py", "/pkg/new.py")],
        &[
            ("/pkg/__init__.py", "from . import new as old\n"),
            (
                "/decl.py",
                "import pkg.new as source\nold = source\ndef outer():\n from pkg import new\n print(new.C)\n",
            ),
        ],
    );
    let mixed = "if input(): from . import old as old\nelse: from . import old\n";
    db.write_file("/pkg/__init__.py", mixed).unwrap();
    assert_success(
        &db,
        &[file("/pkg/old.py", "/pkg/new.py")],
        &[
            (
                "/pkg/__init__.py",
                "if input(): from . import new as old\nelse: from . import new\n",
            ),
            (
                "/decl.py",
                "import pkg.new as source\nold = source\ndef outer():\n from pkg import new\n print(new.C)\n",
            ),
        ],
    );
}

#[test]
fn representable_cross_parent_file_move() {
    let db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/x.py", ""),
        ("/a/one/__init__.py", ""),
        ("/a/one/old.py", "from .. import one\nclass C: ...\n"),
        (
            "/use.py",
            "import a.one.old\nprint(a.one.old.C)\nfrom a.one import old\nprint(old.C)\nfrom a import x\nfor x in [old.C]: print(x)\n",
        ),
        (
            "/aliased.py",
            "import a.one.old as stable\nprint(stable.C)\n",
        ),
    ]);
    assert_success(
        &db,
        &[
            file("/a/one/old.py", "/a/two/new.py"),
            file("/a/x.py", "/a/y.py"),
        ],
        &[
            (
                "/use.py",
                "import a.two.new\nprint(a.two.new.C)\nfrom a.two import new\nprint(new.C)\nfrom a import y\nfor x in [new.C]: print(x)\n",
            ),
            (
                "/aliased.py",
                "import a.two.new as stable\nprint(stable.C)\n",
            ),
        ],
    );
}

#[test]
fn regular_package_and_runtime_provenance() {
    let db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/p/__init__.py", ""),
        ("/a/p/__init__.pyi", ""),
        ("/a/p/mod.py", "from . import h\nfrom ..p import h\n"),
        ("/a/p/h.py", ""),
        ("/use.py", "import a.p.mod\nfrom a.p import *\na.p.mod\n"),
    ]);
    assert_success(
        &db,
        &[PathRename::directory("/a/p".into(), "/a/n".into())],
        &[
            ("/a/p/mod.py", "from . import h\nfrom ..n import h\n"),
            ("/use.py", "import a.n.mod\nfrom a.n import *\na.n.mod\n"),
        ],
    );

    let shadowed = test_db(&[
        ("/p/__init__.py", ""),
        ("/p/old.py", "class C: ...\n"),
        ("/p/old.pyi", "class C: ...\n"),
        ("/use.py", "import p.old\nprint(p.old.C)\n"),
    ]);
    assert_file_no_edits(&shadowed, "/p/old.pyi", "/p/new.pyi");
    assert_success(
        &shadowed,
        &[file("/p/old.py", "/p/new.py")],
        &[("/use.py", "import p.new\nprint(p.new.C)\n")],
    );
}

#[test]
fn unsupported_semantics_are_omitted() {
    for (name, source) in [
        ("R2-03", "from facade import old\nold.C\n"),
        ("R8-02 qualified store", "import pkg.old\npkg.old = 1\n"),
        ("R8-03 qualified delete", "import pkg.old\ndel pkg.old\n"),
        ("augmented attribute", "import old\nold.VALUE+=1"),
        ("nested attribute store", "import old\nold.VALUE=1"),
        ("nested attribute delete", "import old\ndel old.VALUE"),
        ("self assignment", "import pkg.old\npkg.old=pkg.old\n"),
        (
            "assignment-backed class attribute",
            "import old as source\nclass C: old = source\nC.old.C\n",
        ),
        ("global", "def f():\n global old\n import old\n old\n"),
        ("changed augmented", "import old\nold += 1\n"),
        ("star propagation", "from facade import *\nprint(old.C)\n"),
        ("deferred import", "def f():\n old.x\n import old\n"),
        (
            "stale package load",
            "from pkg import old\nimport pkg\npkg.old.C\n",
        ),
        (
            "class import fallback",
            "from pkg import old\nclass C:\n old.C\n from pkg import old\n",
        ),
        (
            "deferred closure",
            "def outer():\n def inner(): return old.C\n from pkg import old\n return inner\n",
        ),
        (
            "deleted exception target",
            "from pkg import old\ntry: 1 / int(input())\nexcept Exception as old: pass\nprint(old)\n",
        ),
        (
            "conditional stable deletion",
            "from pkg import old\nif flag:\n old = 1\n del old\nprint(old)\n",
        ),
        (
            "stable local deletion",
            "import old\ndef f(old):\n del old\n print(old)\n",
        ),
    ] {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/old.py", ""),
            ("/facade.py", "from pkg import old\n"),
            ("/main.py", source),
        ]);
        let package = source.contains("pkg") || source.contains("facade");
        let (rename, mut expected) = if package {
            (
                file("/pkg/old.py", "/pkg/new.py"),
                source
                    .replace("import pkg.old", "import pkg.new")
                    .replace("from pkg import old", "from pkg import new"),
            )
        } else {
            (
                file("/old.py", "/new.py"),
                source.replace("import old", "import new"),
            )
        };
        if name == "self assignment" {
            expected = expected.replace("pkg.old=pkg.old", "pkg.old=pkg.new");
        }
        let mut expected_files = Vec::new();
        if source != expected.as_str() {
            expected_files.push(("/main.py", expected.as_str()));
        }
        if package {
            expected_files.push(("/facade.py", "from pkg import new\n"));
        }
        assert_success_named(name, &db, &[rename], &expected_files);
    }
}

#[test]
fn scope_declarations_omit_only_dependent_occurrences() {
    let db = test_db(&[
        ("/old.py", "class C: ...\n"),
        ("/other.py", "class C: ...\n"),
        ("/pkg/__init__.py", ""),
        ("/pkg/old.py", "class C: ...\n"),
        (
            "/use.py",
            "import old\nimport other\nimport pkg.old\nprint(old.C, other.C, pkg.old.C)\ndef global_use():\n global old, pkg\n def nested(): return old.C, other.C, pkg.old.C\n return old.C, other.C, pkg.old.C, nested\ndef outer():\n import old\n import pkg.old\n def nonlocal_use():\n  nonlocal old, pkg\n  def nested(): return old.C, other.C, pkg.old.C\n  return old.C, other.C, pkg.old.C, nested\n return old.C, pkg.old.C, nonlocal_use\ndef sibling(): return old.C, other.C, pkg.old.C\ndef annotated():\n value: \"other.C\"\n",
        ),
    ]);
    assert_success(
        &db,
        &[
            file("/old.py", "/new.py"),
            file("/other.py", "/renamed.py"),
            file("/pkg/old.py", "/pkg/new.py"),
        ],
        &[(
            "/use.py",
            "import new\nimport renamed\nimport pkg.new\nprint(new.C, renamed.C, pkg.new.C)\ndef global_use():\n global old, pkg\n def nested(): return old.C, renamed.C, pkg.old.C\n return old.C, renamed.C, pkg.old.C, nested\ndef outer():\n import new\n import pkg.new\n def nonlocal_use():\n  nonlocal old, pkg\n  def nested(): return old.C, renamed.C, pkg.old.C\n  return old.C, renamed.C, pkg.old.C, nested\n return new.C, pkg.new.C, nonlocal_use\ndef sibling(): return new.C, renamed.C, pkg.new.C\ndef annotated():\n value: \"renamed.C\"\n",
        )],
    );
}

#[test]
fn unsupported_requests_and_imports_are_omitted() {
    let mut db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/old.py", ""),
        ("/b/__init__.py", ""),
        ("/use.py", "from a import old, sibling\n"),
    ]);
    assert_file_no_edits(&db, "/a/old.py", "/b/new.py");
    db.write_file("/use.py", "import a.old\n").unwrap();
    assert_file_no_edits(&db, "/a/old.py", "/b/new.py");
    db.write_file("/use.py", "import a.old.missing\n").unwrap();
    assert_file_no_edits(&db, "/a/old.py", "/a/new.py");
    assert_file_no_edits(&db, "/a/__init__.py", "/a/new.py");
    assert_no_edits(
        "R10-04 unresolved package alias",
        &test_db(&[
            ("/old/__init__.py", ""),
            ("/use.py", "from old import missing\n"),
        ]),
        &[PathRename::directory("/old".into(), "/new".into())],
    );
    assert_no_edits(
        "mixed relative aliases",
        &test_db(&[
            ("/a/__init__.py", ""),
            ("/a/one/__init__.py", ""),
            ("/a/one/old.py", "from . import helper, stable\n"),
            ("/a/one/helper.py", ""),
            ("/a/one/stable.py", ""),
        ]),
        &[
            file("/a/one/old.py", "/a/two/new.py"),
            file("/a/one/helper.py", "/a/two/helper.py"),
        ],
    );
}

#[test]
fn import_statements_are_coherent_units() {
    let db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/old.py", ""),
        ("/a/x.py", ""),
        ("/b/__init__.py", ""),
        (
            "/use.py",
            "from a import old, sibling\nfrom a import x\nprint(old, x)\n",
        ),
    ]);
    assert_success(
        &db,
        &[file("/a/old.py", "/b/new.py"), file("/a/x.py", "/a/y.py")],
        &[(
            "/use.py",
            "from a import old, sibling\nfrom a import y\nprint(old, y)\n",
        )],
    );
}

#[test]
fn reports_known_omissions() {
    let mut db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/old.py", ""),
        ("/b/__init__.py", ""),
        ("/use.py", "from a import old\nx = None  # type: old.C\n"),
    ]);
    let complete = will_rename_paths(
        &db,
        &[file("/a/old.py", "/a/new.py")],
        &db.project().files(&db),
        |_| true,
    );
    assert!(!complete.has_known_omissions());

    db.write_file("/use.py", "from a import old, sibling\n")
        .unwrap();
    let incomplete = will_rename_paths(
        &db,
        &[file("/a/old.py", "/b/new.py")],
        &db.project().files(&db),
        |_| true,
    );
    assert!(incomplete.has_known_omissions());

    db.write_file("/use.py", "import a.old\na.old = None\n")
        .unwrap();
    let unsupported_use = will_rename_paths(
        &db,
        &[file("/a/old.py", "/a/new.py")],
        &db.project().files(&db),
        |_| true,
    );
    assert!(unsupported_use.has_known_omissions());
}

#[test]
fn best_effort_request_contract() {
    let mut db = test_db(&[
        ("/a/__init__.py", ""),
        ("/a/x.py", ""),
        ("/a/o/__init__.py", ""),
        ("/a/o/old.py", ""),
        ("/a/one/__init__.py", ""),
        ("/a/one/old.py", "from .. import x\n"),
        ("/b/__init__.py", ""),
        ("/b/x.py", ""),
        ("/ns/mod.py", ""),
        ("/pkg/__init__.py", ""),
        ("/pkg/old.py", ""),
        ("/pkg/old.pyi", ""),
        ("/x.py", ""),
        (
            "/q.py",
            "from a.o import old\nfrom a import o\no.old\nif flag: from a import x\nelse: from b import x\n",
        ),
        ("/u.py", "import q\nq.x\n"),
        ("/independent.py", "import x\nx.VALUE\n"),
    ]);
    assert_success(&db, &[file("/pkg/old.py", "/pkg/new.py")], &[]);
    assert_success(
        &db,
        &[file("/a/o/old.py", "/a/new.py")],
        &[(
            "/q.py",
            "from a import new\nfrom a import o\no.old\nif flag: from a import x\nelse: from b import x\n",
        )],
    );
    let conflicts = || vec![file("/a/x.py", "/a/y.py"), file("/b/x.py", "/b/z.py")];
    assert_success(
        &db,
        &conflicts(),
        &[
            ("/a/one/old.py", "from .. import y\n"),
            (
                "/q.py",
                "from a.o import old\nfrom a import o\no.old\nif flag: from a import y\nelse: from b import z\n",
            ),
        ],
    );
    db.write_file("/u.py", "if q:from a import x\nelse: from b import x\nx")
        .unwrap();
    assert_no_edits(
        "namespace package",
        &db,
        &[PathRename::directory("/ns".into(), "/newns".into())],
    );
    assert_no_edits(
        "relative rebasing",
        &db,
        &[file("/a/one/old.py", "/b/new.py")],
    );
    assert_success(
        &db,
        &conflicts(),
        &[
            ("/a/one/old.py", "from .. import y\n"),
            (
                "/q.py",
                "from a.o import old\nfrom a import o\no.old\nif flag: from a import y\nelse: from b import z\n",
            ),
            ("/u.py", "if q:from a import y\nelse: from b import z\nx"),
        ],
    );
    assert_no_edits("extension change", &db, &[file("/x.py", "/x.pyi")]);
    assert_success(
        &db,
        &[
            file("/pkg/old.py", "/pkg/new.py"),
            file("/pkg/old.pyi", "/pkg/new.pyi"),
            file("/x.py", "/y.py"),
        ],
        &[("/independent.py", "import y\ny.VALUE\n")],
    );
    assert_success(
        &db,
        &[
            PathRename::directory("/pkg".into(), "/newpkg".into()),
            file("/pkg/old.py", "/elsewhere.py"),
            file("/x.py", "/y.py"),
        ],
        &[("/independent.py", "import y\ny.VALUE\n")],
    );
}

fn file(old: &str, new: &str) -> PathRename {
    PathRename::file(old.into(), new.into())
}

fn assert_file_no_edits(db: &TestDb, old: &str, new: &str) {
    assert_no_edits(old, db, &[file(old, new)]);
}

fn assert_success(db: &TestDb, renames: &[PathRename], expected: &[(&str, &str)]) {
    assert_success_named("rename", db, renames, expected);
}

fn assert_success_named(
    name: &str,
    db: &TestDb,
    renames: &[PathRename],
    expected: &[(&str, &str)],
) {
    let edits = will_rename_paths(db, renames, &db.project().files(db), |_| true).into_edits();
    let actual: BTreeSet<_> = edits.iter().map(|edit| edit.range.file()).collect();
    let expected_files: BTreeSet<_> = expected
        .iter()
        .map(|(path, _)| system_path_to_file(db, *path).unwrap())
        .collect();
    assert_eq!(actual, expected_files, "{name}");
    for &(path, contents) in expected {
        assert_eq!(apply_edits(db, &edits, path), contents, "{name}: {path}");
    }
}

fn assert_no_edits(name: &str, db: &TestDb, renames: &[PathRename]) {
    assert!(
        will_rename_paths(db, renames, &db.project().files(db), |_| true)
            .into_edits()
            .is_empty(),
        "{name}"
    );
}

fn test_db(files: &[(&str, &str)]) -> TestDb {
    let mut db = TestDb::new(ProjectMetadata::new("test", "/".into()));
    db.init_program_with_python_version(PythonVersion::latest_ty())
        .unwrap();
    db.write_files(files.iter().copied()).unwrap();
    db
}

fn apply_edits(db: &TestDb, edits: &[FileRenameEdit], path: &str) -> String {
    let file = system_path_to_file(db, path).unwrap();
    let mut edits: Vec<_> = edits
        .iter()
        .filter(|edit| edit.range.file() == file)
        .collect();
    edits.sort_unstable_by_key(|edit| std::cmp::Reverse(edit.range.start()));
    let mut result = source_text(db, file).as_str().to_owned();
    for edit in edits {
        result.replace_range(edit.range.range().to_std_range(), &edit.value);
    }
    result
}
