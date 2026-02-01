"""program/module to trace Python program or function execution

Sample use, command line:
  trace.py -c -f counts --ignore-dir '$prefix' spam.py eggs
  trace.py -t --ignore-dir '$prefix' spam.py eggs
  trace.py --trackcalls spam.py eggs

Sample use, programmatically
  import sys

  # create a Trace object, telling it what to ignore, and whether to
  # do tracing or line-counting or both.
  tracer = trace.Trace(ignoredirs=[sys.base_prefix, sys.base_exec_prefix,],
                       trace=0, count=1)
  # run the new command using the given tracer
  tracer.run('main()')
  # make a report, placing output in /tmp
  r = tracer.results()
  r.write_results(show_missing=True, coverdir="/tmp")
"""

import sys
import types
from _typeshed import Incomplete, StrPath, TraceFunction
from collections.abc import Callable, Iterable, Mapping, Sequence
from typing import Any, TypeVar
from typing_extensions import ParamSpec, TypeAlias

__all__ = ["Trace", "CoverageResults"]

_T = TypeVar("_T")
_P = ParamSpec("_P")
_FileModuleFunction: TypeAlias = tuple[str, str | None, str]

class CoverageResults:
    counts: dict[tuple[str, int], int]
    counter: dict[tuple[str, int], int]
    calledfuncs: dict[_FileModuleFunction, int]
    callers: dict[tuple[_FileModuleFunction, _FileModuleFunction], int]
    inifile: StrPath | None
    outfile: StrPath | None
    def __init__(
        self,
        counts: dict[tuple[str, int], int] | None = None,
        calledfuncs: dict[_FileModuleFunction, int] | None = None,
        infile: StrPath | None = None,
        callers: dict[tuple[_FileModuleFunction, _FileModuleFunction], int] | None = None,
        outfile: StrPath | None = None,
    ) -> None: ...  # undocumented
    def update(self, other: CoverageResults) -> None:
        """Merge in the data from another CoverageResults"""
    if sys.version_info >= (3, 13):
        def write_results(
            self,
            show_missing: bool = True,
            summary: bool = False,
            coverdir: StrPath | None = None,
            *,
            ignore_missing_files: bool = False,
        ) -> None:
            """
            Write the coverage results.

            :param show_missing: Show lines that had no hits.
            :param summary: Include coverage summary per module.
            :param coverdir: If None, the results of each module are placed in its
                             directory, otherwise it is included in the directory
                             specified.
            :param ignore_missing_files: If True, counts for files that no longer
                             exist are silently ignored. Otherwise, a missing file
                             will raise a FileNotFoundError.
            """
    else:
        def write_results(self, show_missing: bool = True, summary: bool = False, coverdir: StrPath | None = None) -> None:
            """
            Write the coverage results.

            :param show_missing: Show lines that had no hits.
            :param summary: Include coverage summary per module.
            :param coverdir: If None, the results of each module are placed in its
                             directory, otherwise it is included in the directory
                             specified.
            """

    def write_results_file(
        self, path: StrPath, lines: Sequence[str], lnotab: Any, lines_hit: Mapping[int, int], encoding: str | None = None
    ) -> tuple[int, int]:
        """Return a coverage results file in path."""

    def is_ignored_filename(self, filename: str) -> bool:  # undocumented
        """Return True if the filename does not refer to a file
        we want to have reported.
        """

class _Ignore:
    def __init__(self, modules: Iterable[str] | None = None, dirs: Iterable[StrPath] | None = None) -> None: ...
    def names(self, filename: str, modulename: str) -> int: ...

class Trace:
    inifile: StrPath | None
    outfile: StrPath | None
    ignore: _Ignore
    counts: dict[str, int]
    pathtobasename: dict[Incomplete, Incomplete]
    donothing: int
    trace: int
    start_time: int | None
    globaltrace: TraceFunction
    localtrace: TraceFunction
    def __init__(
        self,
        count: int = 1,
        trace: int = 1,
        countfuncs: int = 0,
        countcallers: int = 0,
        ignoremods: Sequence[str] = (),
        ignoredirs: Sequence[str] = (),
        infile: StrPath | None = None,
        outfile: StrPath | None = None,
        timing: bool = False,
    ) -> None:
        """
        @param count true iff it should count number of times each
                     line is executed
        @param trace true iff it should print out each line that is
                     being counted
        @param countfuncs true iff it should just output a list of
                     (filename, modulename, funcname,) for functions
                     that were called at least once;  This overrides
                     'count' and 'trace'
        @param ignoremods a list of the names of modules to ignore
        @param ignoredirs a list of the names of directories to ignore
                     all of the (recursive) contents of
        @param infile file from which to read stored counts to be
                     added into the results
        @param outfile file in which to write the results
        @param timing true iff timing information be displayed
        """

    def run(self, cmd: str | types.CodeType) -> None: ...
    def runctx(
        self, cmd: str | types.CodeType, globals: Mapping[str, Any] | None = None, locals: Mapping[str, Any] | None = None
    ) -> None: ...
    def runfunc(self, func: Callable[_P, _T], /, *args: _P.args, **kw: _P.kwargs) -> _T: ...
    def file_module_function_of(self, frame: types.FrameType) -> _FileModuleFunction: ...
    def globaltrace_trackcallers(self, frame: types.FrameType, why: str, arg: Any) -> None:
        """Handler for call events.

        Adds information about who called who to the self._callers dict.
        """

    def globaltrace_countfuncs(self, frame: types.FrameType, why: str, arg: Any) -> None:
        """Handler for call events.

        Adds (filename, modulename, funcname) to the self._calledfuncs dict.
        """

    def globaltrace_lt(self, frame: types.FrameType, why: str, arg: Any) -> None:
        """Handler for call events.

        If the code block being entered is to be ignored, returns 'None',
        else returns self.localtrace.
        """

    def localtrace_trace_and_count(self, frame: types.FrameType, why: str, arg: Any) -> TraceFunction: ...
    def localtrace_trace(self, frame: types.FrameType, why: str, arg: Any) -> TraceFunction: ...
    def localtrace_count(self, frame: types.FrameType, why: str, arg: Any) -> TraceFunction: ...
    def results(self) -> CoverageResults: ...
