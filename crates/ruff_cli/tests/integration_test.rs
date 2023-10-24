#![cfg(not(target_family = "wasm"))]

use std::fs;
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::path::Path;
use std::process::Command;
use std::str;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
#[cfg(unix)]
use clap::Parser;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
#[cfg(unix)]
use path_absolutize::path_dedot;
use tempfile::TempDir;

#[cfg(unix)]
use ruff_cli::args::Args;
#[cfg(unix)]
use ruff_cli::run;

const BIN_NAME: &str = "ruff";
const STDIN_BASE_OPTIONS: &[&str] = &["--isolated", "--no-cache", "-", "--output-format", "text"];

#[test]
fn stdin_success() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .pass_stdin(""), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[test]
fn stdin_error() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

#[test]
fn stdin_filename() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "F401.py"])
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    F401.py:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

/// Raise `TCH` errors in `.py` files ...
#[test]
fn stdin_source_type_py() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "TCH.py"])
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    TCH.py:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

/// ... but not in `.pyi` files.
#[test]
fn stdin_source_type_pyi() {
    let args = ["--stdin-filename", "TCH.pyi", "--select", "TCH"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[cfg(unix)]
#[test]
fn stdin_json() {
    let args = [
        "-",
        "--isolated",
        "--no-cache",
        "--output-format",
        "json",
        "--stdin-filename",
        "F401.py",
    ];

    let directory = path_dedot::CWD.to_str().unwrap();
    let binding = Path::new(directory).join("F401.py");
    let file_path = binding.display();

    insta::with_settings!({filters => vec![
        (file_path.to_string().as_str(), "/path/to/F401.py"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(args)
            .pass_stdin("import os\n"));
    });
}

#[test]
fn stdin_fix_py() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\nimport sys\n\nprint(sys.version)\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    import sys

    print(sys.version)

    ----- stderr -----
    Found 1 error (1 fixed, 0 remaining).
    "###);
}

#[test]
fn stdin_fix_jupyter() {
    let args = ["--fix", "--stdin-filename", "Jupyter.ipynb"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin(r#"{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": 1,
   "id": "dccc687c-96e2-4604-b957-a8a89b5bec06",
   "metadata": {},
   "outputs": [],
   "source": [
    "import os"
   ]
  },
  {
   "cell_type": "markdown",
   "id": "19e1b029-f516-4662-a9b9-623b93edac1a",
   "metadata": {},
   "source": [
    "Foo"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": 2,
   "id": "cdce7b92-b0fb-4c02-86f6-e233b26fa84f",
   "metadata": {},
   "outputs": [],
   "source": [
    "import sys"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": 3,
   "id": "e40b33d2-7fe4-46c5-bdf0-8802f3052565",
   "metadata": {},
   "outputs": [
    {
     "name": "stdout",
     "output_type": "stream",
     "text": [
      "1\n"
     ]
    }
   ],
   "source": [
    "print(1)"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "a1899bc8-d46f-4ec0-b1d1-e1ca0f04bf60",
   "metadata": {},
   "outputs": [],
   "source": []
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3 (ipykernel)",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "codemirror_mode": {
    "name": "ipython",
    "version": 3
   },
   "file_extension": ".py",
   "mimetype": "text/x-python",
   "name": "python",
   "nbconvert_exporter": "python",
   "pygments_lexer": "ipython3",
   "version": "3.11.2"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
     "cells": [
      {
       "cell_type": "code",
       "execution_count": 1,
       "id": "dccc687c-96e2-4604-b957-a8a89b5bec06",
       "metadata": {},
       "outputs": [],
       "source": []
      },
      {
       "cell_type": "markdown",
       "id": "19e1b029-f516-4662-a9b9-623b93edac1a",
       "metadata": {},
       "source": [
        "Foo"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 2,
       "id": "cdce7b92-b0fb-4c02-86f6-e233b26fa84f",
       "metadata": {},
       "outputs": [],
       "source": []
      },
      {
       "cell_type": "code",
       "execution_count": 3,
       "id": "e40b33d2-7fe4-46c5-bdf0-8802f3052565",
       "metadata": {},
       "outputs": [
        {
         "name": "stdout",
         "output_type": "stream",
         "text": [
          "1\n"
         ]
        }
       ],
       "source": [
        "print(1)"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": null,
       "id": "a1899bc8-d46f-4ec0-b1d1-e1ca0f04bf60",
       "metadata": {},
       "outputs": [],
       "source": []
      }
     ],
     "metadata": {
      "kernelspec": {
       "display_name": "Python 3 (ipykernel)",
       "language": "python",
       "name": "python3"
      },
      "language_info": {
       "codemirror_mode": {
        "name": "ipython",
        "version": 3
       },
       "file_extension": ".py",
       "mimetype": "text/x-python",
       "name": "python",
       "nbconvert_exporter": "python",
       "pygments_lexer": "ipython3",
       "version": "3.11.2"
      }
     },
     "nbformat": 4,
     "nbformat_minor": 5
    }
    ----- stderr -----
    Found 2 errors (2 fixed, 0 remaining).
    "###);
}

#[test]
fn stdin_fix_when_not_fixable_should_still_print_contents() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\nimport sys\n\nif (1, 2):\n     print(sys.version)\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    import sys

    if (1, 2):
         print(sys.version)

    ----- stderr -----
    -:3:4: F634 If test is a tuple, which is always `True`
    Found 2 errors (1 fixed, 1 remaining).
    "###);
}

#[test]
fn stdin_fix_when_no_issues_should_still_print_contents() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import sys\n\nprint(sys.version)\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    import sys

    print(sys.version)

    ----- stderr -----
    "###);
}

#[test]
fn stdin_format_jupyter() {
    let args = ["format", "--stdin-filename", "Jupyter.ipynb", "--isolated"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(args)
        .pass_stdin(r#"{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "dccc687c-96e2-4604-b957-a8a89b5bec06",
   "metadata": {},
   "outputs": [],
   "source": [
    "x=1"
   ]
  },
  {
   "cell_type": "markdown",
   "id": "19e1b029-f516-4662-a9b9-623b93edac1a",
   "metadata": {},
   "source": [
    "Foo"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "cdce7b92-b0fb-4c02-86f6-e233b26fa84f",
   "metadata": {},
   "outputs": [],
   "source": [
    "def func():\n",
    "  pass\n",
    "print(1)\n",
    "import os"
   ]
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3 (ipykernel)",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "codemirror_mode": {
    "name": "ipython",
    "version": 3
   },
   "file_extension": ".py",
   "mimetype": "text/x-python",
   "name": "python",
   "nbconvert_exporter": "python",
   "pygments_lexer": "ipython3",
   "version": "3.10.13"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
     "cells": [
      {
       "cell_type": "code",
       "execution_count": null,
       "id": "dccc687c-96e2-4604-b957-a8a89b5bec06",
       "metadata": {},
       "outputs": [],
       "source": [
        "x = 1"
       ]
      },
      {
       "cell_type": "markdown",
       "id": "19e1b029-f516-4662-a9b9-623b93edac1a",
       "metadata": {},
       "source": [
        "Foo"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": null,
       "id": "cdce7b92-b0fb-4c02-86f6-e233b26fa84f",
       "metadata": {},
       "outputs": [],
       "source": [
        "def func():\n",
        "    pass\n",
        "\n",
        "\n",
        "print(1)\n",
        "import os"
       ]
      }
     ],
     "metadata": {
      "kernelspec": {
       "display_name": "Python 3 (ipykernel)",
       "language": "python",
       "name": "python3"
      },
      "language_info": {
       "codemirror_mode": {
        "name": "ipython",
        "version": 3
       },
       "file_extension": ".py",
       "mimetype": "text/x-python",
       "name": "python",
       "nbconvert_exporter": "python",
       "pygments_lexer": "ipython3",
       "version": "3.10.13"
      }
     },
     "nbformat": 4,
     "nbformat_minor": 5
    }

    ----- stderr -----
    "###);
}

#[test]
fn show_source() {
    let args = ["--show-source"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("l = 1"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
      |
    1 | l = 1
      | ^ E741
      |

    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn explain_status_codes_f401() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME)).args(["--explain", "F401"]));
}
#[test]
fn explain_status_codes_ruf404() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME)).args(["--explain", "RUF404"]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'RUF404' for '[RULE]'

    For more information, try '--help'.
    "###);
}

#[test]
fn show_statistics() {
    let args = ["--select", "F401", "--statistics"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import sys\nimport os\n\nprint(os.getuid())\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    1	F401	[*] `sys` imported but unused

    ----- stderr -----
    "###);
}

#[test]
fn nursery_prefix() {
    // `--select E` should detect E741, but not E225, which is in the nursery.
    let args = ["--select", "E"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn nursery_all() {
    // `--select ALL` should detect E741, but not E225, which is in the nursery.
    let args = ["--select", "ALL"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:1: D100 Missing docstring in public module
    Found 2 errors.

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    "###);
}

#[test]
fn nursery_direct() {
    // `--select E225` should detect E225.
    let args = ["--select", "E225"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:2: E225 Missing whitespace around operator
    Found 1 error.

    ----- stderr -----
    warning: Selection of nursery rule `E225` without the `--preview` flag is deprecated.
    "###);
}

#[test]
fn nursery_group_selector() {
    // Only nursery rules should be detected e.g. E225 and a warning should be displayed
    let args = ["--select", "NURSERY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 Missing whitespace around operator
    Found 2 errors.

    ----- stderr -----
    warning: The `NURSERY` selector has been deprecated. Use the `--preview` flag instead.
    "###);
}

#[test]
fn nursery_group_selector_preview_enabled() {
    // Only nursery rules should be detected e.g. E225 and a warning should be displayed
    let args = ["--select", "NURSERY", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 [*] Missing whitespace around operator
    Found 2 errors.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: The `NURSERY` selector has been deprecated.
    "###);
}

#[test]
fn preview_enabled_prefix() {
    // E741 and E225 (preview) should both be detected
    let args = ["--select", "E", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:2: E225 [*] Missing whitespace around operator
    Found 2 errors.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

#[test]
fn preview_enabled_all() {
    let args = ["--select", "ALL", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:1: D100 Missing docstring in public module
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 [*] Missing whitespace around operator
    Found 4 errors.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    "###);
}

#[test]
fn preview_enabled_direct() {
    // E225 should be detected without warning
    let args = ["--select", "E225", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:2: E225 [*] Missing whitespace around operator
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

#[test]
fn preview_disabled_direct() {
    // FURB145 is preview not nursery so selecting should be empty
    let args = ["--select", "FURB145"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("a = l[:]\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Selection `FURB145` has no effect because the `--preview` flag was not included.
    "###);
}

#[test]
fn preview_disabled_prefix_empty() {
    // Warns that the selection is empty since all of the CPY rules are in preview
    let args = ["--select", "CPY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Selection `CPY` has no effect because the `--preview` flag was not included.
    "###);
}

#[test]
fn preview_disabled_does_not_warn_for_empty_ignore_selections() {
    // Does not warn that the selection is empty since the user is not trying to enable the rule
    let args = ["--ignore", "CPY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn preview_disabled_does_not_warn_for_empty_fixable_selections() {
    // Does not warn that the selection is empty since the user is not trying to enable the rule
    let args = ["--fixable", "CPY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn preview_group_selector() {
    // `--select PREVIEW` should error (selector was removed)
    let args = ["--select", "PREVIEW", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'PREVIEW' for '--select <RULE_CODE>'

    For more information, try '--help'.
    "###);
}

#[test]
fn preview_enabled_group_ignore() {
    // `--select E --ignore PREVIEW` should detect E741 and E225, which is in preview but "E" is more specific.
    let args = ["--select", "E", "--ignore", "PREVIEW", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'PREVIEW' for '--ignore <RULE_CODE>'

    For more information, try '--help'.
    "###);
}

/// An unreadable pyproject.toml in non-isolated mode causes ruff to hard-error trying to build up
/// configuration globs
#[cfg(unix)]
#[test]
fn unreadable_pyproject_toml() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    // Create an empty file with 000 permissions
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o000)
        .open(pyproject_toml)?;

    // Don't `--isolated` since the configuration discovery is where the error happens
    let args = Args::parse_from(["", "check", "--no-cache", tempdir.path().to_str().unwrap()]);
    let err = run(args).err().context("Unexpected success")?;
    assert_eq!(
        err.chain()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Permission denied (os error 13)".to_string()],
    );
    Ok(())
}

/// Check the output with an unreadable directory
#[cfg(unix)]
#[test]
fn unreadable_dir() -> Result<()> {
    // Create a directory with 000 (not iterable/readable) permissions
    let tempdir = TempDir::new()?;
    let unreadable_dir = tempdir.path().join("unreadable_dir");
    fs::create_dir(&unreadable_dir)?;
    fs::set_permissions(&unreadable_dir, Permissions::from_mode(0o000))?;

    // We (currently?) have to use a subcommand to check exit status (currently wrong) and logging
    // output
    // TODO(konstin): This should be a failure, but we currently can't track that
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["--no-cache", "--isolated"])
        .arg(&unreadable_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Encountered error: Permission denied (os error 13)
    "###);
    Ok(())
}

/// Check that reading arguments from an argfile works
#[cfg(unix)]
#[test]
fn check_input_from_argfile() -> Result<()> {
    let tempdir = TempDir::new()?;

    // Create python files
    let file_a_path = tempdir.path().join("a.py");
    let file_b_path = tempdir.path().join("b.py");
    fs::write(&file_a_path, b"import os")?;
    fs::write(&file_b_path, b"print('hello, world!')")?;

    // Create a the input file for argfile to expand
    let input_file_path = tempdir.path().join("file_paths.txt");
    fs::write(
        &input_file_path,
        format!("{}\n{}", file_a_path.display(), file_b_path.display()),
    )?;

    // Generate the args with the argfile notation
    let args = vec![
        "check".to_string(),
        "--no-cache".to_string(),
        "--isolated".to_string(),
        format!("@{}", &input_file_path.display()),
    ];

    insta::with_settings!({filters => vec![
        (file_a_path.display().to_string().as_str(), "/path/to/a.py"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(args)
            .pass_stdin(""), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        /path/to/a.py:1:8: F401 [*] `os` imported but unused
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
    });

    Ok(())
}

#[test]
fn check_hints_hidden_unsafe_fixes() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args([
            "-",
            "--output-format=text",
            "--isolated",
            "--select",
            "F601,UP034",
            "--no-cache",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    -:2:7: UP034 [*] Avoid extraneous parentheses
    Found 2 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn check_hints_hidden_unsafe_fixes_with_no_safe_fixes() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["-", "--output-format", "text", "--no-cache", "--isolated", "--select", "F601"])
        .pass_stdin("x = {'a': 1, 'a': 1}\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn check_shows_unsafe_fixes_with_opt_in() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args([
            "-",
            "--output-format=text",
            "--isolated",
            "--select",
            "F601,UP034",
            "--no-cache",
            "--unsafe-fixes",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 [*] Dictionary key literal `'a'` repeated
    -:2:7: UP034 [*] Avoid extraneous parentheses
    Found 2 errors.
    [*] 2 fixable with the --fix option.

    ----- stderr -----
    "###);
}

#[test]
fn fix_applies_safe_fixes_by_default() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601,UP034",
                "--fix",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    x = {'a': 1, 'a': 1}
    print('foo')

    ----- stderr -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    Found 2 errors (1 fixed, 1 remaining).
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).
    "###);
}

#[test]
fn fix_applies_unsafe_fixes_with_opt_in() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601,UP034",
                "--fix",
                "--unsafe-fixes",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    x = {'a': 1}
    print('foo')

    ----- stderr -----
    Found 2 errors (2 fixed, 0 remaining).
    "###);
}

#[test]
fn fix_does_not_apply_display_only_fixes() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "B006",
                "--fix",
            ])
            .pass_stdin("def add_to_list(item, some_list=[]): ..."),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    def add_to_list(item, some_list=[]): ...
    ----- stderr -----
    -:1:33: B006 Do not use mutable data structures for argument defaults
    Found 1 error.
    "###);
}

#[test]
fn fix_does_not_apply_display_only_fixes_with_unsafe_fixes_enabled() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "B006",
                "--fix",
                "--unsafe-fixes",
            ])
            .pass_stdin("def add_to_list(item, some_list=[]): ..."),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    def add_to_list(item, some_list=[]): ...
    ----- stderr -----
    -:1:33: B006 Do not use mutable data structures for argument defaults
    Found 1 error.
    "###);
}

#[test]
fn fix_only_unsafe_fixes_available() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601",
                "--fix",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    x = {'a': 1, 'a': 1}
    print(('foo'))

    ----- stderr -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).
    "###);
}

#[test]
fn fix_only_flag_applies_safe_fixes_by_default() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601,UP034",
                "--fix-only",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    x = {'a': 1, 'a': 1}
    print('foo')

    ----- stderr -----
    Fixed 1 error (1 additional fix available with `--unsafe-fixes`).
    "###);
}

#[test]
fn fix_only_flag_applies_unsafe_fixes_with_opt_in() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601,UP034",
                "--fix-only",
                "--unsafe-fixes",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    x = {'a': 1}
    print('foo')

    ----- stderr -----
    Fixed 2 errors.
    "###);
}

#[test]
fn diff_shows_safe_fixes_by_default() {
    assert_cmd_snapshot!(
    Command::new(get_cargo_bin(BIN_NAME))
        .args([
            "-",
            "--output-format",
            "text",
            "--isolated",
            "--no-cache",
            "--select",
            "F601,UP034",
            "--diff",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    @@ -1,2 +1,2 @@
     x = {'a': 1, 'a': 1}
    -print(('foo'))
    +print('foo')


    ----- stderr -----
    Would fix 1 error (1 additional fix available with `--unsafe-fixes`).
    "###
    );
}

#[test]
fn diff_shows_unsafe_fixes_with_opt_in() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "F601,UP034",
                "--diff",
                "--unsafe-fixes",
            ])
            .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    @@ -1,2 +1,2 @@
    -x = {'a': 1, 'a': 1}
    -print(('foo'))
    +x = {'a': 1}
    +print('foo')


    ----- stderr -----
    Would fix 2 errors.
    "###
    );
}

#[test]
fn diff_does_not_show_display_only_fixes_with_unsafe_fixes_enabled() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args([
                "-",
                "--output-format",
                "text",
                "--isolated",
                "--no-cache",
                "--select",
                "B006",
                "--diff",
                "--unsafe-fixes",
            ])
            .pass_stdin("def add_to_list(item, some_list=[]): ..."),
            @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[test]
fn diff_only_unsafe_fixes_available() {
    assert_cmd_snapshot!(
    Command::new(get_cargo_bin(BIN_NAME))
        .args([
            "-",
            "--output-format",
            "text",
            "--isolated",
            "--no-cache",
            "--select",
            "F601",
            "--diff",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No errors would be fixed (1 fix available with `--unsafe-fixes`).
    "###
    );
}

#[test]
fn check_extend_unsafe_fixes() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
extend-unsafe-fixes = ["UP034"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .args([
            "--output-format",
            "text",
            "--no-cache",
            "--select",
            "F601,UP034",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    -:2:7: UP034 Avoid extraneous parentheses
    Found 2 errors.
    No fixes available (2 hidden fixes can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn check_extend_safe_fixes() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
extend-safe-fixes = ["F601"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .args([
            "--output-format",
            "text",
            "--no-cache",
            "--select",
            "F601,UP034",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 [*] Dictionary key literal `'a'` repeated
    -:2:7: UP034 [*] Avoid extraneous parentheses
    Found 2 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn check_extend_unsafe_fixes_conflict_with_extend_safe_fixes() -> Result<()> {
    // Adding a rule to both options should result in it being treated as unsafe
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
extend-unsafe-fixes = ["UP034"]
extend-safe-fixes = ["UP034"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .args([
            "--output-format",
            "text",
            "--no-cache",
            "--select",
            "F601,UP034",
        ])
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    -:2:7: UP034 Avoid extraneous parentheses
    Found 2 errors.
    No fixes available (2 hidden fixes can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);

    Ok(())
}
