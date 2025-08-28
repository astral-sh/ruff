"""Generate Python documentation in HTML or text for interactive use.

At the Python interactive prompt, calling help(thing) on a Python object
documents the object, and calling help() starts up an interactive
help session.

Or, at the shell command line outside of Python:

Run "pydoc <name>" to show documentation on something.  <name> may be
the name of a function, module, package, or a dotted reference to a
class or function within a module or module in a package.  If the
argument contains a path segment delimiter (e.g. slash on Unix,
backslash on Windows) it is treated as the path to a Python source file.

Run "pydoc -k <keyword>" to search for a keyword in the synopsis lines
of all available modules.

Run "pydoc -n <hostname>" to start an HTTP server with the given
hostname (default: localhost) on the local machine.

Run "pydoc -p <port>" to start an HTTP server on the given port on the
local machine.  Port number 0 can be used to get an arbitrary unused port.

Run "pydoc -b" to start an HTTP server on an arbitrary unused port and
open a web browser to interactively browse documentation.  Combine with
the -n and -p options to control the hostname and port used.

Run "pydoc -w <name>" to write out the HTML documentation for a module
to a file named "<name>.html".

Module docs for core modules are assumed to be in

    https://docs.python.org/X.Y/library/

This can be overridden by setting the PYTHONDOCS environment variable
to a different URL or to a local directory containing the Library
Reference Manual pages.
"""

import sys
from _typeshed import OptExcInfo, SupportsWrite, Unused
from abc import abstractmethod
from builtins import list as _list  # "list" conflicts with method name
from collections.abc import Callable, Container, Mapping, MutableMapping
from reprlib import Repr
from types import MethodType, ModuleType, TracebackType
from typing import IO, Any, AnyStr, Final, NoReturn, Protocol, TypeVar, type_check_only
from typing_extensions import TypeGuard, deprecated

__all__ = ["help"]

_T = TypeVar("_T")

__author__: Final[str]
__date__: Final[str]
__version__: Final[str]
__credits__: Final[str]

@type_check_only
class _Pager(Protocol):
    def __call__(self, text: str, title: str = "") -> None: ...

def pathdirs() -> list[str]:
    """Convert sys.path into a list of absolute, existing, unique paths."""

def getdoc(object: object) -> str:
    """Get the doc string or comments for an object."""

def splitdoc(doc: AnyStr) -> tuple[AnyStr, AnyStr]:
    """Split a doc string into a synopsis line (if any) and the rest."""

def classname(object: object, modname: str) -> str:
    """Get a class name and qualify it with a module name if necessary."""

def isdata(object: object) -> bool:
    """Check if an object is of a type that probably means it's data."""

def replace(text: AnyStr, *pairs: AnyStr) -> AnyStr:
    """Do a series of global replacements on a string."""

def cram(text: str, maxlen: int) -> str:
    """Omit part of a string if needed to make it fit in a maximum length."""

def stripid(text: str) -> str:
    """Remove the hexadecimal id from a Python object representation."""

def allmethods(cl: type) -> MutableMapping[str, MethodType]: ...
def visiblename(name: str, all: Container[str] | None = None, obj: object = None) -> bool:
    """Decide whether to show documentation on a variable."""

def classify_class_attrs(object: object) -> list[tuple[str, str, type, str]]:
    """Wrap inspect.classify_class_attrs, with fixup for data descriptors and bound methods."""

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13.")
    def ispackage(path: str) -> bool:  # undocumented
        """Guess whether a path refers to a package directory."""

else:
    def ispackage(path: str) -> bool:  # undocumented
        """Guess whether a path refers to a package directory."""

def source_synopsis(file: IO[AnyStr]) -> AnyStr | None:
    """Return the one-line summary of a file object, if present"""

def synopsis(filename: str, cache: MutableMapping[str, tuple[int, str]] = {}) -> str | None:
    """Get the one-line summary out of a module file."""

class ErrorDuringImport(Exception):
    """Errors that occurred while trying to import something to document it."""

    filename: str
    exc: type[BaseException] | None
    value: BaseException | None
    tb: TracebackType | None
    def __init__(self, filename: str, exc_info: OptExcInfo) -> None: ...

def importfile(path: str) -> ModuleType:
    """Import a Python source file or compiled file given its path."""

def safeimport(path: str, forceload: bool = ..., cache: MutableMapping[str, ModuleType] = {}) -> ModuleType | None:
    """Import a module; handle errors; return None if the module isn't found.

    If the module *is* found but an exception occurs, it's wrapped in an
    ErrorDuringImport exception and reraised.  Unlike __import__, if a
    package path is specified, the module at the end of the path is returned,
    not the package at the beginning.  If the optional 'forceload' argument
    is 1, we reload the module from disk (unless it's a dynamic extension).
    """

