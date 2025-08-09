"""
Main program for 2to3.
"""

from _typeshed import FileDescriptorOrPath
from collections.abc import Container, Iterable, Iterator, Mapping, Sequence
from logging import _ExcInfoType
from typing import AnyStr, Literal

from . import refactor as refactor

def diff_texts(a: str, b: str, filename: str) -> Iterator[str]:
    """Return a unified diff of two strings."""

class StdoutRefactoringTool(refactor.MultiprocessRefactoringTool):
    """
    A refactoring tool that can avoid overwriting its input files.
    Prints output to stdout.

    Output files can optionally be written to a different directory and or
    have an extra file suffix appended to their name for use in situations
    where you do not want to replace the input files.
    """

    nobackups: bool
    show_diffs: bool
    def __init__(
        self,
        fixers: Iterable[str],
        options: Mapping[str, object] | None,
        explicit: Container[str] | None,
        nobackups: bool,
        show_diffs: bool,
        input_base_dir: str = "",
        output_dir: str = "",
        append_suffix: str = "",
    ) -> None:
        """
        Args:
            fixers: A list of fixers to import.
            options: A dict with RefactoringTool configuration.
            explicit: A list of fixers to run even if they are explicit.
            nobackups: If true no backup '.bak' files will be created for those
                files that are being refactored.
            show_diffs: Should diffs of the refactoring be printed to stdout?
            input_base_dir: The base directory for all input files.  This class
                will strip this path prefix off of filenames before substituting
                it with output_dir.  Only meaningful if output_dir is supplied.
                All files processed by refactor() must start with this path.
            output_dir: If supplied, all converted files will be written into
                this directory tree instead of input_base_dir.
            append_suffix: If supplied, all files output by this tool will have
                this appended to their filename.  Useful for changing .py to
                .py3 for example by passing append_suffix='3'.
        """
    # Same as super.log_error and Logger.error
    def log_error(  # type: ignore[override]
        self,
        msg: str,
        *args: Iterable[str],
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None: ...
    # Same as super.write_file but without default values
    def write_file(  # type: ignore[override]
        self, new_text: str, filename: FileDescriptorOrPath, old_text: str, encoding: str | None
    ) -> None: ...
    # filename has to be str
    def print_output(self, old: str, new: str, filename: str, equal: bool) -> None: ...  # type: ignore[override]

def warn(msg: object) -> None: ...
def main(fixer_pkg: str, args: Sequence[AnyStr] | None = None) -> Literal[0, 1, 2]:
    """Main program.

    Args:
        fixer_pkg: the name of a package where the fixers are located.
        args: optional; a list of command line arguments. If omitted,
              sys.argv[1:] is used.

    Returns a suggested exit status (0, 1, 2).
    """
