from __future__ import annotations

from typing import TYPE_CHECKING

import mdformat

if TYPE_CHECKING:
    import argparse

    from markdown_it import MarkdownIt
    from mdformat.renderer import RenderContext, RenderTreeNode


class NoEscapeTextPlugin:
    r"""Overrides the default text formatting behavior of mdformat.

    By default mdformat will escape any markdown special character found in a
    text block, e.g., <. Some of these characters are found in our
    documentation, and when escaped (i.e. \<) will be rendered incorrectly by
    mkdocs, i.e., the backslash will appear in the render. Because our only
    purpose in using mdformat is to manage the line-breaks, it makes sense to
    override its text formatting behavior.
    """

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
