#![cfg(not(target_family = "wasm"))]

use std::fs;
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
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

fn ruff_cmd() -> Command {
    Command::new(get_cargo_bin(BIN_NAME))
}

/// Builder for `ruff check` commands.
#[derive(Debug)]
struct RuffCheck<'a> {
    output_format: &'a str,
    config: Option<&'a Path>,
    filename: Option<&'a str>,
    args: Vec<&'a str>,
}

impl<'a> Default for RuffCheck<'a> {
    fn default() -> RuffCheck<'a> {
        RuffCheck {
            output_format: "text",
            config: None,
            filename: None,
            args: vec![],
        }
    }
}

impl<'a> RuffCheck<'a> {
    /// Set the `--config` option.
    #[must_use]
    fn config(mut self, config: &'a Path) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the `--output-format` option.
    #[must_use]
    fn output_format(mut self, format: &'a str) -> Self {
        self.output_format = format;
        self
    }

    /// Set the input file to pass to `ruff check`.
    #[must_use]
    fn filename(mut self, filename: &'a str) -> Self {
        self.filename = Some(filename);
        self
    }

    /// Set the list of positional arguments.
    #[must_use]
    fn args(mut self, args: impl IntoIterator<Item = &'a str>) -> Self {
        self.args = args.into_iter().collect();
        self
    }

    /// Generate a [`Command`] for the `ruff check` command.
    fn build(self) -> Command {
        let mut cmd = ruff_cmd();
        cmd.args(["--output-format", self.output_format, "--no-cache"]);
        if let Some(path) = self.config {
            cmd.arg("--config");
            cmd.arg(path);
        } else {
            cmd.arg("--isolated");
        }
        if let Some(filename) = self.filename {
            cmd.arg(filename);
        } else {
            cmd.arg("-");
        }
        cmd.args(self.args);
        cmd
    }
}

#[test]
fn stdin_success() {
    let mut cmd = RuffCheck::default().args([]).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin(""), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[test]
fn stdin_error() {
    let mut cmd = RuffCheck::default().args([]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--stdin-filename", "F401.py"])
        .build();
    assert_cmd_snapshot!(cmd
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

#[test]
fn check_default_files() -> Result<()> {
    let tempdir = TempDir::new()?;
    fs::write(
        tempdir.path().join("foo.py"),
        r#"
import foo   # unused import
"#,
    )?;
    fs::write(
        tempdir.path().join("bar.py"),
        r#"
import bar   # unused import
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--isolated", "--no-cache", "--select", "F401"]).current_dir(tempdir.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    bar.py:2:8: F401 [*] `bar` imported but unused
    foo.py:2:8: F401 [*] `foo` imported but unused
    Found 2 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn check_warn_stdin_filename_with_files() {
    let mut cmd = RuffCheck::default()
        .args(["--stdin-filename", "F401.py"])
        .filename("foo.py")
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    F401.py:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: Ignoring file foo.py in favor of standard input.
    "###);
}

/// Raise `TCH` errors in `.py` files ...
#[test]
fn stdin_source_type_py() {
    let mut cmd = RuffCheck::default()
        .args(["--stdin-filename", "TCH.py"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--stdin-filename", "TCH.pyi", "--select", "TCH"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let directory = path_dedot::CWD.to_str().unwrap();
    let binding = Path::new(directory).join("F401.py");
    let file_path = binding.display();

    let mut cmd = RuffCheck::default()
        .output_format("json")
        .args(["--stdin-filename", "F401.py"])
        .build();

    insta::with_settings!({filters => vec![
        (file_path.to_string().as_str(), "/path/to/F401.py"),
    ]}, {
        assert_cmd_snapshot!(cmd.pass_stdin("import os\n"));
    });
}

#[test]
fn stdin_fix_py() {
    let mut cmd = RuffCheck::default().args(["--fix"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--fix", "--stdin-filename", "Jupyter.ipynb"])
        .build();
    assert_cmd_snapshot!(cmd
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
fn stdin_override_parser_ipynb() {
    let mut cmd = RuffCheck::default()
        .args(["--extension", "py:ipynb", "--stdin-filename", "Jupyter.py"])
        .build();
    assert_cmd_snapshot!(cmd
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
    success: false
    exit_code: 1
    ----- stdout -----
    Jupyter.py:cell 1:1:8: F401 [*] `os` imported but unused
    Jupyter.py:cell 3:1:8: F401 [*] `sys` imported but unused
    Found 2 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

#[test]
fn stdin_override_parser_py() {
    let mut cmd = RuffCheck::default()
        .args([
            "--extension",
            "ipynb:python",
            "--stdin-filename",
            "F401.ipynb",
        ])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    F401.ipynb:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
}

#[test]
fn stdin_fix_when_not_fixable_should_still_print_contents() {
    let mut cmd = RuffCheck::default().args(["--fix"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--fix"]).build();
    assert_cmd_snapshot!(cmd
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
    assert_cmd_snapshot!(ruff_cmd()
        .args(["format", "--stdin-filename", "Jupyter.ipynb", "--isolated"])
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
    let mut cmd = RuffCheck::default().args(["--show-source"]).build();
    assert_cmd_snapshot!(cmd
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
    assert_cmd_snapshot!(ruff_cmd().args(["--explain", "F401"]));
}
#[test]
fn explain_status_codes_ruf404() {
    assert_cmd_snapshot!(ruff_cmd().args(["--explain", "RUF404"]), @r###"
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F401", "--statistics"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "E"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "ALL"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "E225"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "NURSERY"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "NURSERY", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "E", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "ALL", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "E225", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "FURB145"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "CPY"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--ignore", "CPY"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--fixable", "CPY"]).build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "PREVIEW", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "E", "--ignore", "PREVIEW", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .filename(unreadable_dir.to_str().unwrap())
        .args([])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
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
    let argfile = format!("@{}", &input_file_path.display());
    let mut cmd = RuffCheck::default().filename(argfile.as_ref()).build();
    insta::with_settings!({filters => vec![
        (file_a_path.display().to_string().as_str(), "/path/to/a.py"),
    ]}, {
        assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default().args(["--select", "F601"]).build();
    assert_cmd_snapshot!(cmd
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
fn check_no_hint_for_hidden_unsafe_fixes_when_disabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--no-unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    -:2:7: UP034 [*] Avoid extraneous parentheses
    Found 2 errors.
    [*] 1 fixable with the --fix option.

    ----- stderr -----
    "###);
}

#[test]
fn check_no_hint_for_hidden_unsafe_fixes_with_no_safe_fixes_when_disabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601", "--no-unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn check_shows_unsafe_fixes_with_opt_in() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--fix", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "B006", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "B006", "--fix", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--fix-only"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--fix-only", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--diff"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601,UP034", "--diff", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "B006", "--diff", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
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
    let mut cmd = RuffCheck::default()
        .args(["--select", "F601", "--diff"])
        .build();
    assert_cmd_snapshot!(cmd
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

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "F601,UP034"])
        .build();
    assert_cmd_snapshot!(cmd
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

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "F601,UP034"])
        .build();
    assert_cmd_snapshot!(cmd
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

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "F601,UP034"])
        .build();
    assert_cmd_snapshot!(cmd
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
fn check_extend_unsafe_fixes_conflict_with_extend_safe_fixes_by_specificity() -> Result<()> {
    // Adding a rule to one option with a more specific selector should override the other option
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
target-version = "py310"
[lint]
extend-unsafe-fixes = ["UP", "UP034"]
extend-safe-fixes = ["UP03"]
"#,
    )?;

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "F601,UP018,UP034,UP038"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\nprint(str('foo'))\nisinstance(x, (int, str))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:14: F601 Dictionary key literal `'a'` repeated
    -:2:7: UP034 Avoid extraneous parentheses
    -:3:7: UP018 Unnecessary `str` call (rewrite as a literal)
    -:4:1: UP038 [*] Use `X | Y` in `isinstance` call instead of `(X, Y)`
    Found 4 errors.
    [*] 1 fixable with the `--fix` option (3 hidden fixes can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn check_docstring_conventions_overrides() -> Result<()> {
    // But if we explicitly select it, we override the convention
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.pydocstyle]
convention = "numpy"
"#,
    )?;

    let stdin = r#"
def log(x, base) -> float:
    """Calculate natural log of a value

    Parameters
    ----------
    x :
        Hello
    """
    return math.log(x)
"#;

    // If we only select the prefix, then everything passes
    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "D41"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin(stdin), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );

    // But if we select the exact code, we get an error
    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "D417"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin(stdin), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:5: D417 Missing argument description in the docstring for `log`: `base`
    Found 1 error.

    ----- stderr -----
    "###
    );
    Ok(())
}
