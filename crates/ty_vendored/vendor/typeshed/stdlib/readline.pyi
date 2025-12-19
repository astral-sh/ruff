"""Importing this module enables command line editing using libedit readline."""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Callable, Sequence
from typing import Literal
from typing_extensions import TypeAlias

if sys.platform != "win32":
    _Completer: TypeAlias = Callable[[str, int], str | None]
    _CompDisp: TypeAlias = Callable[[str, Sequence[str], int], None]

    def parse_and_bind(string: str, /) -> None:
        """Execute the init line provided in the string argument."""

    def read_init_file(filename: StrOrBytesPath | None = None, /) -> None:
        """Execute a readline initialization file.

        The default filename is the last filename used.
        """

    def get_line_buffer() -> str:
        """Return the current contents of the line buffer."""

    def insert_text(string: str, /) -> None:
        """Insert text into the line buffer at the cursor position."""

    def redisplay() -> None:
        """Change what's displayed on the screen to reflect contents of the line buffer."""

    def read_history_file(filename: StrOrBytesPath | None = None, /) -> None:
        """Load a readline history file.

        The default filename is ~/.history.
        """

    def write_history_file(filename: StrOrBytesPath | None = None, /) -> None:
        """Save a readline history file.

        The default filename is ~/.history.
        """

    def append_history_file(nelements: int, filename: StrOrBytesPath | None = None, /) -> None:
        """Append the last nelements items of the history list to file.

        The default filename is ~/.history.
        """

    def get_history_length() -> int:
        """Return the maximum number of lines that will be written to the history file."""

    def set_history_length(length: int, /) -> None:
        """Set the maximal number of lines which will be written to the history file.

        A negative length is used to inhibit history truncation.
        """

    def clear_history() -> None:
        """Clear the current readline history."""

    def get_current_history_length() -> int:
        """Return the current (not the maximum) length of history."""

    def get_history_item(index: int, /) -> str:
        """Return the current contents of history item at one-based index."""

    def remove_history_item(pos: int, /) -> None:
        """Remove history item given by its zero-based position."""

    def replace_history_item(pos: int, line: str, /) -> None:
        """Replaces history item given by its position with contents of line.

        pos is zero-based.
        """

    def add_history(string: str, /) -> None:
        """Add an item to the history buffer."""

    def set_auto_history(enabled: bool, /) -> None:
        """Enables or disables automatic history."""

    def set_startup_hook(function: Callable[[], object] | None = None, /) -> None:
        """Set or remove the function invoked by the rl_startup_hook callback.

        The function is called with no arguments just
        before readline prints the first prompt.
        """

    def set_pre_input_hook(function: Callable[[], object] | None = None, /) -> None:
        """Set or remove the function invoked by the rl_pre_input_hook callback.

        The function is called with no arguments after the first prompt
        has been printed and just before readline starts reading input
        characters.
        """

    def set_completer(function: _Completer | None = None, /) -> None:
        """Set or remove the completer function.

        The function is called as function(text, state),
        for state in 0, 1, 2, ..., until it returns a non-string.
        It should return the next possible completion starting with 'text'.
        """

    def get_completer() -> _Completer | None:
        """Get the current completer function."""

    def get_completion_type() -> int:
        """Get the type of completion being attempted."""

    def get_begidx() -> int:
        """Get the beginning index of the completion scope."""

    def get_endidx() -> int:
        """Get the ending index of the completion scope."""

    def set_completer_delims(string: str, /) -> None:
        """Set the word delimiters for completion."""

    def get_completer_delims() -> str:
        """Get the word delimiters for completion."""

    def set_completion_display_matches_hook(function: _CompDisp | None = None, /) -> None:
        """Set or remove the completion display function.

        The function is called as
          function(substitution, [matches], longest_match_length)
        once each time matches need to be displayed.
        """
    if sys.version_info >= (3, 13):
        backend: Literal["readline", "editline"]
