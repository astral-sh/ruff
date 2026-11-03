from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from benchmark.projects import Project
from benchmark.targets import (
    FILE_LIST,
    PYRIGHT_CONFIG,
    neutralize_file_directives,
    write_cli_targets,
)


class CliTargetsTest(unittest.TestCase):
    def test_checkers_get_the_same_source_files(self):
        project = Project(
            name="sample",
            repository="",
            revision="",
            install_arguments=[],
            python_version="3.12",
            include=["pkg"],
            exclude=["pkg/skip", "pkg/**/tests/**"],
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for name in (
                "pkg/api.py",
                "pkg/api.pyi",
                "pkg/plain.py",
                "pkg/assets/data.json",
                "pkg/stub only.pyi",
                "pkg/sub.pyi",
                "pkg/sub/__init__.pyi",
                "pkg/native.pyi",
                "pkg/native/build.py",
                "pkg/skip/excluded.py",
                "pkg/sub/tests/excluded.py",
                "outside.py",
            ):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            (root / "pkg/untracked.py").touch()
            # Exercise the symlink entry on hosts without permission to create symlinks.
            blob = subprocess.check_output(
                ["git", "hash-object", "-w", "--stdin"],
                cwd=root,
                input="api.py",
                text=True,
            ).strip()
            subprocess.run(
                [
                    "git",
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    f"120000,{blob},pkg/link.py",
                ],
                cwd=root,
                check=True,
            )

            native_project = write_cli_targets(project, root)
            files = (root / FILE_LIST).read_text(encoding="utf-8").splitlines()
            self.assertEqual(
                files,
                [
                    "./pkg/api.pyi",
                    "./pkg/native.pyi",
                    "./pkg/plain.py",
                    "./pkg/stub only.pyi",
                    "./pkg/sub/__init__.pyi",
                ],
            )
            config = json.loads((root / PYRIGHT_CONFIG).read_text(encoding="utf-8"))
            self.assertEqual(config["include"], ["pkg"])
            self.assertEqual(
                native_project.exclude,
                [
                    "pkg/api.py",
                    "pkg/link.py",
                    "pkg/native",
                    "pkg/skip",
                    "pkg/sub.pyi",
                    "pkg/sub/tests",
                    "pkg/untracked.py",
                ],
            )
            self.assertEqual(config["exclude"], native_project.exclude)
            self.assertTrue(config["analyzeUnannotatedFunctions"])

            with self.assertRaisesRegex(ValueError, "No Python source files"):
                write_cli_targets(project._replace(exclude=["pkg"]), root)

            (root / "pkg/untracked[1].py").touch()
            with self.assertRaisesRegex(ValueError, "Unsupported benchmark exclusion"):
                write_cli_targets(project, root)

    def test_native_discovery_excludes_generated_python_subtrees(self):
        project = Project(
            name="sample",
            repository="",
            revision="",
            install_arguments=[],
            python_version="3.12",
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            (root / "pkg").mkdir()
            (root / "pkg/main.py").touch()
            (root / "assets").mkdir()
            (root / "assets/data.json").touch()
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            dependency = root / "venv/lib/site-packages/dependency"
            dependency.mkdir(parents=True)
            (dependency / "__init__.py").touch()
            (root / "pkg/untracked.py").touch()

            native = write_cli_targets(project, root)
            self.assertEqual(native.include, ["."])
            self.assertEqual(native.exclude, ["pkg/untracked.py", "venv"])
            self.assertEqual(
                (root / FILE_LIST).read_text(encoding="utf-8"), "./pkg/main.py\n"
            )

    def test_file_directives_preserve_line_suppressions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            original = (
                "# mypy: ignore-errors\n"
                "# pyrefly: ignore-errors\n"
                "# pyright: reportAssignmentType=false\n"
                '"""Example:\n# mypy: ignore-errors\n"""\n'
                'text = "# mypy: ignore-errors"\n'
                'number: int = "wrong"  # type: ignore[assignment]\n'
                'other: int = "wrong"  # pyright: ignore[reportAssignmentType]\n'
            )
            (root / "main.py").write_text(original)
            (root / "whole.py").write_text('# type: ignore\nvalue: int = "wrong"\n')
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            neutralize_file_directives(root)
            normalized = (root / "main.py").read_text()
            self.assertEqual(normalized.count("\n"), original.count("\n"))
            self.assertIn(
                '"""Example:\n# benchmark: mypy: ignore-errors\n"""', normalized
            )
            self.assertIn('text = "# mypy: ignore-errors"', normalized)
            self.assertIn("# type: ignore[assignment]", normalized)
            self.assertIn("# pyright: ignore[reportAssignmentType]", normalized)
            self.assertTrue(normalized.startswith("# benchmark: mypy: ignore-errors"))
            self.assertTrue(
                (root / "whole.py").read_text().startswith("# benchmark: type: ignore")
            )
            report = (root / "benchmark-source-overrides.json").read_bytes()
            neutralize_file_directives(root)
            self.assertEqual((root / "main.py").read_text(), normalized)
            self.assertEqual(
                (root / "benchmark-source-overrides.json").read_bytes(), report
            )
