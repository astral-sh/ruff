import io
import os
import sys
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

import fuzz


class TestDeferredMinimization(unittest.TestCase):
    def test_detect_then_minimize(self) -> None:
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            failures = root / "failures"
            report = root / "report.md"
            corpus.mkdir()
            (corpus / "0.py").write_text("pass\n")
            original = "unused = 42\npanic()\n"
            (corpus / "1.py").write_text(original)
            (corpus / "2.py").write_text("flaky()\n")

            def run(*args: str) -> fuzz.ExitCode:
                options = [
                    "--bin=ty",
                    "--quiet",
                    "--test-executable=ty-new",
                    f"--baseline-executable={sys.executable}",
                    "--only-new-bugs",
                    *args,
                ]
                return fuzz.run_fuzzer(fuzz.parse_args(options))

            def detects_bug(source: str, args: fuzz.ResolvedCliArgs) -> bool:
                self.assertIsNotNone(args.baseline_executable_path)
                return "panic()" in source or (
                    "flaky()" in source and not args.minimize_failures
                )

            with (
                # Run both minimizers with the same replacement bug predicate.
                patch("concurrent.futures.ProcessPoolExecutor", ThreadPoolExecutor),
                patch.object(fuzz, "contains_reportable_bug", side_effect=detects_bug),
                patch.object(fuzz, "generate_random_code", side_effect=AssertionError),
                patch.dict(
                    os.environ,
                    GITHUB_ACTIONS="true",
                    GITHUB_SERVER_URL="https://github.com",
                    GITHUB_REPOSITORY="astral-sh/ruff",
                    GITHUB_RUN_ID="123",
                ),
                redirect_stdout(io.StringIO()) as output,
            ):
                with patch.object(fuzz, "minimize_repro", side_effect=AssertionError):
                    status = run(
                        f"--corpus-dir={corpus}",
                        f"--save-failures={failures}",
                        "0",
                        "1",
                        "2",
                    )
                self.assertEqual(status, 1)
                self.assertEqual(
                    sorted(p.name for p in failures.iterdir()), ["1.py", "2.py"]
                )
                self.assertEqual((failures / "1.py").read_text(), original)
                self.assertIn("::error::ty fuzzing failed on seed 1", output.getvalue())

                with patch(
                    "fuzz.subprocess.check_output",
                    side_effect=[
                        '{"commit_info":{"commit_hash":"abc123"}}',
                        "def456\n",
                    ],
                ):
                    status = run(
                        f"--minimize-failures={failures}",
                        f"--write-github-issue={report}",
                    )
                self.assertEqual(status, 0)
                body = report.read_text()
                self.assertIn("### Seed: 1", body)
                self.assertIn("panic()", body)
                self.assertNotIn("unused = 42", body)
                self.assertIn("Seed: 2 (original source", body)
                self.assertIn("flaky()", body)
