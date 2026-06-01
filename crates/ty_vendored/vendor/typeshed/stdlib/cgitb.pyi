"""More comprehensive traceback formatting for Python scripts.

To enable this module, do:

    import cgitb; cgitb.enable()

at the top of your script.  The optional arguments to enable() are:

    display     - if true, tracebacks are displayed in the web browser
    logdir      - if set, tracebacks are written to files in this directory
    context     - number of lines of source code to show for each stack frame
    format      - 'text' or 'html' controls the output format

By default, tracebacks are displayed but not saved, the context is 5 lines
and the output format is 'html' (for backwards compatibility with the
original use of this module)

Alternatively, if you have caught an exception and want cgitb to display it
for you, call cgitb.handler().  The optional argument to handler() is a
3-item tuple (etype, evalue, etb) just like the value of sys.exc_info().
The default handler displays output as HTML.

"""

from _typeshed import OptExcInfo, StrOrBytesPath
from collections.abc import Callable
from types import FrameType, TracebackType
from typing import IO, Any, Final

__UNDEF__: Final[object]  # undocumented sentinel

def reset() -> str:  # undocumented
    """Return a string that resets the CGI and browser to a known state."""

def small(text: str) -> str: ...  # undocumented
def strong(text: str) -> str: ...  # undocumented
def grey(text: str) -> str: ...  # undocumented
def lookup(name: str, frame: FrameType, locals: dict[str, Any]) -> tuple[str | None, Any]:  # undocumented
    """Find the value for a given name in the given environment."""

def scanvars(
    reader: Callable[[], bytes], frame: FrameType, locals: dict[str, Any]
) -> list[tuple[str, str | None, Any]]:  # undocumented
    """Scan one logical line of Python and look up values of variables used."""

def html(einfo: OptExcInfo, context: int = 5) -> str:
    """Return a nice HTML document describing a given traceback."""

def text(einfo: OptExcInfo, context: int = 5) -> str:
    """Return a plain text document describing a given traceback."""

class Hook:  # undocumented
    """A hook to replace sys.excepthook that shows tracebacks in HTML."""

    def __init__(
        self,
        display: int = 1,
        logdir: StrOrBytesPath | None = None,
        context: int = 5,
        file: IO[str] | None = None,
        format: str = "html",
    ) -> None: ...
    def __call__(self, etype: type[BaseException] | None, evalue: BaseException | None, etb: TracebackType | None) -> None: ...
    def handle(self, info: OptExcInfo | None = None) -> None: ...

def handler(info: OptExcInfo | None = None) -> None: ...
def enable(display: int = 1, logdir: StrOrBytesPath | None = None, context: int = 5, format: str = "html") -> None:
    """Install an exception handler that formats tracebacks as HTML.

    The optional argument 'display' can be set to 0 to suppress sending the
    traceback to the browser, and 'logdir' can be set to a directory to cause
    tracebacks to be written to files there.
    """
