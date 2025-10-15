"""Refactoring framework.

Used as a main program, this can refactor any number of files and/or
recursively descend down directories.  Imported as a module, this
provides infrastructure to write your own refactoring tool.
"""

from _typeshed import FileDescriptorOrPath, StrPath, SupportsGetItem
from collections.abc import Container, Generator, Iterable, Mapping
from logging import Logger, _ExcInfoType
from multiprocessing import JoinableQueue
from multiprocessing.synchronize import Lock
from typing import Any, ClassVar, Final, NoReturn, overload

from .btm_matcher import BottomMatcher
from .fixer_base import BaseFix
from .pgen2.driver import Driver
from .pgen2.grammar import Grammar
from .pytree import Node

def get_all_fix_names(fixer_pkg: str, remove_prefix: bool = True) -> list[str]:
    """Return a sorted list of all available fix names in the given package."""

def get_fixers_from_package(pkg_name: str) -> list[str]:
    """
    Return the fully qualified names for fixers in the package pkg_name.
    """

class FixerError(Exception):
    """A fixer could not be loaded."""

class RefactoringTool:
    CLASS_PREFIX: ClassVar[str]
    FILE_PREFIX: ClassVar[str]
    fixers: Iterable[str]
    explicit: Container[str]
    options: dict[str, Any]
    grammar: Grammar
    write_unchanged_files: bool
    errors: list[tuple[str, Iterable[str], dict[str, _ExcInfoType]]]
    logger: Logger
    fixer_log: list[str]
    wrote: bool
    driver: Driver
    pre_order: list[BaseFix]
    post_order: list[BaseFix]
    files: list[StrPath]
    BM: BottomMatcher
    bmi_pre_order: list[BaseFix]
    bmi_post_order: list[BaseFix]
    def __init__(
        self, fixer_names: Iterable[str], options: Mapping[str, object] | None = None, explicit: Container[str] | None = None
    ) -> None:
        """Initializer.

        Args:
            fixer_names: a list of fixers to import
            options: a dict with configuration.
            explicit: a list of fixers to run even if they are explicit.
        """

    def get_fixers(self) -> tuple[list[BaseFix], list[BaseFix]]:
        """Inspects the options to load the requested patterns and handlers.

        Returns:
          (pre_order, post_order), where pre_order is the list of fixers that
          want a pre-order AST traversal, and post_order is the list that want
          post-order traversal.
        """

    def log_error(self, msg: str, *args: Iterable[str], **kwargs: _ExcInfoType) -> NoReturn:
        """Called when an error occurs."""

    @overload
    def log_message(self, msg: object) -> None:
        """Hook to log a message."""

    @overload
    def log_message(self, msg: str, *args: object) -> None: ...
    @overload
    def log_debug(self, msg: object) -> None: ...
    @overload
    def log_debug(self, msg: str, *args: object) -> None: ...
    def print_output(self, old_text: str, new_text: str, filename: StrPath, equal: bool) -> None:
        """Called with the old version, new version, and filename of a
        refactored file.
        """

    def refactor(self, items: Iterable[str], write: bool = False, doctests_only: bool = False) -> None:
        """Refactor a list of files and directories."""

    def refactor_dir(self, dir_name: str, write: bool = False, doctests_only: bool = False) -> None:
        """Descends down a directory and refactor every Python file found.

        Python files are assumed to have a .py extension.

        Files and subdirectories starting with '.' are skipped.
        """

    def _read_python_source(self, filename: FileDescriptorOrPath) -> tuple[str, str]:
        """
        Do our best to decode a Python source file correctly.
        """

    def refactor_file(self, filename: StrPath, write: bool = False, doctests_only: bool = False) -> None:
        """Refactors a file."""

    def refactor_string(self, data: str, name: str) -> Node | None:
        """Refactor a given input string.

        Args:
            data: a string holding the code to be refactored.
            name: a human-readable name for use in error/log messages.

        Returns:
            An AST corresponding to the refactored input stream; None if
            there were errors during the parse.
        """

    def refactor_stdin(self, doctests_only: bool = False) -> None: ...
    def refactor_tree(self, tree: Node, name: str) -> bool:
        """Refactors a parse tree (modifying the tree in place).

        For compatible patterns the bottom matcher module is
        used. Otherwise the tree is traversed node-to-node for
        matches.

        Args:
            tree: a pytree.Node instance representing the root of the tree
                  to be refactored.
            name: a human-readable name for this tree.

        Returns:
            True if the tree was modified, False otherwise.
        """

    def traverse_by(self, fixers: SupportsGetItem[int, Iterable[BaseFix]] | None, traversal: Iterable[Node]) -> None:
        """Traverse an AST, applying a set of fixers to each node.

        This is a helper method for refactor_tree().

        Args:
            fixers: a list of fixer instances.
            traversal: a generator that yields AST nodes.

        Returns:
            None
        """

    def processed_file(
        self, new_text: str, filename: StrPath, old_text: str | None = None, write: bool = False, encoding: str | None = None
    ) -> None:
        """
        Called when a file has been refactored and there may be changes.
        """

    def write_file(self, new_text: str, filename: FileDescriptorOrPath, old_text: str, encoding: str | None = None) -> None:
        """Writes a string to a file.

        It first shows a unified diff between the old text and the new text, and
        then rewrites the file; the latter is only done if the write option is
        set.
        """
    PS1: Final = ">>> "
    PS2: Final = "... "
    def refactor_docstring(self, input: str, filename: StrPath) -> str:
        """Refactors a docstring, looking for doctests.

        This returns a modified version of the input string.  It looks
        for doctests, which start with a ">>>" prompt, and may be
        continued with "..." prompts, as long as the "..." is indented
        the same as the ">>>".

        (Unfortunately we can't use the doctest module's parser,
        since, like most parsers, it is not geared towards preserving
        the original source.)
        """

    def refactor_doctest(self, block: list[str], lineno: int, indent: int, filename: StrPath) -> list[str]:
        """Refactors one doctest.

        A doctest is given as a block of lines, the first of which starts
        with ">>>" (possibly indented), while the remaining lines start
        with "..." (identically indented).

        """

    def summarize(self) -> None: ...
    def parse_block(self, block: Iterable[str], lineno: int, indent: int) -> Node:
        """Parses a block into a tree.

        This is necessary to get correct line number / offset information
        in the parser diagnostics and embedded into the parse tree.
        """

    def wrap_toks(
        self, block: Iterable[str], lineno: int, indent: int
    ) -> Generator[tuple[int, str, tuple[int, int], tuple[int, int], str], None, None]:
        """Wraps a tokenize stream to systematically modify start/end."""

    def gen_lines(self, block: Iterable[str], indent: int) -> Generator[str, None, None]:
        """Generates lines as expected by tokenize from a list of lines.

        This strips the first len(indent + self.PS1) characters off each line.
        """

class MultiprocessingUnsupported(Exception): ...

class MultiprocessRefactoringTool(RefactoringTool):
    queue: JoinableQueue[None | tuple[Iterable[str], bool | int]] | None
    output_lock: Lock | None
    def refactor(
        self, items: Iterable[str], write: bool = False, doctests_only: bool = False, num_processes: int = 1
    ) -> None: ...
