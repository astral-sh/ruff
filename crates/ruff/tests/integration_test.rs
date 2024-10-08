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
use ruff::args::Args;
#[cfg(unix)]
use ruff::run;

const BIN_NAME: &str = "ruff";

fn ruff_cmd() -> Command {
    Command::new(get_cargo_bin(BIN_NAME))
}

/// Builder for `ruff check` commands.
#[derive(Debug, Default)]
struct RuffCheck<'a> {
    output_format: Option<&'a str>,
    config: Option<&'a Path>,
    filename: Option<&'a str>,
    args: Vec<&'a str>,
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
        self.output_format = Some(format);
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
        cmd.arg("check");
        if let Some(output_format) = self.output_format {
            cmd.args(["--output-format", output_format]);
        }
        cmd.arg("--no-cache");

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
    All checks passed!

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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

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
        r"
import foo   # unused import
",
    )?;
    fs::write(
        tempdir.path().join("bar.py"),
        r"
import bar   # unused import
",
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--isolated", "--no-cache", "--select", "F401"]).current_dir(tempdir.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    bar.py:2:8: F401 [*] `bar` imported but unused
      |
    2 | import bar   # unused import
      |        ^^^ F401
      |
      = help: Remove unused import: `bar`

    foo.py:2:8: F401 [*] `foo` imported but unused
      |
    2 | import foo   # unused import
      |        ^^^ F401
      |
      = help: Remove unused import: `foo`

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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

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
    All checks passed!

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
    "import os\n",
    "print(1)"
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
    "import sys\n",
    "print(x)"
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
    {
     "cells": [
      {
       "cell_type": "code",
       "execution_count": 1,
       "id": "dccc687c-96e2-4604-b957-a8a89b5bec06",
       "metadata": {},
       "outputs": [],
       "source": [
        "print(1)"
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
        "print(x)"
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
    }
    ----- stderr -----
    Jupyter.ipynb:cell 3:1:7: F821 Undefined name `x`
      |
    1 | print(x)
      |       ^ F821
      |

    Found 3 errors (2 fixed, 1 remaining).
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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

    Jupyter.py:cell 3:1:8: F401 [*] `sys` imported but unused
      |
    1 | import sys
      |        ^^^ F401
      |
      = help: Remove unused import: `sys`

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
      |
    1 | import os
      |        ^^ F401
      |
      = help: Remove unused import: `os`

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
      |
    1 | import sys
    2 | 
    3 | if (1, 2):
      |    ^^^^^^ F634
    4 |      print(sys.version)
      |

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
    All checks passed!
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
fn stdin_parse_error() {
    let mut cmd = RuffCheck::default().build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("from foo import\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:16: SyntaxError: Expected one or more symbol names after import
      |
    1 | from foo import
      |                ^
      |

    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn stdin_multiple_parse_error() {
    let mut cmd = RuffCheck::default().build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("from foo import\nbar =\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:16: SyntaxError: Expected one or more symbol names after import
      |
    1 | from foo import
      |                ^
    2 | bar =
      |

    -:2:6: SyntaxError: Expected an expression
      |
    1 | from foo import
    2 | bar =
      |      ^
      |

    Found 2 errors.

    ----- stderr -----
    "###);
}

#[test]
fn parse_error_not_included() {
    // Select any rule except for `E999`, syntax error should still be shown.
    let mut cmd = RuffCheck::default().args(["--select=I"]).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("foo =\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:6: SyntaxError: Expected an expression
      |
    1 | foo =
      |      ^
      |

    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn deprecated_parse_error_selection() {
    let mut cmd = RuffCheck::default().args(["--select=E999"]).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("foo =\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:6: SyntaxError: Expected an expression
      |
    1 | foo =
      |      ^
      |

    Found 1 error.

    ----- stderr -----
    warning: Rule `E999` is deprecated and will be removed in a future release. Syntax errors will always be shown regardless of whether this rule is selected or not.
    "###);
}

#[test]
fn full_output_preview() {
    let mut cmd = RuffCheck::default().args(["--preview"]).build();
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
fn full_output_preview_config() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r"
[tool.ruff]
preview = true
",
    )?;
    let mut cmd = RuffCheck::default().config(&pyproject_toml).build();
    assert_cmd_snapshot!(cmd.pass_stdin("l = 1"), @r###"
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
    Ok(())
}

#[test]
fn full_output_format() {
    let mut cmd = RuffCheck::default().output_format("full").build();
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
fn rule_f401() {
    assert_cmd_snapshot!(ruff_cmd().args(["rule", "F401"]));
}

#[test]
fn rule_invalid_rule_name() {
    assert_cmd_snapshot!(ruff_cmd().args(["rule", "RUF404"]), @r###"
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
    1	F401	[*] unused-import

    ----- stderr -----
    "###);
}

#[test]
fn show_statistics_json() {
    let mut cmd = RuffCheck::default()
        .args([
            "--select",
            "F401",
            "--statistics",
            "--output-format",
            "json",
        ])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("import sys\nimport os\n\nprint(os.getuid())\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    [
      {
        "code": "F401",
        "name": "unused-import",
        "count": 1,
        "fixable": true
      }
    ]

    ----- stderr -----
    "###);
}

#[test]
fn preview_enabled_prefix() {
    // All the RUF9XX test rules should be triggered
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF9", "--output-format=concise", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF900 Hey this is a stable test rule.
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    -:1:1: RUF911 Hey this is a preview test rule.
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 6 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn preview_enabled_all() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "ALL", "--output-format=concise", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: D100 Missing docstring in public module
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:1: RUF900 Hey this is a stable test rule.
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    -:1:1: RUF911 Hey this is a preview test rule.
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 8 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    "###);
}

#[test]
fn preview_enabled_direct() {
    // Should be enabled without warning
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF911", "--output-format=concise", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF911 Hey this is a preview test rule.
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn preview_disabled_direct() {
    // RUFF911 is preview so we should warn without selecting
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF911", "--output-format=concise"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: Selection `RUF911` has no effect because preview is not enabled.
    "###);
}

#[test]
fn preview_disabled_prefix_empty() {
    // Warns that the selection is empty since all of the RUF91 rules are in preview
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF91", "--output-format=concise"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: Selection `RUF91` has no effect because preview is not enabled.
    "###);
}

#[test]
fn preview_disabled_does_not_warn_for_empty_ignore_selections() {
    // Does not warn that the selection is empty since the user is not trying to enable the rule
    let mut cmd = RuffCheck::default()
        .args(["--ignore", "RUF9", "--output-format=concise"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
}

#[test]
fn preview_disabled_does_not_warn_for_empty_fixable_selections() {
    // Does not warn that the selection is empty since the user is not trying to enable the rule
    let mut cmd = RuffCheck::default()
        .args(["--fixable", "RUF9", "--output-format=concise"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
}

#[test]
fn preview_group_selector() {
    // `--select PREVIEW` should error (selector was removed)
    let mut cmd = RuffCheck::default()
        .args([
            "--select",
            "PREVIEW",
            "--preview",
            "--output-format=concise",
        ])
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
    // Should detect stable and unstable rules, RUF9 is more specific than RUF so ignore has no effect
    let mut cmd = RuffCheck::default()
        .args([
            "--select",
            "RUF9",
            "--ignore",
            "RUF",
            "--preview",
            "--output-format=concise",
        ])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF900 Hey this is a stable test rule.
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    -:1:1: RUF911 Hey this is a preview test rule.
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 6 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn removed_direct() {
    // Selection of a removed rule should fail
    let mut cmd = RuffCheck::default().args(["--select", "RUF931"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Rule `RUF931` was removed and cannot be selected.
    "###);
}

#[test]
fn removed_direct_multiple() {
    // Selection of multiple removed rule should fail with a message
    // including all the rules
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF930", "--select", "RUF931"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: The following rules have been removed and cannot be selected:
        - RUF930
        - RUF931

    "###);
}

#[test]
fn removed_indirect() {
    // Selection _including_ a removed rule without matching should not fail
    // nor should the rule be used
    let mut cmd = RuffCheck::default().args(["--select", "RUF93"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
}

#[test]
fn redirect_direct() {
    // Selection of a redirected rule directly should use the new rule and warn
    let mut cmd = RuffCheck::default().args(["--select", "RUF940"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 1 error.

    ----- stderr -----
    warning: `RUF940` has been remapped to `RUF950`.
    "###);
}

#[test]
fn redirect_indirect() {
    // Selection _including_ a redirected rule without matching should not fail
    // nor should the rule be used
    let mut cmd = RuffCheck::default().args(["--select", "RUF94"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
}

#[test]
fn redirect_prefix() {
    // Selection using a redirected prefix should switch to all rules in the
    // new prefix
    let mut cmd = RuffCheck::default().args(["--select", "RUF96"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 1 error.

    ----- stderr -----
    warning: `RUF96` has been remapped to `RUF95`.
    "###);
}

#[test]
fn deprecated_direct() {
    // Selection of a deprecated rule without preview enabled should still work
    // but a warning should be displayed
    let mut cmd = RuffCheck::default().args(["--select", "RUF920"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF920 Hey this is a deprecated test rule.
    Found 1 error.

    ----- stderr -----
    warning: Rule `RUF920` is deprecated and will be removed in a future release.
    "###);
}

#[test]
fn deprecated_multiple_direct() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF920", "--select", "RUF921"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF920 Hey this is a deprecated test rule.
    -:1:1: RUF921 Hey this is another deprecated test rule.
    Found 2 errors.

    ----- stderr -----
    warning: Rule `RUF920` is deprecated and will be removed in a future release.
    warning: Rule `RUF921` is deprecated and will be removed in a future release.
    "###);
}

#[test]
fn deprecated_indirect() {
    // `RUF92` includes deprecated rules but should not warn
    // since it is not a "direct" selection
    let mut cmd = RuffCheck::default().args(["--select", "RUF92"]).build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF920 Hey this is a deprecated test rule.
    -:1:1: RUF921 Hey this is another deprecated test rule.
    Found 2 errors.

    ----- stderr -----
    "###);
}

#[test]
fn deprecated_direct_preview_enabled() {
    // Direct selection of a deprecated rule in preview should fail
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF920", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Selection of deprecated rule `RUF920` is not allowed when preview is enabled.
    "###);
}

#[test]
fn deprecated_indirect_preview_enabled() {
    // `RUF920` is deprecated and should be off by default in preview.
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF92", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
}

#[test]
fn deprecated_multiple_direct_preview_enabled() {
    // Direct selection of the deprecated rules in preview should fail with
    // a message listing all of the rule codes
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF920", "--select", "RUF921", "--preview"])
        .build();
    assert_cmd_snapshot!(cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Selection of deprecated rules is not allowed when preview is enabled. Remove selection of:
    	- RUF920
    	- RUF921

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
        .truncate(true)
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
        vec![
            format!("Failed to read {}/pyproject.toml", tempdir.path().display()),
            "Permission denied (os error 13)".to_string()
        ],
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
    All checks passed!

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

    // Create the input file for argfile to expand
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
          |
        1 | import os
          |        ^^ F401
          |
          = help: Remove unused import: `os`

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
        .args(["--select", "RUF901,RUF902"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 2 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn check_hints_hidden_unsafe_fixes_with_no_safe_fixes() {
    let mut cmd = RuffCheck::default().args(["--select", "RUF902"]).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);
}

#[test]
fn check_no_hint_for_hidden_unsafe_fixes_when_disabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--no-unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 2 errors.
    [*] 1 fixable with the --fix option.

    ----- stderr -----
    "###);
}

#[test]
fn check_no_hint_for_hidden_unsafe_fixes_with_no_safe_fixes_when_disabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF902", "--no-unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\n"),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn check_shows_unsafe_fixes_with_opt_in() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 [*] Hey this is a stable test rule with an unsafe fix.
    Found 2 errors.
    [*] 2 fixable with the --fix option.

    ----- stderr -----
    "###);
}

#[test]
fn fix_applies_safe_fixes_by_default() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    # fix from stable-test-rule-safe-fix

    ----- stderr -----
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 2 errors (1 fixed, 1 remaining).
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).
    "###);
}

#[test]
fn fix_applies_unsafe_fixes_with_opt_in() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--fix", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    # fix from stable-test-rule-unsafe-fix
    # fix from stable-test-rule-safe-fix

    ----- stderr -----
    Found 2 errors (2 fixed, 0 remaining).
    "###);
}

#[test]
fn fix_does_not_apply_display_only_fixes() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF903", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("def add_to_list(item, some_list=[]): ..."),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    def add_to_list(item, some_list=[]): ...
    ----- stderr -----
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    Found 1 error.
    "###);
}

#[test]
fn fix_does_not_apply_display_only_fixes_with_unsafe_fixes_enabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF903", "--fix", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("def add_to_list(item, some_list=[]): ..."),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    def add_to_list(item, some_list=[]): ...
    ----- stderr -----
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    Found 1 error.
    "###);
}

#[test]
fn fix_only_unsafe_fixes_available() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF902", "--fix"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).
    "###);
}

#[test]
fn fix_only_flag_applies_safe_fixes_by_default() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--fix-only"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    # fix from stable-test-rule-safe-fix

    ----- stderr -----
    Fixed 1 error (1 additional fix available with `--unsafe-fixes`).
    "###);
}

#[test]
fn fix_only_flag_applies_unsafe_fixes_with_opt_in() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--fix-only", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    # fix from stable-test-rule-unsafe-fix
    # fix from stable-test-rule-safe-fix

    ----- stderr -----
    Fixed 2 errors.
    "###);
}

#[test]
fn diff_shows_safe_fixes_by_default() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--diff"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    @@ -0,0 +1 @@
    +# fix from stable-test-rule-safe-fix


    ----- stderr -----
    Would fix 1 error (1 additional fix available with `--unsafe-fixes`).
    "###
    );
}

#[test]
fn diff_shows_unsafe_fixes_with_opt_in() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF901,RUF902", "--diff", "--unsafe-fixes"])
        .build();
    assert_cmd_snapshot!(cmd,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    @@ -0,0 +1,2 @@
    +# fix from stable-test-rule-unsafe-fix
    +# fix from stable-test-rule-safe-fix


    ----- stderr -----
    Would fix 2 errors.
    "###
    );
}

#[test]
fn diff_does_not_show_display_only_fixes_with_unsafe_fixes_enabled() {
    let mut cmd = RuffCheck::default()
        .args(["--select", "RUF903", "--diff", "--unsafe-fixes"])
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
        .args(["--select", "RUF902", "--diff"])
        .build();
    assert_cmd_snapshot!(cmd,
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
extend-unsafe-fixes = ["RUF901"]
"#,
    )?;

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "RUF901,RUF902"])
        .build();
    assert_cmd_snapshot!(cmd,
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
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
extend-safe-fixes = ["RUF902"]
"#,
    )?;

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "RUF901,RUF902"])
        .build();
    assert_cmd_snapshot!(cmd,
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 [*] Hey this is a stable test rule with an unsafe fix.
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
extend-unsafe-fixes = ["RUF902"]
extend-safe-fixes = ["RUF902"]
"#,
    )?;

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "RUF901,RUF902"])
        .build();
    assert_cmd_snapshot!(cmd,
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    Found 2 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

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
extend-unsafe-fixes = ["RUF", "RUF901"]
extend-safe-fixes = ["RUF9"]
"#,
    )?;

    let mut cmd = RuffCheck::default()
        .config(&ruff_toml)
        .args(["--select", "RUF9"])
        .build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = {'a': 1, 'a': 1}\nprint(('foo'))\nprint(str('foo'))\nisinstance(x, (int, str))\n"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: RUF900 Hey this is a stable test rule.
    -:1:1: RUF901 Hey this is a stable test rule with a safe fix.
    -:1:1: RUF902 [*] Hey this is a stable test rule with an unsafe fix.
    -:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    -:1:1: RUF920 Hey this is a deprecated test rule.
    -:1:1: RUF921 Hey this is another deprecated test rule.
    -:1:1: RUF950 Hey this is a test rule that was redirected from another.
    Found 7 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

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
    All checks passed!

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
      |
    2 | def log(x, base) -> float:
      |     ^^^ D417
    3 |     """Calculate natural log of a value
      |

    Found 1 error.

    ----- stderr -----
    "###
    );
    Ok(())
}

#[test]
fn fix_preview() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
preview = true
explicit-preview-rules = true
select = ["RUF017"]
"#,
    )?;

    let mut cmd = RuffCheck::default().config(&ruff_toml).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = [1, 2, 3]\ny = [4, 5, 6]\nsum([x, y], [])"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:3:1: RUF017 Avoid quadratic list summation
      |
    1 | x = [1, 2, 3]
    2 | y = [4, 5, 6]
    3 | sum([x, y], [])
      | ^^^^^^^^^^^^^^^ RUF017
      |
      = help: Replace with `functools.reduce`

    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn unfixable_preview() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
preview = true
explicit-preview-rules = true
select = ["RUF017"]
unfixable = ["RUF"]
"#,
    )?;

    let mut cmd = RuffCheck::default().config(&ruff_toml).build();
    assert_cmd_snapshot!(cmd
        .pass_stdin("x = [1, 2, 3]\ny = [4, 5, 6]\nsum([x, y], [])"),
            @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:3:1: RUF017 Avoid quadratic list summation
      |
    1 | x = [1, 2, 3]
    2 | y = [4, 5, 6]
    3 | sum([x, y], [])
      | ^^^^^^^^^^^^^^^ RUF017
      |
      = help: Replace with `functools.reduce`

    Found 1 error.

    ----- stderr -----
    "###);

    Ok(())
}
