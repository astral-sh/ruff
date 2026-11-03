from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from benchmark.projects import Project
from benchmark.targets import FILE_LIST, PYRIGHT_CONFIG, write_cli_targets


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
