import _tkinter
import itertools
import sys
import tkinter
from typing import Any, ClassVar, Final, Literal, TypedDict, overload, type_check_only
from typing_extensions import TypeAlias, Unpack

__all__ = ["NORMAL", "ROMAN", "BOLD", "ITALIC", "nametofont", "Font", "families", "names"]

NORMAL: Final = "normal"
ROMAN: Final = "roman"
BOLD: Final = "bold"
ITALIC: Final = "italic"

_FontDescription: TypeAlias = (
    str  # "Helvetica 12"
    | Font  # A font object constructed in Python
    | list[Any]  # ["Helvetica", 12, BOLD]
    | tuple[str]  # ("Liberation Sans",) needs wrapping in tuple/list to handle spaces
    # ("Liberation Sans", 12) or ("Liberation Sans", 12, "bold", "italic", "underline")
    | tuple[str, int, Unpack[tuple[str, ...]]]  # Any number of trailing options is permitted
    | tuple[str, int, list[str] | tuple[str, ...]]  # Options can also be passed as list/tuple
    | _tkinter.Tcl_Obj  # A font object constructed in Tcl
)

@type_check_only
class _FontDict(TypedDict):
    family: str
    size: int
    weight: Literal["normal", "bold"]
    slant: Literal["roman", "italic"]
    underline: bool
    overstrike: bool

@type_check_only
class _MetricsDict(TypedDict):
    ascent: int
    descent: int
    linespace: int
    fixed: bool

class Font:
    """Represents a named font.

    Constructor options are:

    font -- font specifier (name, system font, or (family, size, style)-tuple)
    name -- name to use for this font configuration (defaults to a unique name)
    exists -- does a named font by this name already exist?
       Creates a new named font if False, points to the existing font if True.
       Raises _tkinter.TclError if the assertion is false.

       the following are ignored if font is specified:

    family -- font 'family', e.g. Courier, Times, Helvetica
    size -- font size in points
    weight -- font thickness: NORMAL, BOLD
    slant -- font slant: ROMAN, ITALIC
    underline -- font underlining: false (0), true (1)
    overstrike -- font strikeout: false (0), true (1)

    """

    name: str
    delete_font: bool
    counter: ClassVar[itertools.count[int]]  # undocumented
    def __init__(
        self,
        # In tkinter, 'root' refers to tkinter.Tk by convention, but the code
        # actually works with any tkinter widget so we use tkinter.Misc.
        root: tkinter.Misc | None = None,
        font: _FontDescription | None = None,
        name: str | None = None,
        exists: bool = False,
        *,
        family: str = ...,
        size: int = ...,
        weight: Literal["normal", "bold"] = ...,
        slant: Literal["roman", "italic"] = ...,
        underline: bool = ...,
        overstrike: bool = ...,
    ) -> None: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __setitem__(self, key: str, value: Any) -> None: ...
    @overload
    def cget(self, option: Literal["family"]) -> str:
        """Get font attribute"""

    @overload
    def cget(self, option: Literal["size"]) -> int: ...
    @overload
    def cget(self, option: Literal["weight"]) -> Literal["normal", "bold"]: ...
    @overload
    def cget(self, option: Literal["slant"]) -> Literal["roman", "italic"]: ...
    @overload
    def cget(self, option: Literal["underline", "overstrike"]) -> bool: ...
    @overload
    def cget(self, option: str) -> Any: ...
    __getitem__ = cget
    @overload
    def actual(self, option: Literal["family"], displayof: tkinter.Misc | None = None) -> str:
        """Return actual font attributes"""

    @overload
    def actual(self, option: Literal["size"], displayof: tkinter.Misc | None = None) -> int: ...
    @overload
    def actual(self, option: Literal["weight"], displayof: tkinter.Misc | None = None) -> Literal["normal", "bold"]: ...
    @overload
    def actual(self, option: Literal["slant"], displayof: tkinter.Misc | None = None) -> Literal["roman", "italic"]: ...
    @overload
    def actual(self, option: Literal["underline", "overstrike"], displayof: tkinter.Misc | None = None) -> bool: ...
    @overload
    def actual(self, option: None, displayof: tkinter.Misc | None = None) -> _FontDict: ...
    @overload
    def actual(self, *, displayof: tkinter.Misc | None = None) -> _FontDict: ...
    def config(
        self,
        *,
        family: str = ...,
        size: int = ...,
        weight: Literal["normal", "bold"] = ...,
        slant: Literal["roman", "italic"] = ...,
        underline: bool = ...,
        overstrike: bool = ...,
    ) -> _FontDict | None:
        """Modify font attributes"""
    configure = config
    def copy(self) -> Font:
        """Return a distinct copy of the current font"""

    @overload
    def metrics(self, option: Literal["ascent", "descent", "linespace"], /, *, displayof: tkinter.Misc | None = ...) -> int:
        """Return font metrics.

        For best performance, create a dummy widget
        using this font before calling this method.
        """

    @overload
    def metrics(self, option: Literal["fixed"], /, *, displayof: tkinter.Misc | None = ...) -> bool: ...
    @overload
    def metrics(self, *, displayof: tkinter.Misc | None = ...) -> _MetricsDict: ...
    def measure(self, text: str, displayof: tkinter.Misc | None = None) -> int:
        """Return text width"""

    def __eq__(self, other: object) -> bool: ...
    def __del__(self) -> None: ...

def families(root: tkinter.Misc | None = None, displayof: tkinter.Misc | None = None) -> tuple[str, ...]:
    """Get font families (as a tuple)"""

def names(root: tkinter.Misc | None = None) -> tuple[str, ...]:
    """Get names of defined fonts (as a tuple)"""

if sys.version_info >= (3, 10):
    def nametofont(name: str, root: tkinter.Misc | None = None) -> Font:
        """Given the name of a tk named font, returns a Font representation."""

else:
    def nametofont(name: str) -> Font:
        """Given the name of a tk named font, returns a Font representation."""
