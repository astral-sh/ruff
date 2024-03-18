from __future__ import annotations

import re
from pathlib import Path
from typing import TYPE_CHECKING

import mdformat

if TYPE_CHECKING:
    import argparse

    from markdown_it import MarkdownIt
    from mdformat.renderer import RenderContext, RenderTreeNode

ROOT_DIR = Path(__file__).resolve().parent.parent


def dir_name(linter_name: str) -> str:
    return linter_name.replace("-", "_")


def pascal_case(linter_name: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in linter_name.split("-"))


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(
        f"_{word.lower()}" if word.isupper() else word for word in name
    ).lstrip("_")


def get_indent(line: str) -> str:
    return re.match(r"^\s*", line).group()  # type: ignore[union-attr]


class NoEscapeTextPlugin:
    """Overrides the default text formatting behavior of mdformat."""

    def __init__(self: NoEscapeTextPlugin) -> None:
        self.POSTPROCESSORS = {"text": NoEscapeTextPlugin.text}
        self.RENDERERS = {}

    @staticmethod
    def add_cli_options(parser: argparse.ArgumentParser) -> None:
        pass

    @staticmethod
    def update_mdit(mdit: MarkdownIt) -> None:
        pass

    @staticmethod
    def text(_text: str, node: RenderTreeNode, _context: RenderContext) -> str:
        return node.content


def add_no_escape_text_plugin() -> None:
    """Add NoEscapeTextPlugin to the list of mdformat extensions."""
    mdformat.plugins.PARSER_EXTENSIONS["no-escape-text"] = NoEscapeTextPlugin()
