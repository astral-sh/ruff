"""Base class for fixers (optional, but recommended)."""

from _typeshed import Incomplete, StrPath
from abc import ABCMeta, abstractmethod
from collections.abc import MutableMapping
from typing import ClassVar, Literal, TypeVar

from .pytree import Base, Leaf, Node

_N = TypeVar("_N", bound=Base)

class BaseFix:
    """Optional base class for fixers.

    The subclass name must be FixFooBar where FooBar is the result of
    removing underscores and capitalizing the words of the fix name.
    For example, the class name for a fixer named 'has_key' should be
    FixHasKey.
    """

    PATTERN: ClassVar[str | None]
    pattern: Incomplete | None
    pattern_tree: Incomplete | None
    options: Incomplete | None
    filename: Incomplete | None
    numbers: Incomplete
    used_names: Incomplete
    order: ClassVar[Literal["post", "pre"]]
    explicit: ClassVar[bool]
    run_order: ClassVar[int]
    keep_line_order: ClassVar[bool]
    BM_compatible: ClassVar[bool]
    syms: Incomplete
    log: Incomplete
    def __init__(self, options: MutableMapping[str, Incomplete], log: list[str]) -> None:
        """Initializer.  Subclass may override.

        Args:
            options: a dict containing the options passed to RefactoringTool
            that could be used to customize the fixer through the command line.
            log: a list to append warnings and other messages to.
        """

    def compile_pattern(self) -> None:
        """Compiles self.PATTERN into self.pattern.

        Subclass may override if it doesn't want to use
        self.{pattern,PATTERN} in .match().
        """

    def set_filename(self, filename: StrPath) -> None:
        """Set the filename.

        The main refactoring tool should call this.
        """

    def match(self, node: _N) -> Literal[False] | dict[str, _N]:
        """Returns match for a given parse tree node.

        Should return a true or false object (not necessarily a bool).
        It may return a non-empty dict of matching sub-nodes as
        returned by a matching pattern.

        Subclass may override.
        """

    @abstractmethod
    def transform(self, node: Base, results: dict[str, Base]) -> Node | Leaf | None:
        """Returns the transformation for a given parse tree node.

        Args:
          node: the root of the parse tree that matched the fixer.
          results: a dict mapping symbolic names to part of the match.

        Returns:
          None, or a node that is a modified copy of the
          argument node.  The node argument may also be modified in-place to
          effect the same change.

        Subclass *must* override.
        """

    def new_name(self, template: str = "xxx_todo_changeme") -> str:
        """Return a string suitable for use as an identifier

        The new name is guaranteed not to conflict with other identifiers.
        """
    first_log: bool
    def log_message(self, message: str) -> None: ...
    def cannot_convert(self, node: Base, reason: str | None = None) -> None:
        """Warn the user that a given chunk of code is not valid Python 3,
        but that it cannot be converted automatically.

        First argument is the top-level node for the code in question.
        Optional second argument is why it can't be converted.
        """

    def warning(self, node: Base, reason: str) -> None:
        """Used for warning the user about possible uncertainty in the
        translation.

        First argument is the top-level node for the code in question.
        Optional second argument is why it can't be converted.
        """

    def start_tree(self, tree: Node, filename: StrPath) -> None:
        """Some fixers need to maintain tree-wide state.
        This method is called once, at the start of tree fix-up.

        tree - the root node of the tree to be processed.
        filename - the name of the file the tree came from.
        """

    def finish_tree(self, tree: Node, filename: StrPath) -> None:
        """Some fixers need to maintain tree-wide state.
        This method is called once, at the conclusion of tree fix-up.

        tree - the root node of the tree to be processed.
        filename - the name of the file the tree came from.
        """

class ConditionalFix(BaseFix, metaclass=ABCMeta):
    """Base class for fixers which not execute if an import is found."""

    skip_on: ClassVar[str | None]
    def start_tree(self, tree: Node, filename: StrPath, /) -> None: ...
    def should_skip(self, node: Base) -> bool: ...