class Doc:
    PYTHONDOCS: str
    def document(self, object: object, name: str | None = None, *args: Any) -> str:
        """Generate documentation for an object."""

    def fail(self, object: object, name: str | None = None, *args: Any) -> NoReturn:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docmodule(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docclass(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docroutine(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docother(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docproperty(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    @abstractmethod
    def docdata(self, object: object, name: str | None = None, *args: Any) -> str:
        """Raise an exception for unimplemented types."""

    def getdocloc(self, object: object, basedir: str = ...) -> str | None:
        """Return the location of module docs or None"""

class HTMLRepr(Repr):
    """Class for safely making an HTML representation of a Python object."""

    def __init__(self) -> None: ...
    def escape(self, text: str) -> str: ...
    def repr(self, object: object) -> str: ...
    def repr1(self, x: object, level: complex) -> str: ...
    def repr_string(self, x: str, level: complex) -> str: ...
    def repr_str(self, x: str, level: complex) -> str: ...
    def repr_instance(self, x: object, level: complex) -> str: ...
    def repr_unicode(self, x: AnyStr, level: complex) -> str: ...

class HTMLDoc(Doc):
    """Formatter class for HTML documentation."""

    _repr_instance: HTMLRepr
    repr = _repr_instance.repr
    escape = _repr_instance.escape
    def page(self, title: str, contents: str) -> str:
        """Format an HTML page."""
    if sys.version_info >= (3, 11):
        def heading(self, title: str, extras: str = "") -> str:
            """Format a page heading."""

        def section(
            self,
            title: str,
            cls: str,
            contents: str,
            width: int = 6,
            prelude: str = "",
            marginalia: str | None = None,
            gap: str = "&nbsp;",
        ) -> str:
            """Format a section with a heading."""

        def multicolumn(self, list: list[_T], format: Callable[[_T], str]) -> str:
            """Format a list of items into a multi-column list."""
    else:
        def heading(self, title: str, fgcol: str, bgcol: str, extras: str = "") -> str:
            """Format a page heading."""

        def section(
            self,
            title: str,
            fgcol: str,
            bgcol: str,
            contents: str,
            width: int = 6,
            prelude: str = "",
            marginalia: str | None = None,
            gap: str = "&nbsp;",
        ) -> str:
            """Format a section with a heading."""

        def multicolumn(self, list: list[_T], format: Callable[[_T], str], cols: int = 4) -> str:
            """Format a list of items into a multi-column list."""

    def bigsection(self, title: str, *args: Any) -> str:
        """Format a section with a big heading."""

    def preformat(self, text: str) -> str:
        """Format literal preformatted text."""

    def grey(self, text: str) -> str: ...
    def namelink(self, name: str, *dicts: MutableMapping[str, str]) -> str:
        """Make a link for an identifier, given name-to-URL mappings."""

    def classlink(self, object: object, modname: str) -> str:
        """Make a link for a class."""

    def modulelink(self, object: object) -> str:
        """Make a link for a module."""

    def modpkglink(self, modpkginfo: tuple[str, str, bool, bool]) -> str:
        """Make a link for a module or package to display in an index."""

    def markup(
        self,
        text: str,
        escape: Callable[[str], str] | None = None,
        funcs: Mapping[str, str] = {},
        classes: Mapping[str, str] = {},
        methods: Mapping[str, str] = {},
    ) -> str:
        """Mark up some plain text, given a context of symbols to look for.
        Each context dictionary maps object names to anchor names.
        """

    def formattree(self, tree: list[tuple[type, tuple[type, ...]] | list[Any]], modname: str, parent: type | None = None) -> str:
        """Produce HTML for a class tree as given by inspect.getclasstree()."""

    def docmodule(self, object: object, name: str | None = None, mod: str | None = None, *ignored: Unused) -> str:
        """Produce HTML documentation for a module object."""

    def docclass(
        self,
        object: object,
        name: str | None = None,
        mod: str | None = None,
        funcs: Mapping[str, str] = {},
        classes: Mapping[str, str] = {},
        *ignored: Unused,
    ) -> str:
        """Produce HTML documentation for a class object."""

    def formatvalue(self, object: object) -> str:
        """Format an argument default value as text."""

    def docother(self, object: object, name: str | None = None, mod: Any | None = None, *ignored: Unused) -> str:
        """Produce HTML documentation for a data object."""
    if sys.version_info >= (3, 11):
        def docroutine(  # type: ignore[override]
            self,
            object: object,
            name: str | None = None,
            mod: str | None = None,
            funcs: Mapping[str, str] = {},
            classes: Mapping[str, str] = {},
            methods: Mapping[str, str] = {},
            cl: type | None = None,
            homecls: type | None = None,
        ) -> str:
            """Produce HTML documentation for a function or method object."""

        def docproperty(
            self, object: object, name: str | None = None, mod: str | None = None, cl: Any | None = None, *ignored: Unused
        ) -> str:
            """Produce html documentation for a data descriptor."""

        def docdata(
            self, object: object, name: str | None = None, mod: Any | None = None, cl: Any | None = None, *ignored: Unused
        ) -> str:
            """Produce html documentation for a data descriptor."""
    else:
        def docroutine(  # type: ignore[override]
            self,
            object: object,
            name: str | None = None,
            mod: str | None = None,
            funcs: Mapping[str, str] = {},
            classes: Mapping[str, str] = {},
            methods: Mapping[str, str] = {},
            cl: type | None = None,
        ) -> str:
            """Produce HTML documentation for a function or method object."""

        def docproperty(self, object: object, name: str | None = None, mod: str | None = None, cl: Any | None = None) -> str:  # type: ignore[override]
            """Produce html documentation for a data descriptor."""

        def docdata(self, object: object, name: str | None = None, mod: Any | None = None, cl: Any | None = None) -> str:  # type: ignore[override]
            """Produce html documentation for a data descriptor."""
    if sys.version_info >= (3, 11):
        def parentlink(self, object: type | ModuleType, modname: str) -> str:
            """Make a link for the enclosing class or module."""

    def index(self, dir: str, shadowed: MutableMapping[str, bool] | None = None) -> str:
        """Generate an HTML index for a directory of modules."""

    def filelink(self, url: str, path: str) -> str:
        """Make a link to source file."""

class TextRepr(Repr):
    """Class for safely making a text representation of a Python object."""

    def __init__(self) -> None: ...
    def repr1(self, x: object, level: complex) -> str: ...
    def repr_string(self, x: str, level: complex) -> str: ...
    def repr_str(self, x: str, level: complex) -> str: ...
    def repr_instance(self, x: object, level: complex) -> str: ...

class TextDoc(Doc):
    """Formatter class for text documentation."""

    _repr_instance: TextRepr
    repr = _repr_instance.repr
    def bold(self, text: str) -> str:
        """Format a string in bold by overstriking."""

    def indent(self, text: str, prefix: str = "    ") -> str:
        """Indent text by prepending a given prefix to each line."""

    def section(self, title: str, contents: str) -> str:
        """Format a section with a given heading."""

    def formattree(
        self, tree: list[tuple[type, tuple[type, ...]] | list[Any]], modname: str, parent: type | None = None, prefix: str = ""
    ) -> str:
        """Render in text a class tree as returned by inspect.getclasstree()."""

    def docclass(self, object: object, name: str | None = None, mod: str | None = None, *ignored: Unused) -> str:
        """Produce text documentation for a given class object."""

    def formatvalue(self, object: object) -> str:
        """Format an argument default value as text."""
    if sys.version_info >= (3, 11):
        def docroutine(  # type: ignore[override]
            self,
            object: object,
            name: str | None = None,
            mod: str | None = None,
            cl: Any | None = None,
            homecls: Any | None = None,
        ) -> str:
            """Produce text documentation for a function or method object."""

        def docmodule(self, object: object, name: str | None = None, mod: Any | None = None, *ignored: Unused) -> str:
            """Produce text documentation for a given module object."""

        def docproperty(
            self, object: object, name: str | None = None, mod: Any | None = None, cl: Any | None = None, *ignored: Unused
        ) -> str:
            """Produce text documentation for a data descriptor."""

        def docdata(
            self, object: object, name: str | None = None, mod: str | None = None, cl: Any | None = None, *ignored: Unused
        ) -> str:
            """Produce text documentation for a data descriptor."""

        def docother(
            self,
            object: object,
            name: str | None = None,
            mod: str | None = None,
            parent: str | None = None,
            *ignored: Unused,
            maxlen: int | None = None,
            doc: Any | None = None,
        ) -> str:
            """Produce text documentation for a data object."""
    else:
        def docroutine(self, object: object, name: str | None = None, mod: str | None = None, cl: Any | None = None) -> str:  # type: ignore[override]
            """Produce text documentation for a function or method object."""

        def docmodule(self, object: object, name: str | None = None, mod: Any | None = None) -> str:  # type: ignore[override]
            """Produce text documentation for a given module object."""

        def docproperty(self, object: object, name: str | None = None, mod: Any | None = None, cl: Any | None = None) -> str:  # type: ignore[override]
            """Produce text documentation for a data descriptor."""

        def docdata(self, object: object, name: str | None = None, mod: str | None = None, cl: Any | None = None) -> str:  # type: ignore[override]
            """Produce text documentation for a data descriptor."""

        def docother(  # type: ignore[override]
            self,
            object: object,
            name: str | None = None,
            mod: str | None = None,
            parent: str | None = None,
            maxlen: int | None = None,
            doc: Any | None = None,
        ) -> str:
            """Produce text documentation for a data object."""

if sys.version_info >= (3, 13):
    def pager(text: str, title: str = "") -> None:
        """The first time this is called, determine what kind of pager to use."""

else:
    def pager(text: str) -> None:
        """The first time this is called, determine what kind of pager to use."""

def plain(text: str) -> str:
    """Remove boldface formatting from text."""

def describe(thing: Any) -> str:
    """Produce a short description of the given thing."""

def locate(path: str, forceload: bool = ...) -> object:
    """Locate an object by name or dotted path, importing as necessary."""

if sys.version_info >= (3, 13):
    def get_pager() -> _Pager:
        """Decide what method to use for paging through text."""

    def pipe_pager(text: str, cmd: str, title: str = "") -> None:
        """Page through text by feeding it to another program."""

    def tempfile_pager(text: str, cmd: str, title: str = "") -> None:
        """Page through text by invoking a program on a temporary file."""

    def tty_pager(text: str, title: str = "") -> None:
        """Page through text on a text terminal."""

    def plain_pager(text: str, title: str = "") -> None:
        """Simply print unformatted text.  This is the ultimate fallback."""
    # For backwards compatibility.
    getpager = get_pager
    pipepager = pipe_pager
    tempfilepager = tempfile_pager
    ttypager = tty_pager
    plainpager = plain_pager
else:
    def getpager() -> Callable[[str], None]:
        """Decide what method to use for paging through text."""

    def pipepager(text: str, cmd: str) -> None:
        """Page through text by feeding it to another program."""

    def tempfilepager(text: str, cmd: str) -> None:
        """Page through text by invoking a program on a temporary file."""

    def ttypager(text: str) -> None:
        """Page through text on a text terminal."""

    def plainpager(text: str) -> None:
        """Simply print unformatted text.  This is the ultimate fallback."""

text: TextDoc
html: HTMLDoc

def resolve(thing: str | object, forceload: bool = ...) -> tuple[object, str] | None:
    """Given an object or a path to an object, get the object and its name."""

def render_doc(
    thing: str | object, title: str = "Python Library Documentation: %s", forceload: bool = ..., renderer: Doc | None = None
) -> str:
    """Render text documentation, given an object or a path to an object."""

if sys.version_info >= (3, 11):
    def doc(
        thing: str | object,
        title: str = "Python Library Documentation: %s",
        forceload: bool = ...,
        output: SupportsWrite[str] | None = None,
        is_cli: bool = False,
    ) -> None:
        """Display text documentation, given an object or a path to an object."""

else:
    def doc(
        thing: str | object,
        title: str = "Python Library Documentation: %s",
        forceload: bool = ...,
        output: SupportsWrite[str] | None = None,
    ) -> None:
        """Display text documentation, given an object or a path to an object."""

def writedoc(thing: str | object, forceload: bool = ...) -> None:
    """Write HTML documentation to a file in the current directory."""

def writedocs(dir: str, pkgpath: str = "", done: Any | None = None) -> None:
    """Write out HTML documentation for all modules in a directory tree."""

class Helper:
    keywords: dict[str, str | tuple[str, str]]
    symbols: dict[str, str]
    topics: dict[str, str | tuple[str, ...]]
    def __init__(self, input: IO[str] | None = None, output: IO[str] | None = None) -> None: ...
    @property
    def input(self) -> IO[str]: ...
    @property
    def output(self) -> IO[str]: ...
    def __call__(self, request: str | Helper | object = ...) -> None: ...
    def interact(self) -> None: ...
    def getline(self, prompt: str) -> str:
        """Read one line, using input() when appropriate."""
    if sys.version_info >= (3, 11):
        def help(self, request: Any, is_cli: bool = False) -> None: ...
    else:
        def help(self, request: Any) -> None: ...

    def intro(self) -> None: ...
    def list(self, items: _list[str], columns: int = 4, width: int = 80) -> None: ...
    def listkeywords(self) -> None: ...
    def listsymbols(self) -> None: ...
    def listtopics(self) -> None: ...
    def showtopic(self, topic: str, more_xrefs: str = "") -> None: ...
    def showsymbol(self, symbol: str) -> None: ...
    def listmodules(self, key: str = "") -> None: ...

help: Helper

class ModuleScanner:
    """An interruptible scanner that searches module synopses."""

    quit: bool
    def run(
        self,
        callback: Callable[[str | None, str, str], object],
        key: str | None = None,
        completer: Callable[[], object] | None = None,
        onerror: Callable[[str], object] | None = None,
    ) -> None: ...

def apropos(key: str) -> None:
    """Print all the one-line module summaries that contain a substring."""

def ispath(x: object) -> TypeGuard[str]: ...
def cli() -> None:
    """Command-line interface (looks at sys.argv to decide what to do)."""
