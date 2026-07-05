"""Tests for the ruff.format() Python API."""
from __future__ import annotations

import pytest

import ruff


class TestFormatBasic:
    """Basic formatting behavior."""

    def test_format_adds_spaces_around_operators(self):
        result = ruff.format("x = 1+2\n")
        assert result == "x = 1 + 2\n"

    def test_format_normalizes_whitespace(self):
        result = ruff.format("y =   3\n")
        assert result == "y = 3\n"

    def test_format_multiline(self):
        code = "x = 1+2\ny =   3\n"
        result = ruff.format(code)
        assert "x = 1 + 2\n" in result
        assert "y = 3\n" in result

    def test_format_already_formatted(self):
        code = "x = 1 + 2\n"
        result = ruff.format(code)
        assert result == code

    def test_format_empty_string(self):
        result = ruff.format("")
        assert result == ""

    def test_format_trailing_newline(self):
        result = ruff.format("x = 1")
        assert result.endswith("\n")


class TestFormatFilename:
    """Filename-based source type detection."""

    def test_format_pyi_stub(self):
        # .pyi files should format without error
        code = "x: int = 1\n"
        result = ruff.format(code, filename="stub.pyi")
        assert isinstance(result, str)

    def test_format_py_file(self):
        code = "x = 1+2\n"
        result = ruff.format(code, filename="script.py")
        assert result == "x = 1 + 2\n"

    def test_format_no_filename(self):
        # Default behavior (no filename) should work
        code = "x = 1+2\n"
        result = ruff.format(code)
        assert result == "x = 1 + 2\n"


class TestFormatErrors:
    """Error handling for invalid input."""

    def test_format_syntax_error(self):
        with pytest.raises(ValueError, match="Failed to format"):
            ruff.format("def f(:\n  pass\n")

    def test_format_incomplete_expression(self):
        with pytest.raises(ValueError):
            ruff.format("x = (\n")
