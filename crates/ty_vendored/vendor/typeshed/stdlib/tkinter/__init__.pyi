"""Wrapper functions for Tcl/Tk.

Tkinter provides classes which allow the display, positioning and
control of widgets. Toplevel widgets are Tk and Toplevel. Other
widgets are Frame, Label, Entry, Text, Canvas, Button, Radiobutton,
Checkbutton, Scale, Listbox, Scrollbar, OptionMenu, Spinbox
LabelFrame and PanedWindow.

Properties of the widgets are specified with keyword arguments.
Keyword arguments have the same name as the corresponding resource
under Tk.

Widgets are positioned with one of the geometry managers Place, Pack
or Grid. These managers can be called with methods place, pack, grid
available in every Widget.

Actions are bound to events by resources (e.g. keyword argument
command) or with the method bind.

Example (Hello, World):
import tkinter
from tkinter.constants import *
tk = tkinter.Tk()
frame = tkinter.Frame(tk, relief=RIDGE, borderwidth=2)
frame.pack(fill=BOTH,expand=1)
label = tkinter.Label(frame, text="Hello, World")
label.pack(fill=X, expand=1)
button = tkinter.Button(frame,text="Exit",command=tk.destroy)
button.pack(side=BOTTOM)
tk.mainloop()
"""

import _tkinter
import sys
from _typeshed import Incomplete, MaybeNone, StrOrBytesPath
from collections.abc import Callable, Iterable, Mapping, Sequence
from tkinter.constants import *
from tkinter.font import _FontDescription
from types import GenericAlias, TracebackType
from typing import Any, ClassVar, Final, Generic, Literal, NamedTuple, Protocol, TypedDict, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, TypeVarTuple, Unpack, deprecated, disjoint_base

if sys.version_info >= (3, 11):
    from enum import StrEnum
else:
    from enum import Enum

__all__ = [
    "TclError",
    "NO",
    "FALSE",
    "OFF",
    "YES",
    "TRUE",
    "ON",
    "N",
    "S",
    "W",
    "E",
    "NW",
    "SW",
    "NE",
    "SE",
    "NS",
    "EW",
    "NSEW",
    "CENTER",
    "NONE",
    "X",
    "Y",
    "BOTH",
    "LEFT",
    "TOP",
    "RIGHT",
    "BOTTOM",
    "RAISED",
    "SUNKEN",
    "FLAT",
    "RIDGE",
    "GROOVE",
    "SOLID",
    "HORIZONTAL",
    "VERTICAL",
    "NUMERIC",
    "CHAR",
    "WORD",
    "BASELINE",
    "INSIDE",
    "OUTSIDE",
    "SEL",
    "SEL_FIRST",
    "SEL_LAST",
    "END",
    "INSERT",
    "CURRENT",
    "ANCHOR",
    "ALL",
    "NORMAL",
    "DISABLED",
    "ACTIVE",
    "HIDDEN",
    "CASCADE",
    "CHECKBUTTON",
    "COMMAND",
    "RADIOBUTTON",
    "SEPARATOR",
    "SINGLE",
    "BROWSE",
    "MULTIPLE",
    "EXTENDED",
    "DOTBOX",
    "UNDERLINE",
    "PIESLICE",
    "CHORD",
    "ARC",
    "FIRST",
    "LAST",
    "BUTT",
    "PROJECTING",
    "ROUND",
    "BEVEL",
    "MITER",
    "MOVETO",
    "SCROLL",
    "UNITS",
    "PAGES",
    "TkVersion",
    "TclVersion",
    "READABLE",
    "WRITABLE",
    "EXCEPTION",
    "EventType",
    "Event",
    "NoDefaultRoot",
    "Variable",
    "StringVar",
    "IntVar",
    "DoubleVar",
    "BooleanVar",
    "mainloop",
    "getint",
    "getdouble",
    "getboolean",
    "Misc",
    "CallWrapper",
    "XView",
    "YView",
    "Wm",
    "Tk",
    "Tcl",
    "Pack",
    "Place",
    "Grid",
    "BaseWidget",
    "Widget",
    "Toplevel",
    "Button",
    "Canvas",
    "Checkbutton",
    "Entry",
    "Frame",
    "Label",
    "Listbox",
    "Menu",
    "Menubutton",
    "Message",
    "Radiobutton",
    "Scale",
    "Scrollbar",
    "Text",
    "OptionMenu",
    "Image",
    "PhotoImage",
    "BitmapImage",
    "image_names",
    "image_types",
    "Spinbox",
    "LabelFrame",
    "PanedWindow",
]

# Using anything from tkinter.font in this file means that 'import tkinter'
# seems to also load tkinter.font. That's not how it actually works, but
# unfortunately not much can be done about it. https://github.com/python/typeshed/pull/4346

TclError = _tkinter.TclError
wantobjects: int
TkVersion: Final[float]
TclVersion: Final[float]
READABLE: Final = _tkinter.READABLE
WRITABLE: Final = _tkinter.WRITABLE
EXCEPTION: Final = _tkinter.EXCEPTION

# Quick guide for figuring out which widget class to choose:
#   - Misc: any widget (don't use BaseWidget because Tk doesn't inherit from BaseWidget)
#   - Widget: anything that is meant to be put into another widget with e.g. pack or grid
#
# Don't trust tkinter's docstrings, because they have been created by copy/pasting from
# Tk's manual pages more than 10 years ago. Use the latest manual pages instead:
#
#    $ sudo apt install tk-doc tcl-doc
#    $ man 3tk label        # tkinter.Label
#    $ man 3tk ttk_label    # tkinter.ttk.Label
#    $ man 3tcl after       # tkinter.Misc.after
#
# You can also read the manual pages online: https://www.tcl.tk/doc/

# manual page: Tk_GetCursor
_Cursor: TypeAlias = str | tuple[str] | tuple[str, str] | tuple[str, str, str] | tuple[str, str, str, str]

if sys.version_info >= (3, 11):
    @type_check_only
    class _VersionInfoTypeBase(NamedTuple):
        major: int
        minor: int
        micro: int
        releaselevel: str
        serial: int

    if sys.version_info >= (3, 12):
        class _VersionInfoType(_VersionInfoTypeBase): ...
    else:
        @disjoint_base
        class _VersionInfoType(_VersionInfoTypeBase): ...

if sys.version_info >= (3, 11):
    class EventType(StrEnum):
        """An enumeration."""

        Activate = "36"
        ButtonPress = "4"
        Button = ButtonPress
        ButtonRelease = "5"
        Circulate = "26"
        CirculateRequest = "27"
        ClientMessage = "33"
        Colormap = "32"
        Configure = "22"
        ConfigureRequest = "23"
        Create = "16"
        Deactivate = "37"
        Destroy = "17"
        Enter = "7"
        Expose = "12"
        FocusIn = "9"
        FocusOut = "10"
        GraphicsExpose = "13"
        Gravity = "24"
        KeyPress = "2"
        Key = "2"
        KeyRelease = "3"
        Keymap = "11"
        Leave = "8"
        Map = "19"
        MapRequest = "20"
        Mapping = "34"
        Motion = "6"
        MouseWheel = "38"
        NoExpose = "14"
        Property = "28"
        Reparent = "21"
        ResizeRequest = "25"
        Selection = "31"
        SelectionClear = "29"
        SelectionRequest = "30"
        Unmap = "18"
        VirtualEvent = "35"
        Visibility = "15"

else:
    class EventType(str, Enum):
        """An enumeration."""

        Activate = "36"
        ButtonPress = "4"
        Button = ButtonPress
        ButtonRelease = "5"
        Circulate = "26"
        CirculateRequest = "27"
        ClientMessage = "33"
        Colormap = "32"
        Configure = "22"
        ConfigureRequest = "23"
        Create = "16"
        Deactivate = "37"
        Destroy = "17"
        Enter = "7"
        Expose = "12"
        FocusIn = "9"
        FocusOut = "10"
        GraphicsExpose = "13"
        Gravity = "24"
        KeyPress = "2"
        Key = KeyPress
        KeyRelease = "3"
        Keymap = "11"
        Leave = "8"
        Map = "19"
        MapRequest = "20"
        Mapping = "34"
        Motion = "6"
        MouseWheel = "38"
        NoExpose = "14"
        Property = "28"
        Reparent = "21"
        ResizeRequest = "25"
        Selection = "31"
        SelectionClear = "29"
        SelectionRequest = "30"
        Unmap = "18"
        VirtualEvent = "35"
        Visibility = "15"

_W = TypeVar("_W", bound=Misc)
# Events considered covariant because you should never assign to event.widget.
_W_co = TypeVar("_W_co", covariant=True, bound=Misc, default=Misc)

class Event(Generic[_W_co]):
    """Container for the properties of an event.

    Instances of this type are generated if one of the following events occurs:

    KeyPress, KeyRelease - for keyboard events
    ButtonPress, ButtonRelease, Motion, Enter, Leave, MouseWheel - for mouse events
    Visibility, Unmap, Map, Expose, FocusIn, FocusOut, Circulate,
    Colormap, Gravity, Reparent, Property, Destroy, Activate,
    Deactivate - for window events.

    If a callback function for one of these events is registered
    using bind, bind_all, bind_class, or tag_bind, the callback is
    called with an Event as first argument. It will have the
    following attributes (in braces are the event types for which
    the attribute is valid):

        serial - serial number of event
    num - mouse button pressed (ButtonPress, ButtonRelease)
    focus - whether the window has the focus (Enter, Leave)
    height - height of the exposed window (Configure, Expose)
    width - width of the exposed window (Configure, Expose)
    keycode - keycode of the pressed key (KeyPress, KeyRelease)
    state - state of the event as a number (ButtonPress, ButtonRelease,
                            Enter, KeyPress, KeyRelease,
                            Leave, Motion)
    state - state as a string (Visibility)
    time - when the event occurred
    x - x-position of the mouse
    y - y-position of the mouse
    x_root - x-position of the mouse on the screen
             (ButtonPress, ButtonRelease, KeyPress, KeyRelease, Motion)
    y_root - y-position of the mouse on the screen
             (ButtonPress, ButtonRelease, KeyPress, KeyRelease, Motion)
    char - pressed character (KeyPress, KeyRelease)
    send_event - see X/Windows documentation
    keysym - keysym of the event as a string (KeyPress, KeyRelease)
    keysym_num - keysym of the event as a number (KeyPress, KeyRelease)
    type - type of the event as a number
    widget - widget in which the event occurred
    delta - delta of wheel movement (MouseWheel)
    """

    serial: int
    num: int
    focus: bool
    height: int
    width: int
    keycode: int
    state: int | str
    time: int
    x: int
    y: int
    x_root: int
    y_root: int
    char: str
    send_event: bool
    keysym: str
    keysym_num: int
    type: EventType
    widget: _W_co
    delta: int
    if sys.version_info >= (3, 14):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """Represent a PEP 585 generic type

            E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
            """

def NoDefaultRoot() -> None:
    """Inhibit setting of default root window.

    Call this function to inhibit that the first instance of
    Tk is used for windows without an explicit parent window.
    """

class Variable:
    """Class to define value holders for e.g. buttons.

    Subclasses StringVar, IntVar, DoubleVar, BooleanVar are specializations
    that constrain the type of the value returned from get().
    """

    def __init__(self, master: Misc | None = None, value=None, name: str | None = None) -> None:
        """Construct a variable

        MASTER can be given as master widget.
        VALUE is an optional value (defaults to "")
        NAME is an optional Tcl name (defaults to PY_VARnum).

        If NAME matches an existing variable and VALUE is omitted
        then the existing value is retained.
        """

    def set(self, value) -> None:
        """Set the variable to VALUE."""
    initialize = set
    def get(self):
        """Return value of variable."""

    def trace_add(self, mode: Literal["array", "read", "write", "unset"], callback: Callable[[str, str, str], object]) -> str:
        """Define a trace callback for the variable.

        Mode is one of "read", "write", "unset", or a list or tuple of
        such strings.
        Callback must be a function which is called when the variable is
        read, written or unset.

        Return the name of the callback.
        """

    def trace_remove(self, mode: Literal["array", "read", "write", "unset"], cbname: str) -> None:
        """Delete the trace callback for a variable.

        Mode is one of "read", "write", "unset" or a list or tuple of
        such strings.  Must be same as were specified in trace_add().
        cbname is the name of the callback returned from trace_add().
        """

    def trace_info(self) -> list[tuple[tuple[Literal["array", "read", "write", "unset"], ...], str]]:
        """Return all trace callback information."""
    if sys.version_info >= (3, 14):
        @deprecated("Deprecated since Python 3.14. Use `trace_add()` instead.")
        def trace(self, mode, callback) -> str:
            """Define a trace callback for the variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CALLBACK must be a function which is called when
            the variable is read, written or undefined.

            Return the name of the callback.

            This deprecated method wraps a deprecated Tcl method removed
            in Tcl 9.0.  Use trace_add() instead.
            """

        @deprecated("Deprecated since Python 3.14. Use `trace_add()` instead.")
        def trace_variable(self, mode, callback) -> str:
            """Define a trace callback for the variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CALLBACK must be a function which is called when
            the variable is read, written or undefined.

            Return the name of the callback.

            This deprecated method wraps a deprecated Tcl method removed
            in Tcl 9.0.  Use trace_add() instead.
            """

        @deprecated("Deprecated since Python 3.14. Use `trace_remove()` instead.")
        def trace_vdelete(self, mode, cbname) -> None:
            """Delete the trace callback for a variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CBNAME is the name of the callback returned from trace_variable or trace.

            This deprecated method wraps a deprecated Tcl method removed
            in Tcl 9.0.  Use trace_remove() instead.
            """

        @deprecated("Deprecated since Python 3.14. Use `trace_info()` instead.")
        def trace_vinfo(self) -> list[Incomplete]:
            """Return all trace callback information.

            This deprecated method wraps a deprecated Tcl method removed
            in Tcl 9.0.  Use trace_info() instead.
            """
    else:
        def trace(self, mode, callback) -> str:
            """Define a trace callback for the variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CALLBACK must be a function which is called when
            the variable is read, written or undefined.

            Return the name of the callback.

            This deprecated method wraps a deprecated Tcl method that will
            likely be removed in the future.  Use trace_add() instead.
            """

        def trace_variable(self, mode, callback) -> str:
            """Define a trace callback for the variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CALLBACK must be a function which is called when
            the variable is read, written or undefined.

            Return the name of the callback.

            This deprecated method wraps a deprecated Tcl method that will
            likely be removed in the future.  Use trace_add() instead.
            """

        def trace_vdelete(self, mode, cbname) -> None:
            """Delete the trace callback for a variable.

            MODE is one of "r", "w", "u" for read, write, undefine.
            CBNAME is the name of the callback returned from trace_variable or trace.

            This deprecated method wraps a deprecated Tcl method that will
            likely be removed in the future.  Use trace_remove() instead.
            """

        def trace_vinfo(self) -> list[Incomplete]:
            """Return all trace callback information.

            This deprecated method wraps a deprecated Tcl method that will
            likely be removed in the future.  Use trace_info() instead.
            """

    def __eq__(self, other: object) -> bool: ...
    def __del__(self) -> None:
        """Unset the variable in Tcl."""
    __hash__: ClassVar[None]  # type: ignore[assignment]

class StringVar(Variable):
    """Value holder for strings variables."""

    def __init__(self, master: Misc | None = None, value: str | None = None, name: str | None = None) -> None:
        """Construct a string variable.

        MASTER can be given as master widget.
        VALUE is an optional value (defaults to "")
        NAME is an optional Tcl name (defaults to PY_VARnum).

        If NAME matches an existing variable and VALUE is omitted
        then the existing value is retained.
        """

    def set(self, value: str) -> None:
        """Set the variable to VALUE."""
    initialize = set
    def get(self) -> str:
        """Return value of variable as string."""

class IntVar(Variable):
    """Value holder for integer variables."""

    def __init__(self, master: Misc | None = None, value: int | None = None, name: str | None = None) -> None:
        """Construct an integer variable.

        MASTER can be given as master widget.
        VALUE is an optional value (defaults to 0)
        NAME is an optional Tcl name (defaults to PY_VARnum).

        If NAME matches an existing variable and VALUE is omitted
        then the existing value is retained.
        """

    def set(self, value: int) -> None:
        """Set the variable to VALUE."""
    initialize = set
    def get(self) -> int:
        """Return the value of the variable as an integer."""

class DoubleVar(Variable):
    """Value holder for float variables."""

    def __init__(self, master: Misc | None = None, value: float | None = None, name: str | None = None) -> None:
        """Construct a float variable.

        MASTER can be given as master widget.
        VALUE is an optional value (defaults to 0.0)
        NAME is an optional Tcl name (defaults to PY_VARnum).

        If NAME matches an existing variable and VALUE is omitted
        then the existing value is retained.
        """

    def set(self, value: float) -> None:
        """Set the variable to VALUE."""
    initialize = set
    def get(self) -> float:
        """Return the value of the variable as a float."""

class BooleanVar(Variable):
    """Value holder for boolean variables."""

    def __init__(self, master: Misc | None = None, value: bool | None = None, name: str | None = None) -> None:
        """Construct a boolean variable.

        MASTER can be given as master widget.
        VALUE is an optional value (defaults to False)
        NAME is an optional Tcl name (defaults to PY_VARnum).

        If NAME matches an existing variable and VALUE is omitted
        then the existing value is retained.
        """

    def set(self, value: bool) -> None:
        """Set the variable to VALUE."""
    initialize = set
    def get(self) -> bool:
        """Return the value of the variable as a bool."""

def mainloop(n: int = 0) -> None:
    """Run the main loop of Tcl."""

getint = int
getdouble = float

def getboolean(s) -> bool:
    """Convert Tcl object to True or False."""

_Ts = TypeVarTuple("_Ts")

@type_check_only
class _GridIndexInfo(TypedDict, total=False):
    minsize: float | str
    pad: float | str
    uniform: str | None
    weight: int

@type_check_only
class _BusyInfo(TypedDict):
    cursor: _Cursor

class Misc:
    """Internal class.

    Base class which defines methods common for interior widgets.
    """

    master: Misc | None
    tk: _tkinter.TkappType
    children: dict[str, Widget]
    def destroy(self) -> None:
        """Internal function.

        Delete all Tcl commands created for
        this widget in the Tcl interpreter.
        """

    def deletecommand(self, name: str) -> None:
        """Internal function.

        Delete the Tcl command provided in NAME.
        """

    def tk_strictMotif(self, boolean=None):
        """Set Tcl internal variable, whether the look and feel
        should adhere to Motif.

        A parameter of 1 means adhere to Motif (e.g. no color
        change if mouse passes over slider).
        Returns the set value.
        """

    def tk_bisque(self) -> None:
        """Change the color scheme to light brown as used in Tk 3.6 and before."""

    def tk_setPalette(self, *args, **kw) -> None:
        """Set a new color scheme for all widget elements.

        A single color as argument will cause that all colors of Tk
        widget elements are derived from this.
        Alternatively several keyword parameters and its associated
        colors can be given. The following keywords are valid:
        activeBackground, foreground, selectColor,
        activeForeground, highlightBackground, selectBackground,
        background, highlightColor, selectForeground,
        disabledForeground, insertBackground, troughColor.
        """

    def wait_variable(self, name: str | Variable = "PY_VAR") -> None:
        """Wait until the variable is modified.

        A parameter of type IntVar, StringVar, DoubleVar or
        BooleanVar must be given.
        """
    waitvar = wait_variable
    def wait_window(self, window: Misc | None = None) -> None:
        """Wait until a WIDGET is destroyed.

        If no parameter is given self is used.
        """

    def wait_visibility(self, window: Misc | None = None) -> None:
        """Wait until the visibility of a WIDGET changes
        (e.g. it appears).

        If no parameter is given self is used.
        """

    def setvar(self, name: str = "PY_VAR", value: str = "1") -> None:
        """Set Tcl variable NAME to VALUE."""

    def getvar(self, name: str = "PY_VAR"):
        """Return value of Tcl variable NAME."""

    def getint(self, s) -> int: ...
    def getdouble(self, s) -> float: ...
    def getboolean(self, s) -> bool:
        """Return a boolean value for Tcl boolean values true and false given as parameter."""

    def focus_set(self) -> None:
        """Direct input focus to this widget.

        If the application currently does not have the focus
        this widget will get the focus if the application gets
        the focus through the window manager.
        """
    focus = focus_set
    def focus_force(self) -> None:
        """Direct input focus to this widget even if the
        application does not have the focus. Use with
        caution!
        """

    def focus_get(self) -> Misc | None:
        """Return the widget which has currently the focus in the
        application.

        Use focus_displayof to allow working with several
        displays. Return None if application does not have
        the focus.
        """

    def focus_displayof(self) -> Misc | None:
        """Return the widget which has currently the focus on the
        display where this widget is located.

        Return None if the application does not have the focus.
        """

    def focus_lastfor(self) -> Misc | None:
        """Return the widget which would have the focus if top level
        for this widget gets the focus from the window manager.
        """

    def tk_focusFollowsMouse(self) -> None:
        """The widget under mouse will get automatically focus. Can not
        be disabled easily.
        """

    def tk_focusNext(self) -> Misc | None:
        """Return the next widget in the focus order which follows
        widget which has currently the focus.

        The focus order first goes to the next child, then to
        the children of the child recursively and then to the
        next sibling which is higher in the stacking order.  A
        widget is omitted if it has the takefocus resource set
        to 0.
        """

    def tk_focusPrev(self) -> Misc | None:
        """Return previous widget in the focus order. See tk_focusNext for details."""
    # .after() can be called without the "func" argument, but it is basically never what you want.
    # It behaves like time.sleep() and freezes the GUI app.
    def after(self, ms: int | Literal["idle"], func: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts]) -> str:
        """Call function once after given time.

        MS specifies the time in milliseconds. FUNC gives the
        function which shall be called. Additional parameters
        are given as parameters to the function call.  Return
        identifier to cancel scheduling with after_cancel.
        """
    # after_idle is essentially partialmethod(after, "idle")
    def after_idle(self, func: Callable[[Unpack[_Ts]], object], *args: Unpack[_Ts]) -> str:
        """Call FUNC once if the Tcl main loop has no event to
        process.

        Return an identifier to cancel the scheduling with
        after_cancel.
        """

    def after_cancel(self, id: str) -> None:
        """Cancel scheduling of function identified with ID.

        Identifier returned by after or after_idle must be
        given as first parameter.
        """
    if sys.version_info >= (3, 13):
        def after_info(self, id: str | None = None) -> tuple[str, ...]:
            """Return information about existing event handlers.

            With no argument, return a tuple of the identifiers for all existing
            event handlers created by the after and after_idle commands for this
            interpreter.  If id is supplied, it specifies an existing handler; id
            must have been the return value from some previous call to after or
            after_idle and it must not have triggered yet or been canceled. If the
            id doesn't exist, a TclError is raised.  Otherwise, the return value is
            a tuple containing (script, type) where script is a reference to the
            function to be called by the event handler and type is either 'idle'
            or 'timer' to indicate what kind of event handler it is.
            """

    def bell(self, displayof: Literal[0] | Misc | None = 0) -> None:
        """Ring a display's bell."""
    if sys.version_info >= (3, 13):
        # Supports options from `_BusyInfo``
        def tk_busy_cget(self, option: Literal["cursor"]) -> _Cursor:
            """Return the value of busy configuration option.

            The widget must have been previously made busy by
            tk_busy_hold().  Option may have any of the values accepted by
            tk_busy_hold().
            """
        busy_cget = tk_busy_cget
        def tk_busy_configure(self, cnf: Any = None, **kw: Any) -> Any:
            """Query or modify the busy configuration options.

            The widget must have been previously made busy by
            tk_busy_hold().  Options may have any of the values accepted by
            tk_busy_hold().

            Please note that the option database is referenced by the widget
            name or class.  For example, if a Frame widget with name "frame"
            is to be made busy, the busy cursor can be specified for it by
            either call:

                w.option_add('*frame.busyCursor', 'gumby')
                w.option_add('*Frame.BusyCursor', 'gumby')
            """
        tk_busy_config = tk_busy_configure
        busy_configure = tk_busy_configure
        busy_config = tk_busy_configure
        def tk_busy_current(self, pattern: str | None = None) -> list[Misc]:
            """Return a list of widgets that are currently busy.

            If a pattern is given, only busy widgets whose path names match
            a pattern are returned.
            """
        busy_current = tk_busy_current
        def tk_busy_forget(self) -> None:
            """Make this widget no longer busy.

            User events will again be received by the widget.
            """
        busy_forget = tk_busy_forget
        def tk_busy_hold(self, **kw: Unpack[_BusyInfo]) -> None:
            """Make this widget appear busy.

            The specified widget and its descendants will be blocked from
            user interactions.  Normally update() should be called
            immediately afterward to insure that the hold operation is in
            effect before the application starts its processing.

            The only supported configuration option is:

                cursor: the cursor to be displayed when the widget is made
                        busy.
            """
        tk_busy = tk_busy_hold
        busy_hold = tk_busy_hold
        busy = tk_busy_hold
        def tk_busy_status(self) -> bool:
            """Return True if the widget is busy, False otherwise."""
        busy_status = tk_busy_status

    def clipboard_get(self, *, displayof: Misc = ..., type: str = ...) -> str:
        """Retrieve data from the clipboard on window's display.

        The window keyword defaults to the root window of the Tkinter
        application.

        The type keyword specifies the form in which the data is
        to be returned and should be an atom name such as STRING
        or FILE_NAME.  Type defaults to STRING, except on X11, where the default
        is to try UTF8_STRING and fall back to STRING.

        This command is equivalent to:

        selection_get(CLIPBOARD)
        """

    def clipboard_clear(self, *, displayof: Misc = ...) -> None:
        """Clear the data in the Tk clipboard.

        A widget specified for the optional displayof keyword
        argument specifies the target display.
        """

    def clipboard_append(self, string: str, *, displayof: Misc = ..., format: str = ..., type: str = ...) -> None:
        """Append STRING to the Tk clipboard.

        A widget specified at the optional displayof keyword
        argument specifies the target display. The clipboard
        can be retrieved with selection_get.
        """

    def grab_current(self):
        """Return widget which has currently the grab in this application
        or None.
        """

    def grab_release(self) -> None:
        """Release grab for this widget if currently set."""

    def grab_set(self) -> None:
        """Set grab for this widget.

        A grab directs all events to this and descendant
        widgets in the application.
        """

    def grab_set_global(self) -> None:
        """Set global grab for this widget.

        A global grab directs all events to this and
        descendant widgets on the display. Use with caution -
        other applications do not get events anymore.
        """

    def grab_status(self) -> Literal["local", "global"] | None:
        """Return None, "local" or "global" if this widget has
        no, a local or a global grab.
        """

    def option_add(
        self, pattern, value, priority: int | Literal["widgetDefault", "startupFile", "userDefault", "interactive"] | None = None
    ) -> None:
        """Set a VALUE (second parameter) for an option
        PATTERN (first parameter).

        An optional third parameter gives the numeric priority
        (defaults to 80).
        """

    def option_clear(self) -> None:
        """Clear the option database.

        It will be reloaded if option_add is called.
        """

    def option_get(self, name, className):
        """Return the value for an option NAME for this widget
        with CLASSNAME.

        Values with higher priority override lower values.
        """

    def option_readfile(self, fileName, priority=None) -> None:
        """Read file FILENAME into the option database.

        An optional second parameter gives the numeric
        priority.
        """

    def selection_clear(self, **kw) -> None:
        """Clear the current X selection."""

    def selection_get(self, **kw):
        """Return the contents of the current X selection.

        A keyword parameter selection specifies the name of
        the selection and defaults to PRIMARY.  A keyword
        parameter displayof specifies a widget on the display
        to use. A keyword parameter type specifies the form of data to be
        fetched, defaulting to STRING except on X11, where UTF8_STRING is tried
        before STRING.
        """

    def selection_handle(self, command, **kw) -> None:
        """Specify a function COMMAND to call if the X
        selection owned by this widget is queried by another
        application.

        This function must return the contents of the
        selection. The function will be called with the
        arguments OFFSET and LENGTH which allows the chunking
        of very long selections. The following keyword
        parameters can be provided:
        selection - name of the selection (default PRIMARY),
        type - type of the selection (e.g. STRING, FILE_NAME).
        """

    def selection_own(self, **kw) -> None:
        """Become owner of X selection.

        A keyword parameter selection specifies the name of
        the selection (default PRIMARY).
        """

    def selection_own_get(self, **kw):
        """Return owner of X selection.

        The following keyword parameter can
        be provided:
        selection - name of the selection (default PRIMARY),
        type - type of the selection (e.g. STRING, FILE_NAME).
        """

    def send(self, interp, cmd, *args):
        """Send Tcl command CMD to different interpreter INTERP to be executed."""

    def lower(self, belowThis=None) -> None:
        """Lower this widget in the stacking order."""

    def tkraise(self, aboveThis=None) -> None:
        """Raise this widget in the stacking order."""
    lift = tkraise
    if sys.version_info >= (3, 11):
        def info_patchlevel(self) -> _VersionInfoType:
            """Returns the exact version of the Tcl library."""

    def winfo_atom(self, name: str, displayof: Literal[0] | Misc | None = 0) -> int:
        """Return integer which represents atom NAME."""

    def winfo_atomname(self, id: int, displayof: Literal[0] | Misc | None = 0) -> str:
        """Return name of atom with identifier ID."""

    def winfo_cells(self) -> int:
        """Return number of cells in the colormap for this widget."""

    def winfo_children(self) -> list[Widget | Toplevel]:
        """Return a list of all widgets which are children of this widget."""

    def winfo_class(self) -> str:
        """Return window class name of this widget."""

    def winfo_colormapfull(self) -> bool:
        """Return True if at the last color request the colormap was full."""

    def winfo_containing(self, rootX: int, rootY: int, displayof: Literal[0] | Misc | None = 0) -> Misc | None:
        """Return the widget which is at the root coordinates ROOTX, ROOTY."""

    def winfo_depth(self) -> int:
        """Return the number of bits per pixel."""

    def winfo_exists(self) -> bool:
        """Return true if this widget exists."""

    def winfo_fpixels(self, number: float | str) -> float:
        """Return the number of pixels for the given distance NUMBER
        (e.g. "3c") as float.
        """

    def winfo_geometry(self) -> str:
        """Return geometry string for this widget in the form "widthxheight+X+Y"."""

    def winfo_height(self) -> int:
        """Return height of this widget."""

    def winfo_id(self) -> int:
        """Return identifier ID for this widget."""

    def winfo_interps(self, displayof: Literal[0] | Misc | None = 0) -> tuple[str, ...]:
        """Return the name of all Tcl interpreters for this display."""

    def winfo_ismapped(self) -> bool:
        """Return true if this widget is mapped."""

    def winfo_manager(self) -> str:
        """Return the window manager name for this widget."""

    def winfo_name(self) -> str:
        """Return the name of this widget."""

    def winfo_parent(self) -> str:  # return value needs nametowidget()
        """Return the name of the parent of this widget."""

    def winfo_pathname(self, id: int, displayof: Literal[0] | Misc | None = 0):
        """Return the pathname of the widget given by ID."""

    def winfo_pixels(self, number: float | str) -> int:
        """Rounded integer value of winfo_fpixels."""

    def winfo_pointerx(self) -> int:
        """Return the x coordinate of the pointer on the root window."""

    def winfo_pointerxy(self) -> tuple[int, int]:
        """Return a tuple of x and y coordinates of the pointer on the root window."""

    def winfo_pointery(self) -> int:
        """Return the y coordinate of the pointer on the root window."""

    def winfo_reqheight(self) -> int:
        """Return requested height of this widget."""

    def winfo_reqwidth(self) -> int:
        """Return requested width of this widget."""

    def winfo_rgb(self, color: str) -> tuple[int, int, int]:
        """Return a tuple of integer RGB values in range(65536) for color in this widget."""

    def winfo_rootx(self) -> int:
        """Return x coordinate of upper left corner of this widget on the
        root window.
        """

    def winfo_rooty(self) -> int:
        """Return y coordinate of upper left corner of this widget on the
        root window.
        """

    def winfo_screen(self) -> str:
        """Return the screen name of this widget."""

    def winfo_screencells(self) -> int:
        """Return the number of the cells in the colormap of the screen
        of this widget.
        """

    def winfo_screendepth(self) -> int:
        """Return the number of bits per pixel of the root window of the
        screen of this widget.
        """

    def winfo_screenheight(self) -> int:
        """Return the number of pixels of the height of the screen of this widget
        in pixel.
        """

    def winfo_screenmmheight(self) -> int:
        """Return the number of pixels of the height of the screen of
        this widget in mm.
        """

    def winfo_screenmmwidth(self) -> int:
        """Return the number of pixels of the width of the screen of
        this widget in mm.
        """

    def winfo_screenvisual(self) -> str:
        """Return one of the strings directcolor, grayscale, pseudocolor,
        staticcolor, staticgray, or truecolor for the default
        colormodel of this screen.
        """

    def winfo_screenwidth(self) -> int:
        """Return the number of pixels of the width of the screen of
        this widget in pixel.
        """

    def winfo_server(self) -> str:
        """Return information of the X-Server of the screen of this widget in
        the form "XmajorRminor vendor vendorVersion".
        """

    def winfo_toplevel(self) -> Tk | Toplevel:
        """Return the toplevel widget of this widget."""

    def winfo_viewable(self) -> bool:
        """Return true if the widget and all its higher ancestors are mapped."""

    def winfo_visual(self) -> str:
        """Return one of the strings directcolor, grayscale, pseudocolor,
        staticcolor, staticgray, or truecolor for the
        colormodel of this widget.
        """

    def winfo_visualid(self) -> str:
        """Return the X identifier for the visual for this widget."""

    def winfo_visualsavailable(self, includeids: bool = False) -> list[tuple[str, int]]:
        """Return a list of all visuals available for the screen
        of this widget.

        Each item in the list consists of a visual name (see winfo_visual), a
        depth and if includeids is true is given also the X identifier.
        """

    def winfo_vrootheight(self) -> int:
        """Return the height of the virtual root window associated with this
        widget in pixels. If there is no virtual root window return the
        height of the screen.
        """

    def winfo_vrootwidth(self) -> int:
        """Return the width of the virtual root window associated with this
        widget in pixel. If there is no virtual root window return the
        width of the screen.
        """

    def winfo_vrootx(self) -> int:
        """Return the x offset of the virtual root relative to the root
        window of the screen of this widget.
        """

    def winfo_vrooty(self) -> int:
        """Return the y offset of the virtual root relative to the root
        window of the screen of this widget.
        """

    def winfo_width(self) -> int:
        """Return the width of this widget."""

    def winfo_x(self) -> int:
        """Return the x coordinate of the upper left corner of this widget
        in the parent.
        """

    def winfo_y(self) -> int:
        """Return the y coordinate of the upper left corner of this widget
        in the parent.
        """

    def update(self) -> None:
        """Enter event loop until all pending events have been processed by Tcl."""

    def update_idletasks(self) -> None:
        """Enter event loop until all idle callbacks have been called. This
        will update the display of windows but not process events caused by
        the user.
        """

    @overload
    def bindtags(self, tagList: None = None) -> tuple[str, ...]:
        """Set or get the list of bindtags for this widget.

        With no argument return the list of all bindtags associated with
        this widget. With a list of strings as argument the bindtags are
        set to this list. The bindtags determine in which order events are
        processed (see bind).
        """

    @overload
    def bindtags(self, tagList: list[str] | tuple[str, ...]) -> None: ...
    # bind with isinstance(func, str) doesn't return anything, but all other
    # binds do. The default value of func is not str.
    @overload
    def bind(
        self,
        sequence: str | None = None,
        func: Callable[[Event[Misc]], object] | None = None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to this widget at event SEQUENCE a call to function FUNC.

        SEQUENCE is a string of concatenated event
        patterns. An event pattern is of the form
        <MODIFIER-MODIFIER-TYPE-DETAIL> where MODIFIER is one
        of Control, Mod2, M2, Shift, Mod3, M3, Lock, Mod4, M4,
        Button1, B1, Mod5, M5 Button2, B2, Meta, M, Button3,
        B3, Alt, Button4, B4, Double, Button5, B5 Triple,
        Mod1, M1. TYPE is one of Activate, Enter, Map,
        ButtonPress, Button, Expose, Motion, ButtonRelease
        FocusIn, MouseWheel, Circulate, FocusOut, Property,
        Colormap, Gravity Reparent, Configure, KeyPress, Key,
        Unmap, Deactivate, KeyRelease Visibility, Destroy,
        Leave and DETAIL is the button number for ButtonPress,
        ButtonRelease and DETAIL is the Keysym for KeyPress and
        KeyRelease. Examples are
        <Control-Button-1> for pressing Control and mouse button 1 or
        <Alt-A> for pressing A and the Alt key (KeyPress can be omitted).
        An event pattern can also be a virtual event of the form
        <<AString>> where AString can be arbitrary. This
        event can be generated by event_generate.
        If events are concatenated they must appear shortly
        after each other.

        FUNC will be called if the event sequence occurs with an
        instance of Event as argument. If the return value of FUNC is
        "break" no further bound function is invoked.

        An additional boolean parameter ADD specifies whether FUNC will
        be called additionally to the other bound function or whether
        it will replace the previous function.

        Bind will return an identifier to allow deletion of the bound function with
        unbind without memory leak.

        If FUNC or SEQUENCE is omitted the bound function or list
        of bound events are returned.
        """

    @overload
    def bind(self, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    @overload
    def bind(self, *, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    # There's no way to know what type of widget bind_all and bind_class
    # callbacks will get, so those are Misc.
    @overload
    def bind_all(
        self,
        sequence: str | None = None,
        func: Callable[[Event[Misc]], object] | None = None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to all widgets at an event SEQUENCE a call to function FUNC.
        An additional boolean parameter ADD specifies whether FUNC will
        be called additionally to the other bound function or whether
        it will replace the previous function. See bind for the return value.
        """

    @overload
    def bind_all(self, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    @overload
    def bind_all(self, *, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    @overload
    def bind_class(
        self,
        className: str,
        sequence: str | None = None,
        func: Callable[[Event[Misc]], object] | None = None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to widgets with bindtag CLASSNAME at event
        SEQUENCE a call of function FUNC. An additional
        boolean parameter ADD specifies whether FUNC will be
        called additionally to the other bound function or
        whether it will replace the previous function. See bind for
        the return value.
        """

    @overload
    def bind_class(self, className: str, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    @overload
    def bind_class(self, className: str, *, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    def unbind(self, sequence: str, funcid: str | None = None) -> None:
        """Unbind for this widget the event SEQUENCE.

        If FUNCID is given, only unbind the function identified with FUNCID
        and also delete the corresponding Tcl command.

        Otherwise destroy the current binding for SEQUENCE, leaving SEQUENCE
        unbound.
        """

    def unbind_all(self, sequence: str) -> None:
        """Unbind for all widgets for event SEQUENCE all functions."""

    def unbind_class(self, className: str, sequence: str) -> None:
        """Unbind for all widgets with bindtag CLASSNAME for event SEQUENCE
        all functions.
        """

    def mainloop(self, n: int = 0) -> None:
        """Call the mainloop of Tk."""

    def quit(self) -> None:
        """Quit the Tcl interpreter. All widgets will be destroyed."""

    @property
    def _windowingsystem(self) -> Literal["win32", "aqua", "x11"]:
        """Internal function."""

    def nametowidget(self, name: str | Misc | _tkinter.Tcl_Obj) -> Any:
        """Return the Tkinter instance of a widget identified by
        its Tcl name NAME.
        """

    def register(
        self, func: Callable[..., object], subst: Callable[..., Sequence[Any]] | None = None, needcleanup: int = 1
    ) -> str:
        """Return a newly created Tcl function. If this
        function is called, the Python function FUNC will
        be executed. An optional function SUBST can
        be given which will be executed before FUNC.
        """

    def keys(self) -> list[str]:
        """Return a list of all resource names of this widget."""

    @overload
    def pack_propagate(self, flag: bool) -> bool | None:
        """Set or get the status for propagation of geometry information.

        A boolean argument specifies whether the geometry information
        of the slaves will determine the size of this widget. If no argument
        is given the current setting will be returned.
        """

    @overload
    def pack_propagate(self) -> None: ...
    propagate = pack_propagate
    def grid_anchor(self, anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] | None = None) -> None:
        """The anchor value controls how to place the grid within the
        master when no row/column has any weight.

        The default anchor is nw.
        """
    anchor = grid_anchor
    @overload
    def grid_bbox(
        self, column: None = None, row: None = None, col2: None = None, row2: None = None
    ) -> tuple[int, int, int, int] | None:
        """Return a tuple of integer coordinates for the bounding
        box of this widget controlled by the geometry manager grid.

        If COLUMN, ROW is given the bounding box applies from
        the cell with row and column 0 to the specified
        cell. If COL2 and ROW2 are given the bounding box
        starts at that cell.

        The returned integers specify the offset of the upper left
        corner in the master widget and the width and height.
        """

    @overload
    def grid_bbox(self, column: int, row: int, col2: None = None, row2: None = None) -> tuple[int, int, int, int] | None: ...
    @overload
    def grid_bbox(self, column: int, row: int, col2: int, row2: int) -> tuple[int, int, int, int] | None: ...
    bbox = grid_bbox
    def grid_columnconfigure(
        self,
        index: int | str | list[int] | tuple[int, ...],
        cnf: _GridIndexInfo = {},
        *,
        minsize: float | str = ...,
        pad: float | str = ...,
        uniform: str = ...,
        weight: int = ...,
    ) -> _GridIndexInfo | MaybeNone:  # can be None but annoying to check
        """Configure column INDEX of a grid.

        Valid resources are minsize (minimum size of the column),
        weight (how much does additional space propagate to this column)
        and pad (how much space to let additionally).
        """

    def grid_rowconfigure(
        self,
        index: int | str | list[int] | tuple[int, ...],
        cnf: _GridIndexInfo = {},
        *,
        minsize: float | str = ...,
        pad: float | str = ...,
        uniform: str = ...,
        weight: int = ...,
    ) -> _GridIndexInfo | MaybeNone:  # can be None but annoying to check
        """Configure row INDEX of a grid.

        Valid resources are minsize (minimum size of the row),
        weight (how much does additional space propagate to this row)
        and pad (how much space to let additionally).
        """
    columnconfigure = grid_columnconfigure
    rowconfigure = grid_rowconfigure
    def grid_location(self, x: float | str, y: float | str) -> tuple[int, int]:
        """Return a tuple of column and row which identify the cell
        at which the pixel at position X and Y inside the master
        widget is located.
        """

    @overload
    def grid_propagate(self, flag: bool) -> None:
        """Set or get the status for propagation of geometry information.

        A boolean argument specifies whether the geometry information
        of the slaves will determine the size of this widget. If no argument
        is given, the current setting will be returned.
        """

    @overload
    def grid_propagate(self) -> bool: ...
    def grid_size(self) -> tuple[int, int]:
        """Return a tuple of the number of column and rows in the grid."""
    size = grid_size
    # Widget because Toplevel or Tk is never a slave
    def pack_slaves(self) -> list[Widget]:
        """Return a list of all slaves of this widget
        in its packing order.
        """

    def grid_slaves(self, row: int | None = None, column: int | None = None) -> list[Widget]:
        """Return a list of all slaves of this widget
        in its packing order.
        """

    def place_slaves(self) -> list[Widget]:
        """Return a list of all slaves of this widget
        in its packing order.
        """
    slaves = pack_slaves
    def event_add(self, virtual: str, *sequences: str) -> None:
        """Bind a virtual event VIRTUAL (of the form <<Name>>)
        to an event SEQUENCE such that the virtual event is triggered
        whenever SEQUENCE occurs.
        """

    def event_delete(self, virtual: str, *sequences: str) -> None:
        """Unbind a virtual event VIRTUAL from SEQUENCE."""

    def event_generate(
        self,
        sequence: str,
        *,
        above: Misc | int = ...,
        borderwidth: float | str = ...,
        button: int = ...,
        count: int = ...,
        data: Any = ...,  # anything with usable str() value
        delta: int = ...,
        detail: str = ...,
        focus: bool = ...,
        height: float | str = ...,
        keycode: int = ...,
        keysym: str = ...,
        mode: str = ...,
        override: bool = ...,
        place: Literal["PlaceOnTop", "PlaceOnBottom"] = ...,
        root: Misc | int = ...,
        rootx: float | str = ...,
        rooty: float | str = ...,
        sendevent: bool = ...,
        serial: int = ...,
        state: int | str = ...,
        subwindow: Misc | int = ...,
        time: int = ...,
        warp: bool = ...,
        width: float | str = ...,
        when: Literal["now", "tail", "head", "mark"] = ...,
        x: float | str = ...,
        y: float | str = ...,
    ) -> None:
        """Generate an event SEQUENCE. Additional
        keyword arguments specify parameter of the event
        (e.g. x, y, rootx, rooty).
        """

    def event_info(self, virtual: str | None = None) -> tuple[str, ...]:
        """Return a list of all virtual events or the information
        about the SEQUENCE bound to the virtual event VIRTUAL.
        """

    def image_names(self) -> tuple[str, ...]:
        """Return a list of all existing image names."""

    def image_types(self) -> tuple[str, ...]:
        """Return a list of all available image types (e.g. photo bitmap)."""
    # See #4363 and #4891
    def __setitem__(self, key: str, value: Any) -> None: ...
    def __getitem__(self, key: str) -> Any:
        """Return the resource value for a KEY given as string."""

    def cget(self, key: str) -> Any:
        """Return the resource value for a KEY given as string."""

    def configure(self, cnf: Any = None) -> Any:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """
    # TODO: config is an alias of configure, but adding that here creates
    # conflict with the type of config in the subclasses. See #13149

class CallWrapper:
    """Internal class. Stores function to call when some user
    defined Tcl function is called e.g. after an event occurred.
    """

    func: Incomplete
    subst: Incomplete
    widget: Incomplete
    def __init__(self, func, subst, widget) -> None:
        """Store FUNC, SUBST and WIDGET as members."""

    def __call__(self, *args):
        """Apply first function SUBST to arguments, than FUNC."""

class XView:
    """Mix-in class for querying and changing the horizontal position
    of a widget's window.
    """

    @overload
    def xview(self) -> tuple[float, float]:
        """Query and change the horizontal position of the view."""

    @overload
    def xview(self, *args) -> None: ...
    def xview_moveto(self, fraction: float) -> None:
        """Adjusts the view in the window so that FRACTION of the
        total width of the canvas is off-screen to the left.
        """

    @overload
    def xview_scroll(self, number: int, what: Literal["units", "pages"]) -> None:
        """Shift the x-view according to NUMBER which is measured in "units"
        or "pages" (WHAT).
        """

    @overload
    def xview_scroll(self, number: float | str, what: Literal["pixels"]) -> None: ...

class YView:
    """Mix-in class for querying and changing the vertical position
    of a widget's window.
    """

    @overload
    def yview(self) -> tuple[float, float]:
        """Query and change the vertical position of the view."""

    @overload
    def yview(self, *args) -> None: ...
    def yview_moveto(self, fraction: float) -> None:
        """Adjusts the view in the window so that FRACTION of the
        total height of the canvas is off-screen to the top.
        """

    @overload
    def yview_scroll(self, number: int, what: Literal["units", "pages"]) -> None:
        """Shift the y-view according to NUMBER which is measured in
        "units" or "pages" (WHAT).
        """

    @overload
    def yview_scroll(self, number: float | str, what: Literal["pixels"]) -> None: ...

if sys.platform == "darwin":
    @type_check_only
    class _WmAttributes(TypedDict):
        alpha: float
        fullscreen: bool
        modified: bool
        notify: bool
        titlepath: str
        topmost: bool
        transparent: bool
        type: str  # Present, but not actually used on darwin

elif sys.platform == "win32":
    @type_check_only
    class _WmAttributes(TypedDict):
        alpha: float
        transparentcolor: str
        disabled: bool
        fullscreen: bool
        toolwindow: bool
        topmost: bool

else:
    # X11
    @type_check_only
    class _WmAttributes(TypedDict):
        alpha: float
        topmost: bool
        zoomed: bool
        fullscreen: bool
        type: str

class Wm:
    """Provides functions for the communication with the window manager."""

    @overload
    def wm_aspect(self, minNumer: int, minDenom: int, maxNumer: int, maxDenom: int) -> None:
        """Instruct the window manager to set the aspect ratio (width/height)
        of this widget to be between MINNUMER/MINDENOM and MAXNUMER/MAXDENOM. Return a tuple
        of the actual values if no argument is given.
        """

    @overload
    def wm_aspect(
        self, minNumer: None = None, minDenom: None = None, maxNumer: None = None, maxDenom: None = None
    ) -> tuple[int, int, int, int] | None: ...
    aspect = wm_aspect
    if sys.version_info >= (3, 13):
        @overload
        def wm_attributes(self, *, return_python_dict: Literal[False] = False) -> tuple[Any, ...]:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, *, return_python_dict: Literal[True]) -> _WmAttributes: ...

    else:
        @overload
        def wm_attributes(self) -> tuple[Any, ...]:
            """This subcommand returns or sets platform specific attributes

            The first form returns a list of the platform specific flags and
            their values. The second form returns the value for the specific
            option. The third form sets one or more of the values. The values
            are as follows:

            On Windows, -disabled gets or sets whether the window is in a
            disabled state. -toolwindow gets or sets the style of the window
            to toolwindow (as defined in the MSDN). -topmost gets or sets
            whether this is a topmost window (displays above all other
            windows).

            On Macintosh, XXXXX

            On Unix, there are currently no special attribute values.
            """

    @overload
    def wm_attributes(self, option: Literal["-alpha"], /) -> float:
        """Return or sets platform specific attributes.

        When called with a single argument return_python_dict=True,
        return a dict of the platform specific attributes and their values.
        When called without arguments or with a single argument
        return_python_dict=False, return a tuple containing intermixed
        attribute names with the minus prefix and their values.

        When called with a single string value, return the value for the
        specific option.  When called with keyword arguments, set the
        corresponding attributes.
        """

    @overload
    def wm_attributes(self, option: Literal["-fullscreen"], /) -> bool: ...
    @overload
    def wm_attributes(self, option: Literal["-topmost"], /) -> bool: ...
    if sys.platform == "darwin":
        @overload
        def wm_attributes(self, option: Literal["-modified"], /) -> bool:
            """This subcommand returns or sets platform specific attributes

            The first form returns a list of the platform specific flags and
            their values. The second form returns the value for the specific
            option. The third form sets one or more of the values. The values
            are as follows:

            On Windows, -disabled gets or sets whether the window is in a
            disabled state. -toolwindow gets or sets the style of the window
            to toolwindow (as defined in the MSDN). -topmost gets or sets
            whether this is a topmost window (displays above all other
            windows).

            On Macintosh, XXXXX

            On Unix, there are currently no special attribute values.
            """

        @overload
        def wm_attributes(self, option: Literal["-notify"], /) -> bool: ...
        @overload
        def wm_attributes(self, option: Literal["-titlepath"], /) -> str: ...
        @overload
        def wm_attributes(self, option: Literal["-transparent"], /) -> bool: ...
        @overload
        def wm_attributes(self, option: Literal["-type"], /) -> str: ...
    elif sys.platform == "win32":
        @overload
        def wm_attributes(self, option: Literal["-transparentcolor"], /) -> str:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, option: Literal["-disabled"], /) -> bool: ...
        @overload
        def wm_attributes(self, option: Literal["-toolwindow"], /) -> bool: ...
    else:
        # X11
        @overload
        def wm_attributes(self, option: Literal["-zoomed"], /) -> bool:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, option: Literal["-type"], /) -> str: ...
    if sys.version_info >= (3, 13):
        @overload
        def wm_attributes(self, option: Literal["alpha"], /) -> float:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, option: Literal["fullscreen"], /) -> bool: ...
        @overload
        def wm_attributes(self, option: Literal["topmost"], /) -> bool: ...
        if sys.platform == "darwin":
            @overload
            def wm_attributes(self, option: Literal["modified"], /) -> bool: ...
            @overload
            def wm_attributes(self, option: Literal["notify"], /) -> bool: ...
            @overload
            def wm_attributes(self, option: Literal["titlepath"], /) -> str: ...
            @overload
            def wm_attributes(self, option: Literal["transparent"], /) -> bool: ...
            @overload
            def wm_attributes(self, option: Literal["type"], /) -> str: ...
        elif sys.platform == "win32":
            @overload
            def wm_attributes(self, option: Literal["transparentcolor"], /) -> str:
                """Return or sets platform specific attributes.

                When called with a single argument return_python_dict=True,
                return a dict of the platform specific attributes and their values.
                When called without arguments or with a single argument
                return_python_dict=False, return a tuple containing intermixed
                attribute names with the minus prefix and their values.

                When called with a single string value, return the value for the
                specific option.  When called with keyword arguments, set the
                corresponding attributes.
                """

            @overload
            def wm_attributes(self, option: Literal["disabled"], /) -> bool: ...
            @overload
            def wm_attributes(self, option: Literal["toolwindow"], /) -> bool: ...
        else:
            # X11
            @overload
            def wm_attributes(self, option: Literal["zoomed"], /) -> bool:
                """Return or sets platform specific attributes.

                When called with a single argument return_python_dict=True,
                return a dict of the platform specific attributes and their values.
                When called without arguments or with a single argument
                return_python_dict=False, return a tuple containing intermixed
                attribute names with the minus prefix and their values.

                When called with a single string value, return the value for the
                specific option.  When called with keyword arguments, set the
                corresponding attributes.
                """

            @overload
            def wm_attributes(self, option: Literal["type"], /) -> str: ...

    @overload
    def wm_attributes(self, option: str, /): ...
    @overload
    def wm_attributes(self, option: Literal["-alpha"], value: float, /) -> Literal[""]: ...
    @overload
    def wm_attributes(self, option: Literal["-fullscreen"], value: bool, /) -> Literal[""]: ...
    @overload
    def wm_attributes(self, option: Literal["-topmost"], value: bool, /) -> Literal[""]: ...
    if sys.platform == "darwin":
        @overload
        def wm_attributes(self, option: Literal["-modified"], value: bool, /) -> Literal[""]:
            """This subcommand returns or sets platform specific attributes

            The first form returns a list of the platform specific flags and
            their values. The second form returns the value for the specific
            option. The third form sets one or more of the values. The values
            are as follows:

            On Windows, -disabled gets or sets whether the window is in a
            disabled state. -toolwindow gets or sets the style of the window
            to toolwindow (as defined in the MSDN). -topmost gets or sets
            whether this is a topmost window (displays above all other
            windows).

            On Macintosh, XXXXX

            On Unix, there are currently no special attribute values.
            """

        @overload
        def wm_attributes(self, option: Literal["-notify"], value: bool, /) -> Literal[""]: ...
        @overload
        def wm_attributes(self, option: Literal["-titlepath"], value: str, /) -> Literal[""]: ...
        @overload
        def wm_attributes(self, option: Literal["-transparent"], value: bool, /) -> Literal[""]: ...
    elif sys.platform == "win32":
        @overload
        def wm_attributes(self, option: Literal["-transparentcolor"], value: str, /) -> Literal[""]:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, option: Literal["-disabled"], value: bool, /) -> Literal[""]: ...
        @overload
        def wm_attributes(self, option: Literal["-toolwindow"], value: bool, /) -> Literal[""]: ...
    else:
        # X11
        @overload
        def wm_attributes(self, option: Literal["-zoomed"], value: bool, /) -> Literal[""]:
            """Return or sets platform specific attributes.

            When called with a single argument return_python_dict=True,
            return a dict of the platform specific attributes and their values.
            When called without arguments or with a single argument
            return_python_dict=False, return a tuple containing intermixed
            attribute names with the minus prefix and their values.

            When called with a single string value, return the value for the
            specific option.  When called with keyword arguments, set the
            corresponding attributes.
            """

        @overload
        def wm_attributes(self, option: Literal["-type"], value: str, /) -> Literal[""]: ...

    @overload
    def wm_attributes(self, option: str, value, /, *__other_option_value_pairs: Any) -> Literal[""]: ...
    if sys.version_info >= (3, 13):
        if sys.platform == "darwin":
            @overload
            def wm_attributes(
                self,
                *,
                alpha: float = ...,
                fullscreen: bool = ...,
                modified: bool = ...,
                notify: bool = ...,
                titlepath: str = ...,
                topmost: bool = ...,
                transparent: bool = ...,
            ) -> None: ...
        elif sys.platform == "win32":
            @overload
            def wm_attributes(
                self,
                *,
                alpha: float = ...,
                transparentcolor: str = ...,
                disabled: bool = ...,
                fullscreen: bool = ...,
                toolwindow: bool = ...,
                topmost: bool = ...,
            ) -> None:
                """Return or sets platform specific attributes.

                When called with a single argument return_python_dict=True,
                return a dict of the platform specific attributes and their values.
                When called without arguments or with a single argument
                return_python_dict=False, return a tuple containing intermixed
                attribute names with the minus prefix and their values.

                When called with a single string value, return the value for the
                specific option.  When called with keyword arguments, set the
                corresponding attributes.
                """
        else:
            # X11
            @overload
            def wm_attributes(
                self, *, alpha: float = ..., topmost: bool = ..., zoomed: bool = ..., fullscreen: bool = ..., type: str = ...
            ) -> None:
                """Return or sets platform specific attributes.

                When called with a single argument return_python_dict=True,
                return a dict of the platform specific attributes and their values.
                When called without arguments or with a single argument
                return_python_dict=False, return a tuple containing intermixed
                attribute names with the minus prefix and their values.

                When called with a single string value, return the value for the
                specific option.  When called with keyword arguments, set the
                corresponding attributes.
                """
    attributes = wm_attributes
    def wm_client(self, name: str | None = None) -> str:
        """Store NAME in WM_CLIENT_MACHINE property of this widget. Return
        current value.
        """
    client = wm_client
    @overload
    def wm_colormapwindows(self) -> list[Misc]:
        """Store list of window names (WLIST) into WM_COLORMAPWINDOWS property
        of this widget. This list contains windows whose colormaps differ from their
        parents. Return current list of widgets if WLIST is empty.
        """

    @overload
    def wm_colormapwindows(self, wlist: list[Misc] | tuple[Misc, ...], /) -> None: ...
    @overload
    def wm_colormapwindows(self, first_wlist_item: Misc, /, *other_wlist_items: Misc) -> None: ...
    colormapwindows = wm_colormapwindows
    def wm_command(self, value: str | None = None) -> str:
        """Store VALUE in WM_COMMAND property. It is the command
        which shall be used to invoke the application. Return current
        command if VALUE is None.
        """
    command = wm_command
    # Some of these always return empty string, but return type is set to None to prevent accidentally using it
    def wm_deiconify(self) -> None:
        """Deiconify this widget. If it was never mapped it will not be mapped.
        On Windows it will raise this widget and give it the focus.
        """
    deiconify = wm_deiconify
    def wm_focusmodel(self, model: Literal["active", "passive"] | None = None) -> Literal["active", "passive", ""]:
        """Set focus model to MODEL. "active" means that this widget will claim
        the focus itself, "passive" means that the window manager shall give
        the focus. Return current focus model if MODEL is None.
        """
    focusmodel = wm_focusmodel
    def wm_forget(self, window: Wm) -> None:
        """The window will be unmapped from the screen and will no longer
        be managed by wm. toplevel windows will be treated like frame
        windows once they are no longer managed by wm, however, the menu
        option configuration will be remembered and the menus will return
        once the widget is managed again.
        """
    forget = wm_forget
    def wm_frame(self) -> str:
        """Return identifier for decorative frame of this widget if present."""
    frame = wm_frame
    @overload
    def wm_geometry(self, newGeometry: None = None) -> str:
        """Set geometry to NEWGEOMETRY of the form =widthxheight+x+y. Return
        current value if None is given.
        """

    @overload
    def wm_geometry(self, newGeometry: str) -> None: ...
    geometry = wm_geometry
    def wm_grid(self, baseWidth=None, baseHeight=None, widthInc=None, heightInc=None):
        """Instruct the window manager that this widget shall only be
        resized on grid boundaries. WIDTHINC and HEIGHTINC are the width and
        height of a grid unit in pixels. BASEWIDTH and BASEHEIGHT are the
        number of grid units requested in Tk_GeometryRequest.
        """
    grid = wm_grid
    def wm_group(self, pathName=None):
        """Set the group leader widgets for related widgets to PATHNAME. Return
        the group leader of this widget if None is given.
        """
    group = wm_group
    def wm_iconbitmap(self, bitmap=None, default=None):
        """Set bitmap for the iconified widget to BITMAP. Return
        the bitmap if None is given.

        Under Windows, the DEFAULT parameter can be used to set the icon
        for the widget and any descendants that don't have an icon set
        explicitly.  DEFAULT can be the relative path to a .ico file
        (example: root.iconbitmap(default='myicon.ico') ).  See Tk
        documentation for more information.
        """
    iconbitmap = wm_iconbitmap
    def wm_iconify(self) -> None:
        """Display widget as icon."""
    iconify = wm_iconify
    def wm_iconmask(self, bitmap=None):
        """Set mask for the icon bitmap of this widget. Return the
        mask if None is given.
        """
    iconmask = wm_iconmask
    def wm_iconname(self, newName=None) -> str:
        """Set the name of the icon for this widget. Return the name if
        None is given.
        """
    iconname = wm_iconname
    def wm_iconphoto(self, default: bool, image1: _PhotoImageLike | str, /, *args: _PhotoImageLike | str) -> None:
        """Sets the titlebar icon for this window based on the named photo
        images passed through args. If default is True, this is applied to
        all future created toplevels as well.

        The data in the images is taken as a snapshot at the time of
        invocation. If the images are later changed, this is not reflected
        to the titlebar icons. Multiple images are accepted to allow
        different images sizes to be provided. The window manager may scale
        provided icons to an appropriate size.

        On Windows, the images are packed into a Windows icon structure.
        This will override an icon specified to wm_iconbitmap, and vice
        versa.

        On X, the images are arranged into the _NET_WM_ICON X property,
        which most modern window managers support. An icon specified by
        wm_iconbitmap may exist simultaneously.

        On Macintosh, this currently does nothing.
        """
    iconphoto = wm_iconphoto
    def wm_iconposition(self, x: int | None = None, y: int | None = None) -> tuple[int, int] | None:
        """Set the position of the icon of this widget to X and Y. Return
        a tuple of the current values of X and X if None is given.
        """
    iconposition = wm_iconposition
    def wm_iconwindow(self, pathName=None):
        """Set widget PATHNAME to be displayed instead of icon. Return the current
        value if None is given.
        """
    iconwindow = wm_iconwindow
    def wm_manage(self, widget) -> None:
        """The widget specified will become a stand alone top-level window.
        The window will be decorated with the window managers title bar,
        etc.
        """
    manage = wm_manage
    @overload
    def wm_maxsize(self, width: None = None, height: None = None) -> tuple[int, int]:
        """Set max WIDTH and HEIGHT for this widget. If the window is gridded
        the values are given in grid units. Return the current values if None
        is given.
        """

    @overload
    def wm_maxsize(self, width: int, height: int) -> None: ...
    maxsize = wm_maxsize
    @overload
    def wm_minsize(self, width: None = None, height: None = None) -> tuple[int, int]:
        """Set min WIDTH and HEIGHT for this widget. If the window is gridded
        the values are given in grid units. Return the current values if None
        is given.
        """

    @overload
    def wm_minsize(self, width: int, height: int) -> None: ...
    minsize = wm_minsize
    @overload
    def wm_overrideredirect(self, boolean: None = None) -> bool | None:  # returns True or None
        """Instruct the window manager to ignore this widget
        if BOOLEAN is given with 1. Return the current value if None
        is given.
        """

    @overload
    def wm_overrideredirect(self, boolean: bool) -> None: ...
    overrideredirect = wm_overrideredirect
    def wm_positionfrom(self, who: Literal["program", "user"] | None = None) -> Literal["", "program", "user"]:
        """Instruct the window manager that the position of this widget shall
        be defined by the user if WHO is "user", and by its own policy if WHO is
        "program".
        """
    positionfrom = wm_positionfrom
    @overload
    def wm_protocol(self, name: str, func: Callable[[], object] | str) -> None:
        """Bind function FUNC to command NAME for this widget.
        Return the function bound to NAME if None is given. NAME could be
        e.g. "WM_SAVE_YOURSELF" or "WM_DELETE_WINDOW".
        """

    @overload
    def wm_protocol(self, name: str, func: None = None) -> str: ...
    @overload
    def wm_protocol(self, name: None = None, func: None = None) -> tuple[str, ...]: ...
    protocol = wm_protocol
    @overload
    def wm_resizable(self, width: None = None, height: None = None) -> tuple[bool, bool]:
        """Instruct the window manager whether this width can be resized
        in WIDTH or HEIGHT. Both values are boolean values.
        """

    @overload
    def wm_resizable(self, width: bool, height: bool) -> None: ...
    resizable = wm_resizable
    def wm_sizefrom(self, who: Literal["program", "user"] | None = None) -> Literal["", "program", "user"]:
        """Instruct the window manager that the size of this widget shall
        be defined by the user if WHO is "user", and by its own policy if WHO is
        "program".
        """
    sizefrom = wm_sizefrom
    @overload
    def wm_state(self, newstate: None = None) -> str:
        """Query or set the state of this widget as one of normal, icon,
        iconic (see wm_iconwindow), withdrawn, or zoomed (Windows only).
        """

    @overload
    def wm_state(self, newstate: str) -> None: ...
    state = wm_state
    @overload
    def wm_title(self, string: None = None) -> str:
        """Set the title of this widget."""

    @overload
    def wm_title(self, string: str) -> None: ...
    title = wm_title
    @overload
    def wm_transient(self, master: None = None) -> _tkinter.Tcl_Obj:
        """Instruct the window manager that this widget is transient
        with regard to widget MASTER.
        """

    @overload
    def wm_transient(self, master: Wm | _tkinter.Tcl_Obj) -> None: ...
    transient = wm_transient
    def wm_withdraw(self) -> None:
        """Withdraw this widget from the screen such that it is unmapped
        and forgotten by the window manager. Re-draw it with wm_deiconify.
        """
    withdraw = wm_withdraw

class Tk(Misc, Wm):
    """Toplevel widget of Tk which represents mostly the main window
    of an application. It has an associated Tcl interpreter.
    """

    master: None
    def __init__(
        # Make sure to keep in sync with other functions that use the same
        # args.
        # use `git grep screenName` to find them
        self,
        screenName: str | None = None,
        baseName: str | None = None,
        className: str = "Tk",
        useTk: bool = True,
        sync: bool = False,
        use: str | None = None,
    ) -> None:
        """Return a new top level widget on screen SCREENNAME. A new Tcl interpreter will
        be created. BASENAME will be used for the identification of the profile file (see
        readprofile).
        It is constructed from sys.argv[0] without extensions if None is given. CLASSNAME
        is the name of the widget class.
        """
    # Keep this in sync with ttktheme.ThemedTk. See issue #13858
    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        menu: Menu = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def destroy(self) -> None:
        """Destroy this and all descendants widgets. This will
        end the application of this Tcl interpreter.
        """

    def readprofile(self, baseName: str, className: str) -> None:
        """Internal function. It reads .BASENAME.tcl and .CLASSNAME.tcl into
        the Tcl Interpreter and calls exec on the contents of .BASENAME.py and
        .CLASSNAME.py if such a file exists in the home directory.
        """
    report_callback_exception: Callable[[type[BaseException], BaseException, TracebackType | None], object]
    # Tk has __getattr__ so that tk_instance.foo falls back to tk_instance.tk.foo
    # Please keep in sync with _tkinter.TkappType.
    # Some methods are intentionally missing because they are inherited from Misc instead.
    def adderrorinfo(self, msg: str, /): ...
    def call(self, command: Any, /, *args: Any) -> Any: ...
    def createcommand(self, name: str, func, /): ...
    if sys.platform != "win32":
        def createfilehandler(self, file, mask: int, func, /): ...
        def deletefilehandler(self, file, /) -> None: ...

    def createtimerhandler(self, milliseconds: int, func, /): ...
    def dooneevent(self, flags: int = 0, /): ...
    def eval(self, script: str, /) -> str: ...
    def evalfile(self, fileName: str, /): ...
    def exprboolean(self, s: str, /): ...
    def exprdouble(self, s: str, /): ...
    def exprlong(self, s: str, /): ...
    def exprstring(self, s: str, /): ...
    def globalgetvar(self, *args, **kwargs): ...
    def globalsetvar(self, *args, **kwargs): ...
    def globalunsetvar(self, *args, **kwargs): ...
    def interpaddr(self) -> int: ...
    def loadtk(self) -> None: ...
    def record(self, script: str, /): ...
    if sys.version_info < (3, 11):
        @deprecated("Deprecated since Python 3.9; removed in Python 3.11. Use `splitlist()` instead.")
        def split(self, arg, /): ...

    def splitlist(self, arg, /): ...
    def unsetvar(self, *args, **kwargs): ...
    def wantobjects(self, *args, **kwargs): ...
    def willdispatch(self) -> None: ...

def Tcl(screenName: str | None = None, baseName: str | None = None, className: str = "Tk", useTk: bool = False) -> Tk: ...

_InMiscTotal = TypedDict("_InMiscTotal", {"in": Misc})
_InMiscNonTotal = TypedDict("_InMiscNonTotal", {"in": Misc}, total=False)

@type_check_only
class _PackInfo(_InMiscTotal):
    # 'before' and 'after' never appear in _PackInfo
    anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"]
    expand: bool
    fill: Literal["none", "x", "y", "both"]
    side: Literal["left", "right", "top", "bottom"]
    # Paddings come out as int or tuple of int, even though any screen units
    # can be specified in pack().
    ipadx: int
    ipady: int
    padx: int | tuple[int, int]
    pady: int | tuple[int, int]

class Pack:
    """Geometry manager Pack.

    Base class to use the methods pack_* in every widget.
    """

    # _PackInfo is not the valid type for cnf because pad stuff accepts any
    # screen units instead of int only. I didn't bother to create another
    # TypedDict for cnf because it appears to be a legacy thing that was
    # replaced by **kwargs.
    def pack_configure(
        self,
        cnf: Mapping[str, Any] | None = {},
        *,
        after: Misc = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        before: Misc = ...,
        expand: bool | Literal[0, 1] = 0,
        fill: Literal["none", "x", "y", "both"] = ...,
        side: Literal["left", "right", "top", "bottom"] = ...,
        ipadx: float | str = ...,
        ipady: float | str = ...,
        padx: float | str | tuple[float | str, float | str] = ...,
        pady: float | str | tuple[float | str, float | str] = ...,
        in_: Misc = ...,
        **kw: Any,  # allow keyword argument named 'in', see #4836
    ) -> None:
        """Pack a widget in the parent widget. Use as options:
        after=widget - pack it after you have packed widget
        anchor=NSEW (or subset) - position widget according to
                                  given direction
        before=widget - pack it before you will pack widget
        expand=bool - expand widget if parent size grows
        fill=NONE or X or Y or BOTH - fill widget if widget grows
        in=master - use master to contain this widget
        in_=master - see 'in' option description
        ipadx=amount - add internal padding in x direction
        ipady=amount - add internal padding in y direction
        padx=amount - add padding in x direction
        pady=amount - add padding in y direction
        side=TOP or BOTTOM or LEFT or RIGHT -  where to add this widget.
        """

    def pack_forget(self) -> None:
        """Unmap this widget and do not use it for the packing order."""

    def pack_info(self) -> _PackInfo:  # errors if widget hasn't been packed
        """Return information about the packing options
        for this widget.
        """
    pack = pack_configure
    forget = pack_forget
    propagate = Misc.pack_propagate

@type_check_only
class _PlaceInfo(_InMiscNonTotal):  # empty dict if widget hasn't been placed
    anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"]
    bordermode: Literal["inside", "outside", "ignore"]
    width: str  # can be int()ed (even after e.g. widget.place(height='2.3c') or similar)
    height: str  # can be int()ed
    x: str  # can be int()ed
    y: str  # can be int()ed
    relheight: str  # can be float()ed if not empty string
    relwidth: str  # can be float()ed if not empty string
    relx: str  # can be float()ed if not empty string
    rely: str  # can be float()ed if not empty string

class Place:
    """Geometry manager Place.

    Base class to use the methods place_* in every widget.
    """

    def place_configure(
        self,
        cnf: Mapping[str, Any] | None = {},
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        bordermode: Literal["inside", "outside", "ignore"] = ...,
        width: float | str = ...,
        height: float | str = ...,
        x: float | str = ...,
        y: float | str = ...,
        # str allowed for compatibility with place_info()
        relheight: str | float = ...,
        relwidth: str | float = ...,
        relx: str | float = ...,
        rely: str | float = ...,
        in_: Misc = ...,
        **kw: Any,  # allow keyword argument named 'in', see #4836
    ) -> None:
        """Place a widget in the parent widget. Use as options:
        in=master - master relative to which the widget is placed
        in_=master - see 'in' option description
        x=amount - locate anchor of this widget at position x of master
        y=amount - locate anchor of this widget at position y of master
        relx=amount - locate anchor of this widget between 0.0 and 1.0
                      relative to width of master (1.0 is right edge)
        rely=amount - locate anchor of this widget between 0.0 and 1.0
                      relative to height of master (1.0 is bottom edge)
        anchor=NSEW (or subset) - position anchor according to given direction
        width=amount - width of this widget in pixel
        height=amount - height of this widget in pixel
        relwidth=amount - width of this widget between 0.0 and 1.0
                          relative to width of master (1.0 is the same width
                          as the master)
        relheight=amount - height of this widget between 0.0 and 1.0
                           relative to height of master (1.0 is the same
                           height as the master)
        bordermode="inside" or "outside" - whether to take border width of
                                           master widget into account
        """

    def place_forget(self) -> None:
        """Unmap this widget."""

    def place_info(self) -> _PlaceInfo:
        """Return information about the placing options
        for this widget.
        """
    place = place_configure
    info = place_info

@type_check_only
class _GridInfo(_InMiscNonTotal):  # empty dict if widget hasn't been gridded
    column: int
    columnspan: int
    row: int
    rowspan: int
    ipadx: int
    ipady: int
    padx: int | tuple[int, int]
    pady: int | tuple[int, int]
    sticky: str  # consists of letters 'n', 's', 'w', 'e', no repeats, may be empty

class Grid:
    """Geometry manager Grid.

    Base class to use the methods grid_* in every widget.
    """

    def grid_configure(
        self,
        cnf: Mapping[str, Any] | None = {},
        *,
        column: int = ...,
        columnspan: int = ...,
        row: int = ...,
        rowspan: int = ...,
        ipadx: float | str = ...,
        ipady: float | str = ...,
        padx: float | str | tuple[float | str, float | str] = ...,
        pady: float | str | tuple[float | str, float | str] = ...,
        sticky: str = ...,  # consists of letters 'n', 's', 'w', 'e', may contain repeats, may be empty
        in_: Misc = ...,
        **kw: Any,  # allow keyword argument named 'in', see #4836
    ) -> None:
        """Position a widget in the parent widget in a grid. Use as options:
        column=number - use cell identified with given column (starting with 0)
        columnspan=number - this widget will span several columns
        in=master - use master to contain this widget
        in_=master - see 'in' option description
        ipadx=amount - add internal padding in x direction
        ipady=amount - add internal padding in y direction
        padx=amount - add padding in x direction
        pady=amount - add padding in y direction
        row=number - use cell identified with given row (starting with 0)
        rowspan=number - this widget will span several rows
        sticky=NSEW - if cell is larger on which sides will this
                      widget stick to the cell boundary
        """

    def grid_forget(self) -> None:
        """Unmap this widget."""

    def grid_remove(self) -> None:
        """Unmap this widget but remember the grid options."""

    def grid_info(self) -> _GridInfo:
        """Return information about the options
        for positioning this widget in a grid.
        """
    grid = grid_configure
    location = Misc.grid_location
    size = Misc.grid_size

class BaseWidget(Misc):
    """Internal class."""

    master: Misc
    widgetName: str
    def __init__(self, master, widgetName: str, cnf={}, kw={}, extra=()) -> None:
        """Construct a widget with the parent widget MASTER, a name WIDGETNAME
        and appropriate options.
        """

    def destroy(self) -> None:
        """Destroy this and all descendants widgets."""

# This class represents any widget except Toplevel or Tk.
class Widget(BaseWidget, Pack, Place, Grid):
    """Internal class.

    Base class for a widget which can be positioned with the geometry managers
    Pack, Place or Grid.
    """

    # Allow bind callbacks to take e.g. Event[Label] instead of Event[Misc].
    # Tk and Toplevel get notified for their child widgets' events, but other
    # widgets don't.
    @overload
    def bind(
        self: _W,
        sequence: str | None = None,
        func: Callable[[Event[_W]], object] | None = None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to this widget at event SEQUENCE a call to function FUNC.

        SEQUENCE is a string of concatenated event
        patterns. An event pattern is of the form
        <MODIFIER-MODIFIER-TYPE-DETAIL> where MODIFIER is one
        of Control, Mod2, M2, Shift, Mod3, M3, Lock, Mod4, M4,
        Button1, B1, Mod5, M5 Button2, B2, Meta, M, Button3,
        B3, Alt, Button4, B4, Double, Button5, B5 Triple,
        Mod1, M1. TYPE is one of Activate, Enter, Map,
        ButtonPress, Button, Expose, Motion, ButtonRelease
        FocusIn, MouseWheel, Circulate, FocusOut, Property,
        Colormap, Gravity Reparent, Configure, KeyPress, Key,
        Unmap, Deactivate, KeyRelease Visibility, Destroy,
        Leave and DETAIL is the button number for ButtonPress,
        ButtonRelease and DETAIL is the Keysym for KeyPress and
        KeyRelease. Examples are
        <Control-Button-1> for pressing Control and mouse button 1 or
        <Alt-A> for pressing A and the Alt key (KeyPress can be omitted).
        An event pattern can also be a virtual event of the form
        <<AString>> where AString can be arbitrary. This
        event can be generated by event_generate.
        If events are concatenated they must appear shortly
        after each other.

        FUNC will be called if the event sequence occurs with an
        instance of Event as argument. If the return value of FUNC is
        "break" no further bound function is invoked.

        An additional boolean parameter ADD specifies whether FUNC will
        be called additionally to the other bound function or whether
        it will replace the previous function.

        Bind will return an identifier to allow deletion of the bound function with
        unbind without memory leak.

        If FUNC or SEQUENCE is omitted the bound function or list
        of bound events are returned.
        """

    @overload
    def bind(self, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    @overload
    def bind(self, *, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...

class Toplevel(BaseWidget, Wm):
    """Toplevel widget, e.g. for dialogs."""

    # Toplevel and Tk have the same options because they correspond to the same
    # Tcl/Tk toplevel widget. For some reason, config and configure must be
    # copy/pasted here instead of aliasing as 'config = Tk.config'.
    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = 0,
        bg: str = ...,
        border: float | str = 0,
        borderwidth: float | str = 0,
        class_: str = "Toplevel",
        colormap: Literal["new", ""] | Misc = "",
        container: bool = False,
        cursor: _Cursor = "",
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        menu: Menu = ...,
        name: str = ...,
        padx: float | str = 0,
        pady: float | str = 0,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        screen: str = "",  # can't be changed after creating widget
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        use: int = ...,
        visual: str | tuple[str, int] = "",
        width: float | str = 0,
    ) -> None:
        """Construct a toplevel widget with the parent MASTER.

        Valid resource names: background, bd, bg, borderwidth, class,
        colormap, container, cursor, height, highlightbackground,
        highlightcolor, highlightthickness, menu, relief, screen, takefocus,
        use, visual, width.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        menu: Menu = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Button(Widget):
    """Button widget."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = "center",
        background: str = ...,
        bd: float | str = ...,  # same as borderwidth
        bg: str = ...,  # same as background
        bitmap: str = "",
        border: float | str = ...,  # same as borderwidth
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = "",
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = "none",
        cursor: _Cursor = "",
        default: Literal["normal", "active", "disabled"] = "disabled",
        disabledforeground: str = ...,
        fg: str = ...,  # same as foreground
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        # width and height must be int for buttons containing just text, but
        # buttons with an image accept any screen units.
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 1,
        image: _Image | str = "",
        justify: Literal["left", "center", "right"] = "center",
        name: str = ...,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = "",
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = ...,
        repeatinterval: int = ...,
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        text: float | str = "",
        # We allow the textvariable to be any Variable, not necessarily
        # StringVar. This is useful for e.g. a button that displays the value
        # of an IntVar.
        textvariable: Variable = ...,
        underline: int = -1,
        width: float | str = 0,
        wraplength: float | str = 0,
    ) -> None:
        """Construct a button widget with the parent MASTER.

        STANDARD OPTIONS

            activebackground, activeforeground, anchor,
            background, bitmap, borderwidth, cursor,
            disabledforeground, font, foreground
            highlightbackground, highlightcolor,
            highlightthickness, image, justify,
            padx, pady, relief, repeatdelay,
            repeatinterval, takefocus, text,
            textvariable, underline, wraplength

        WIDGET-SPECIFIC OPTIONS

            command, compound, default, height,
            overrelief, state, width
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: _Cursor = ...,
        default: Literal["normal", "active", "disabled"] = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        image: _Image | str = ...,
        justify: Literal["left", "center", "right"] = ...,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = ...,
        repeatinterval: int = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        underline: int = ...,
        width: float | str = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def flash(self) -> None:
        """Flash the button.

        This is accomplished by redisplaying
        the button several times, alternating between active and
        normal colors. At the end of the flash the button is left
        in the same normal/active state as when the command was
        invoked. This command is ignored if the button's state is
        disabled.
        """

    def invoke(self) -> Any:
        """Invoke the command associated with the button.

        The return value is the return value from the command,
        or an empty string if there is no command associated with
        the button. This command is ignored if the button's state
        is disabled.
        """

class Canvas(Widget, XView, YView):
    """Canvas widget to display graphical elements like lines or text."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = 0,
        bg: str = ...,
        border: float | str = 0,
        borderwidth: float | str = 0,
        closeenough: float = 1.0,
        confine: bool = True,
        cursor: _Cursor = "",
        height: float | str = ...,  # see COORDINATES in canvas manual page
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = 0,
        insertofftime: int = 300,
        insertontime: int = 600,
        insertwidth: float | str = 2,
        name: str = ...,
        offset=...,  # undocumented
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        # Setting scrollregion to None doesn't reset it back to empty,
        # but setting it to () does.
        scrollregion: tuple[float | str, float | str, float | str, float | str] | tuple[()] = (),
        selectbackground: str = ...,
        selectborderwidth: float | str = 1,
        selectforeground: str = ...,
        # man page says that state can be 'hidden', but it can't
        state: Literal["normal", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        width: float | str = ...,
        xscrollcommand: str | Callable[[float, float], object] = "",
        xscrollincrement: float | str = 0,
        yscrollcommand: str | Callable[[float, float], object] = "",
        yscrollincrement: float | str = 0,
    ) -> None:
        """Construct a canvas widget with the parent MASTER.

        Valid resource names: background, bd, bg, borderwidth, closeenough,
        confine, cursor, height, highlightbackground, highlightcolor,
        highlightthickness, insertbackground, insertborderwidth,
        insertofftime, insertontime, insertwidth, offset, relief,
        scrollregion, selectbackground, selectborderwidth, selectforeground,
        state, takefocus, width, xscrollcommand, xscrollincrement,
        yscrollcommand, yscrollincrement.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        closeenough: float = ...,
        confine: bool = ...,
        cursor: _Cursor = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = ...,
        insertofftime: int = ...,
        insertontime: int = ...,
        insertwidth: float | str = ...,
        offset=...,  # undocumented
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        scrollregion: tuple[float | str, float | str, float | str, float | str] | tuple[()] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        state: Literal["normal", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: float | str = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
        xscrollincrement: float | str = ...,
        yscrollcommand: str | Callable[[float, float], object] = ...,
        yscrollincrement: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def addtag(self, *args):  # internal method
        """Internal function."""

    def addtag_above(self, newtag: str, tagOrId: str | int) -> None:
        """Add tag NEWTAG to all items above TAGORID."""

    def addtag_all(self, newtag: str) -> None:
        """Add tag NEWTAG to all items."""

    def addtag_below(self, newtag: str, tagOrId: str | int) -> None:
        """Add tag NEWTAG to all items below TAGORID."""

    def addtag_closest(
        self, newtag: str, x: float | str, y: float | str, halo: float | str | None = None, start: str | int | None = None
    ) -> None:
        """Add tag NEWTAG to item which is closest to pixel at X, Y.
        If several match take the top-most.
        All items closer than HALO are considered overlapping (all are
        closest). If START is specified the next below this tag is taken.
        """

    def addtag_enclosed(self, newtag: str, x1: float | str, y1: float | str, x2: float | str, y2: float | str) -> None:
        """Add tag NEWTAG to all items in the rectangle defined
        by X1,Y1,X2,Y2.
        """

    def addtag_overlapping(self, newtag: str, x1: float | str, y1: float | str, x2: float | str, y2: float | str) -> None:
        """Add tag NEWTAG to all items which overlap the rectangle
        defined by X1,Y1,X2,Y2.
        """

    def addtag_withtag(self, newtag: str, tagOrId: str | int) -> None:
        """Add tag NEWTAG to all items with TAGORID."""

    def find(self, *args):  # internal method
        """Internal function."""

    def find_above(self, tagOrId: str | int) -> tuple[int, ...]:
        """Return items above TAGORID."""

    def find_all(self) -> tuple[int, ...]:
        """Return all items."""

    def find_below(self, tagOrId: str | int) -> tuple[int, ...]:
        """Return all items below TAGORID."""

    def find_closest(
        self, x: float | str, y: float | str, halo: float | str | None = None, start: str | int | None = None
    ) -> tuple[int, ...]:
        """Return item which is closest to pixel at X, Y.
        If several match take the top-most.
        All items closer than HALO are considered overlapping (all are
        closest). If START is specified the next below this tag is taken.
        """

    def find_enclosed(self, x1: float | str, y1: float | str, x2: float | str, y2: float | str) -> tuple[int, ...]:
        """Return all items in rectangle defined
        by X1,Y1,X2,Y2.
        """

    def find_overlapping(self, x1: float | str, y1: float | str, x2: float | str, y2: float) -> tuple[int, ...]:
        """Return all items which overlap the rectangle
        defined by X1,Y1,X2,Y2.
        """

    def find_withtag(self, tagOrId: str | int) -> tuple[int, ...]:
        """Return all items with TAGORID."""
    # Incompatible with Misc.bbox(), tkinter violates LSP
    def bbox(self, *args: str | int) -> tuple[int, int, int, int]:  # type: ignore[override]
        """Return a tuple of X1,Y1,X2,Y2 coordinates for a rectangle
        which encloses all items with tags specified as arguments.
        """

    @overload
    def tag_bind(
        self,
        tagOrId: str | int,
        sequence: str | None = None,
        func: Callable[[Event[Canvas]], object] | None = None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to all items with TAGORID at event SEQUENCE a call to function FUNC.

        An additional boolean parameter ADD specifies whether FUNC will be
        called additionally to the other bound function or whether it will
        replace the previous function. See bind for the return value.
        """

    @overload
    def tag_bind(
        self, tagOrId: str | int, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None
    ) -> None: ...
    @overload
    def tag_bind(self, tagOrId: str | int, *, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    def tag_unbind(self, tagOrId: str | int, sequence: str, funcid: str | None = None) -> None:
        """Unbind for all items with TAGORID for event SEQUENCE  the
        function identified with FUNCID.
        """

    def canvasx(self, screenx, gridspacing=None):
        """Return the canvas x coordinate of pixel position SCREENX rounded
        to nearest multiple of GRIDSPACING units.
        """

    def canvasy(self, screeny, gridspacing=None):
        """Return the canvas y coordinate of pixel position SCREENY rounded
        to nearest multiple of GRIDSPACING units.
        """

    @overload
    def coords(self, tagOrId: str | int, /) -> list[float]:
        """Return a list of coordinates for the item given in ARGS."""

    @overload
    def coords(self, tagOrId: str | int, args: list[int] | list[float] | tuple[float, ...], /) -> None: ...
    @overload
    def coords(self, tagOrId: str | int, x1: float, y1: float, /, *args: float) -> None: ...
    # create_foo() methods accept coords as a list or tuple, or as separate arguments.
    # Lists and tuples can be flat as in [1, 2, 3, 4], or nested as in [(1, 2), (3, 4)].
    # Keyword arguments should be the same in all overloads of each method.
    def create_arc(self, *args, **kw) -> int:
        """Create arc shaped region with coordinates x1,y1,x2,y2."""

    def create_bitmap(self, *args, **kw) -> int:
        """Create bitmap with coordinates x1,y1."""

    def create_image(self, *args, **kw) -> int:
        """Create image item with coordinates x1,y1."""

    @overload
    def create_line(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        arrow: Literal["first", "last", "both"] = ...,
        arrowshape: tuple[float, float, float] = ...,
        capstyle: Literal["round", "projecting", "butt"] = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int:
        """Create line with coordinates x1,y1,...,xn,yn."""

    @overload
    def create_line(
        self,
        xy_pair_0: tuple[float, float],
        xy_pair_1: tuple[float, float],
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        arrow: Literal["first", "last", "both"] = ...,
        arrowshape: tuple[float, float, float] = ...,
        capstyle: Literal["round", "projecting", "butt"] = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_line(
        self,
        coords: (
            tuple[float, float, float, float]
            | tuple[tuple[float, float], tuple[float, float]]
            | list[int]
            | list[float]
            | list[tuple[int, int]]
            | list[tuple[float, float]]
        ),
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        arrow: Literal["first", "last", "both"] = ...,
        arrowshape: tuple[float, float, float] = ...,
        capstyle: Literal["round", "projecting", "butt"] = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_oval(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int:
        """Create oval with coordinates x1,y1,x2,y2."""

    @overload
    def create_oval(
        self,
        xy_pair_0: tuple[float, float],
        xy_pair_1: tuple[float, float],
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_oval(
        self,
        coords: (
            tuple[float, float, float, float]
            | tuple[tuple[float, float], tuple[float, float]]
            | list[int]
            | list[float]
            | list[tuple[int, int]]
            | list[tuple[float, float]]
        ),
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_polygon(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        /,
        *xy_pairs: float,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int:
        """Create polygon with coordinates x1,y1,...,xn,yn."""

    @overload
    def create_polygon(
        self,
        xy_pair_0: tuple[float, float],
        xy_pair_1: tuple[float, float],
        /,
        *xy_pairs: tuple[float, float],
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_polygon(
        self,
        coords: (
            tuple[float, ...]
            | tuple[tuple[float, float], ...]
            | list[int]
            | list[float]
            | list[tuple[int, int]]
            | list[tuple[float, float]]
        ),
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        joinstyle: Literal["round", "bevel", "miter"] = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        smooth: bool = ...,
        splinesteps: float = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_rectangle(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int:
        """Create rectangle with coordinates x1,y1,x2,y2."""

    @overload
    def create_rectangle(
        self,
        xy_pair_0: tuple[float, float],
        xy_pair_1: tuple[float, float],
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_rectangle(
        self,
        coords: (
            tuple[float, float, float, float]
            | tuple[tuple[float, float], tuple[float, float]]
            | list[int]
            | list[float]
            | list[tuple[int, int]]
            | list[tuple[float, float]]
        ),
        /,
        *,
        activedash: str | int | list[int] | tuple[int, ...] = ...,
        activefill: str = ...,
        activeoutline: str = ...,
        activeoutlinestipple: str = ...,
        activestipple: str = ...,
        activewidth: float | str = ...,
        dash: str | int | list[int] | tuple[int, ...] = ...,
        dashoffset: float | str = ...,
        disableddash: str | int | list[int] | tuple[int, ...] = ...,
        disabledfill: str = ...,
        disabledoutline: str = ...,
        disabledoutlinestipple: str = ...,
        disabledstipple: str = ...,
        disabledwidth: float | str = ...,
        fill: str = ...,
        offset: float | str = ...,
        outline: str = ...,
        outlineoffset: float | str = ...,
        outlinestipple: str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_text(
        self,
        x: float,
        y: float,
        /,
        *,
        activefill: str = ...,
        activestipple: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        angle: float | str = ...,
        disabledfill: str = ...,
        disabledstipple: str = ...,
        fill: str = ...,
        font: _FontDescription = ...,
        justify: Literal["left", "center", "right"] = ...,
        offset: float | str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        text: float | str = ...,
        width: float | str = ...,
    ) -> int:
        """Create text with coordinates x1,y1."""

    @overload
    def create_text(
        self,
        coords: tuple[float, float] | list[int] | list[float],
        /,
        *,
        activefill: str = ...,
        activestipple: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        angle: float | str = ...,
        disabledfill: str = ...,
        disabledstipple: str = ...,
        fill: str = ...,
        font: _FontDescription = ...,
        justify: Literal["left", "center", "right"] = ...,
        offset: float | str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        stipple: str = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        text: float | str = ...,
        width: float | str = ...,
    ) -> int: ...
    @overload
    def create_window(
        self,
        x: float,
        y: float,
        /,
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        height: float | str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
        window: Widget = ...,
    ) -> int:
        """Create window with coordinates x1,y1,x2,y2."""

    @overload
    def create_window(
        self,
        coords: tuple[float, float] | list[int] | list[float],
        /,
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        height: float | str = ...,
        state: Literal["normal", "hidden", "disabled"] = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
        width: float | str = ...,
        window: Widget = ...,
    ) -> int: ...
    def dchars(self, *args) -> None:
        """Delete characters of text items identified by tag or id in ARGS (possibly
        several times) from FIRST to LAST character (including).
        """

    def delete(self, *tagsOrCanvasIds: str | int) -> None:
        """Delete items identified by all tag or ids contained in ARGS."""

    @overload
    def dtag(self, tag: str, tag_to_delete: str | None = ..., /) -> None:
        """Delete tag or id given as last arguments in ARGS from items
        identified by first argument in ARGS.
        """

    @overload
    def dtag(self, id: int, tag_to_delete: str, /) -> None: ...
    def focus(self, *args):
        """Set focus to the first item specified in ARGS."""

    def gettags(self, tagOrId: str | int, /) -> tuple[str, ...]:
        """Return tags associated with the first item specified in ARGS."""

    def icursor(self, *args) -> None:
        """Set cursor at position POS in the item identified by TAGORID.
        In ARGS TAGORID must be first.
        """

    def index(self, *args):
        """Return position of cursor as integer in item specified in ARGS."""

    def insert(self, *args) -> None:
        """Insert TEXT in item TAGORID at position POS. ARGS must
        be TAGORID POS TEXT.
        """

    def itemcget(self, tagOrId, option):
        """Return the resource value for an OPTION for item TAGORID."""
    # itemconfigure kwargs depend on item type, which is not known when type checking
    def itemconfigure(
        self, tagOrId: str | int, cnf: dict[str, Any] | None = None, **kw: Any
    ) -> dict[str, tuple[str, str, str, str, str]] | None:
        """Configure resources of an item TAGORID.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method without arguments.
        """
    itemconfig = itemconfigure
    def move(self, *args) -> None:
        """Move an item TAGORID given in ARGS."""

    def moveto(self, tagOrId: str | int, x: Literal[""] | float = "", y: Literal[""] | float = "") -> None:
        """Move the items given by TAGORID in the canvas coordinate
        space so that the first coordinate pair of the bottommost
        item with tag TAGORID is located at position (X,Y).
        X and Y may be the empty string, in which case the
        corresponding coordinate will be unchanged. All items matching
        TAGORID remain in the same positions relative to each other.
        """

    def postscript(self, cnf={}, **kw):
        """Print the contents of the canvas to a postscript
        file. Valid options: colormap, colormode, file, fontmap,
        height, pageanchor, pageheight, pagewidth, pagex, pagey,
        rotate, width, x, y.
        """
    # tkinter does:
    #    lower = tag_lower
    #    lift = tkraise = tag_raise
    #
    # But mypy doesn't like aliasing here (maybe because Misc defines the same names)
    def tag_lower(self, first: str | int, second: str | int | None = ..., /) -> None:
        """Lower an item TAGORID given in ARGS
        (optional below another item).
        """

    def lower(self, first: str | int, second: str | int | None = ..., /) -> None:  # type: ignore[override]
        """Lower an item TAGORID given in ARGS
        (optional below another item).
        """

    def tag_raise(self, first: str | int, second: str | int | None = ..., /) -> None:
        """Raise an item TAGORID given in ARGS
        (optional above another item).
        """

    def tkraise(self, first: str | int, second: str | int | None = ..., /) -> None:  # type: ignore[override]
        """Raise an item TAGORID given in ARGS
        (optional above another item).
        """

    def lift(self, first: str | int, second: str | int | None = ..., /) -> None:  # type: ignore[override]
        """Raise an item TAGORID given in ARGS
        (optional above another item).
        """

    def scale(self, tagOrId: str | int, xOrigin: float | str, yOrigin: float | str, xScale: float, yScale: float, /) -> None:
        """Scale item TAGORID with XORIGIN, YORIGIN, XSCALE, YSCALE."""

    def scan_mark(self, x, y) -> None:
        """Remember the current X, Y coordinates."""

    def scan_dragto(self, x, y, gain: int = 10) -> None:
        """Adjust the view of the canvas to GAIN times the
        difference between X and Y and the coordinates given in
        scan_mark.
        """

    def select_adjust(self, tagOrId, index) -> None:
        """Adjust the end of the selection near the cursor of an item TAGORID to index."""

    def select_clear(self) -> None:
        """Clear the selection if it is in this widget."""

    def select_from(self, tagOrId, index) -> None:
        """Set the fixed end of a selection in item TAGORID to INDEX."""

    def select_item(self):
        """Return the item which has the selection."""

    def select_to(self, tagOrId, index) -> None:
        """Set the variable end of a selection in item TAGORID to INDEX."""

    def type(self, tagOrId: str | int) -> int | None:
        """Return the type of the item TAGORID."""

class Checkbutton(Widget):
    """Checkbutton widget which is either in on- or off-state."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = "center",
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = "",
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = "",
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = "none",
        cursor: _Cursor = "",
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 1,
        image: _Image | str = "",
        indicatoron: bool = True,
        justify: Literal["left", "center", "right"] = "center",
        name: str = ...,
        offrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        # The checkbutton puts a value to its variable when it's checked or
        # unchecked. We don't restrict the type of that value here, so
        # Any-typing is fine.
        #
        # I think Checkbutton shouldn't be generic, because then specifying
        # "any checkbutton regardless of what variable it uses" would be
        # difficult, and we might run into issues just like how list[float]
        # and list[int] are incompatible. Also, we would need a way to
        # specify "Checkbutton not associated with any variable", which is
        # done by setting variable to empty string (the default).
        offvalue: Any = 0,
        onvalue: Any = 1,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = "",
        padx: float | str = 1,
        pady: float | str = 1,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        selectcolor: str = ...,
        selectimage: _Image | str = "",
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        text: float | str = "",
        textvariable: Variable = ...,
        tristateimage: _Image | str = "",
        tristatevalue: Any = "",
        underline: int = -1,
        variable: Variable | Literal[""] = ...,
        width: float | str = 0,
        wraplength: float | str = 0,
    ) -> None:
        """Construct a checkbutton widget with the parent MASTER.

        Valid resource names: activebackground, activeforeground, anchor,
        background, bd, bg, bitmap, borderwidth, command, cursor,
        disabledforeground, fg, font, foreground, height,
        highlightbackground, highlightcolor, highlightthickness, image,
        indicatoron, justify, offvalue, onvalue, padx, pady, relief,
        selectcolor, selectimage, state, takefocus, text, textvariable,
        underline, variable, width, wraplength.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: _Cursor = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        justify: Literal["left", "center", "right"] = ...,
        offrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        offvalue: Any = ...,
        onvalue: Any = ...,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        tristateimage: _Image | str = ...,
        tristatevalue: Any = ...,
        underline: int = ...,
        variable: Variable | Literal[""] = ...,
        width: float | str = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def deselect(self) -> None:
        """Put the button in off-state."""

    def flash(self) -> None:
        """Flash the button."""

    def invoke(self) -> Any:
        """Toggle the button and invoke a command if given as resource."""

    def select(self) -> None:
        """Put the button in on-state."""

    def toggle(self) -> None:
        """Toggle the button."""

class Entry(Widget, XView):
    """Entry widget which allows displaying simple text."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = "xterm",
        disabledbackground: str = ...,
        disabledforeground: str = ...,
        exportselection: bool = True,
        fg: str = ...,
        font: _FontDescription = "TkTextFont",
        foreground: str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = 0,
        insertofftime: int = 300,
        insertontime: int = 600,
        insertwidth: float | str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        invcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",  # same as invalidcommand
        justify: Literal["left", "center", "right"] = "left",
        name: str = ...,
        readonlybackground: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "sunken",
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        show: str = "",
        state: Literal["normal", "disabled", "readonly"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        textvariable: Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = "none",
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        vcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",  # same as validatecommand
        width: int = 20,
        xscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct an entry widget with the parent MASTER.

        Valid resource names: background, bd, bg, borderwidth, cursor,
        exportselection, fg, font, foreground, highlightbackground,
        highlightcolor, highlightthickness, insertbackground,
        insertborderwidth, insertofftime, insertontime, insertwidth,
        invalidcommand, invcmd, justify, relief, selectbackground,
        selectborderwidth, selectforeground, show, state, takefocus,
        textvariable, validate, validatecommand, vcmd, width,
        xscrollcommand.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        disabledbackground: str = ...,
        disabledforeground: str = ...,
        exportselection: bool = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = ...,
        insertofftime: int = ...,
        insertontime: int = ...,
        insertwidth: float | str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        invcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        readonlybackground: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        show: str = ...,
        state: Literal["normal", "disabled", "readonly"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        vcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def delete(self, first: str | int, last: str | int | None = None) -> None:
        """Delete text from FIRST to LAST (not included)."""

    def get(self) -> str:
        """Return the text."""

    def icursor(self, index: str | int) -> None:
        """Insert cursor at INDEX."""

    def index(self, index: str | int) -> int:
        """Return position of cursor."""

    def insert(self, index: str | int, string: str) -> None:
        """Insert STRING at INDEX."""

    def scan_mark(self, x) -> None:
        """Remember the current X, Y coordinates."""

    def scan_dragto(self, x) -> None:
        """Adjust the view of the canvas to 10 times the
        difference between X and Y and the coordinates given in
        scan_mark.
        """

    def selection_adjust(self, index: str | int) -> None:
        """Adjust the end of the selection near the cursor to INDEX."""

    def selection_clear(self) -> None:  # type: ignore[override]
        """Clear the selection if it is in this widget."""

    def selection_from(self, index: str | int) -> None:
        """Set the fixed end of a selection to INDEX."""

    def selection_present(self) -> bool:
        """Return True if there are characters selected in the entry, False
        otherwise.
        """

    def selection_range(self, start: str | int, end: str | int) -> None:
        """Set the selection from START to END (not included)."""

    def selection_to(self, index: str | int) -> None:
        """Set the variable end of a selection to INDEX."""
    select_adjust = selection_adjust
    select_clear = selection_clear
    select_from = selection_from
    select_present = selection_present
    select_range = selection_range
    select_to = selection_to

class Frame(Widget):
    """Frame widget which may contain other widgets and can have a 3D border."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = 0,
        bg: str = ...,
        border: float | str = 0,
        borderwidth: float | str = 0,
        class_: str = "Frame",  # can't be changed with configure()
        colormap: Literal["new", ""] | Misc = "",  # can't be changed with configure()
        container: bool = False,  # can't be changed with configure()
        cursor: _Cursor = "",
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        name: str = ...,
        padx: float | str = 0,
        pady: float | str = 0,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        visual: str | tuple[str, int] = "",  # can't be changed with configure()
        width: float | str = 0,
    ) -> None:
        """Construct a frame widget with the parent MASTER.

        Valid resource names: background, bd, bg, borderwidth, class,
        colormap, container, cursor, height, highlightbackground,
        highlightcolor, highlightthickness, relief, takefocus, visual, width.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Label(Widget):
    """Label widget which can display text and bitmaps."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = "center",
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = "",
        border: float | str = ...,
        borderwidth: float | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = "none",
        cursor: _Cursor = "",
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        image: _Image | str = "",
        justify: Literal["left", "center", "right"] = "center",
        name: str = ...,
        padx: float | str = 1,
        pady: float | str = 1,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        text: float | str = "",
        textvariable: Variable = ...,
        underline: int = -1,
        width: float | str = 0,
        wraplength: float | str = 0,
    ) -> None:
        """Construct a label widget with the parent MASTER.

        STANDARD OPTIONS

            activebackground, activeforeground, anchor,
            background, bitmap, borderwidth, cursor,
            disabledforeground, font, foreground,
            highlightbackground, highlightcolor,
            highlightthickness, image, justify,
            padx, pady, relief, takefocus, text,
            textvariable, underline, wraplength

        WIDGET-SPECIFIC OPTIONS

            height, state, width

        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: _Cursor = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        image: _Image | str = ...,
        justify: Literal["left", "center", "right"] = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        underline: int = ...,
        width: float | str = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Listbox(Widget, XView, YView):
    """Listbox widget which can display a list of strings."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activestyle: Literal["dotbox", "none", "underline"] = ...,
        background: str = ...,
        bd: float | str = 1,
        bg: str = ...,
        border: float | str = 1,
        borderwidth: float | str = 1,
        cursor: _Cursor = "",
        disabledforeground: str = ...,
        exportselection: bool | Literal[0, 1] = 1,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: int = 10,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        justify: Literal["left", "center", "right"] = "left",
        # There's no tkinter.ListVar, but seems like bare tkinter.Variable
        # actually works for this:
        #
        #    >>> import tkinter
        #    >>> lb = tkinter.Listbox()
        #    >>> var = lb['listvariable'] = tkinter.Variable()
        #    >>> var.set(['foo', 'bar', 'baz'])
        #    >>> lb.get(0, 'end')
        #    ('foo', 'bar', 'baz')
        listvariable: Variable = ...,
        name: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = 0,
        selectforeground: str = ...,
        # from listbox man page: "The value of the [selectmode] option may be
        # arbitrary, but the default bindings expect it to be either single,
        # browse, multiple, or extended"
        #
        # I have never seen anyone setting this to something else than what
        # "the default bindings expect", but let's support it anyway.
        selectmode: str | Literal["single", "browse", "multiple", "extended"] = "browse",  # noqa: Y051
        setgrid: bool = False,
        state: Literal["normal", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        width: int = 20,
        xscrollcommand: str | Callable[[float, float], object] = "",
        yscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct a listbox widget with the parent MASTER.

        Valid resource names: background, bd, bg, borderwidth, cursor,
        exportselection, fg, font, foreground, height, highlightbackground,
        highlightcolor, highlightthickness, relief, selectbackground,
        selectborderwidth, selectforeground, selectmode, setgrid, takefocus,
        width, xscrollcommand, yscrollcommand, listvariable.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activestyle: Literal["dotbox", "none", "underline"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        disabledforeground: str = ...,
        exportselection: bool = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: int = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        justify: Literal["left", "center", "right"] = ...,
        listvariable: Variable = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        selectmode: str | Literal["single", "browse", "multiple", "extended"] = ...,  # noqa: Y051
        setgrid: bool = ...,
        state: Literal["normal", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
        yscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def activate(self, index: str | int) -> None:
        """Activate item identified by INDEX."""

    def bbox(self, index: str | int) -> tuple[int, int, int, int] | None:  # type: ignore[override]
        """Return a tuple of X1,Y1,X2,Y2 coordinates for a rectangle
        which encloses the item identified by the given index.
        """

    def curselection(self):
        """Return the indices of currently selected item."""

    def delete(self, first: str | int, last: str | int | None = None) -> None:
        """Delete items from FIRST to LAST (included)."""

    def get(self, first: str | int, last: str | int | None = None):
        """Get list of items from FIRST to LAST (included)."""

    def index(self, index: str | int) -> int:
        """Return index of item identified with INDEX."""

    def insert(self, index: str | int, *elements: str | float) -> None:
        """Insert ELEMENTS at INDEX."""

    def nearest(self, y):
        """Get index of item which is nearest to y coordinate Y."""

    def scan_mark(self, x, y) -> None:
        """Remember the current X, Y coordinates."""

    def scan_dragto(self, x, y) -> None:
        """Adjust the view of the listbox to 10 times the
        difference between X and Y and the coordinates given in
        scan_mark.
        """

    def see(self, index: str | int) -> None:
        """Scroll such that INDEX is visible."""

    def selection_anchor(self, index: str | int) -> None:
        """Set the fixed end oft the selection to INDEX."""
    select_anchor = selection_anchor
    def selection_clear(self, first: str | int, last: str | int | None = None) -> None:  # type: ignore[override]
        """Clear the selection from FIRST to LAST (included)."""
    select_clear = selection_clear
    def selection_includes(self, index: str | int):
        """Return True if INDEX is part of the selection."""
    select_includes = selection_includes
    def selection_set(self, first: str | int, last: str | int | None = None) -> None:
        """Set the selection from FIRST to LAST (included) without
        changing the currently selected elements.
        """
    select_set = selection_set
    def size(self) -> int:  # type: ignore[override]
        """Return the number of elements in the listbox."""

    def itemcget(self, index: str | int, option):
        """Return the resource value for an ITEM and an OPTION."""

    def itemconfigure(self, index: str | int, cnf=None, **kw):
        """Configure resources of an ITEM.

        The values for resources are specified as keyword arguments.
        To get an overview about the allowed keyword arguments
        call the method without arguments.
        Valid resource names: background, bg, foreground, fg,
        selectbackground, selectforeground.
        """
    itemconfig = itemconfigure

class Menu(Widget):
    """Menu widget which allows displaying menu bars, pull-down menus and pop-up menus."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeborderwidth: float | str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = "arrow",
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        name: str = ...,
        postcommand: Callable[[], object] | str = "",
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectcolor: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        tearoff: bool | Literal[0, 1] = 1,
        # I guess tearoffcommand arguments are supposed to be widget objects,
        # but they are widget name strings. Use nametowidget() to handle the
        # arguments of tearoffcommand.
        tearoffcommand: Callable[[str, str], object] | str = "",
        title: str = "",
        type: Literal["menubar", "tearoff", "normal"] = "normal",
    ) -> None:
        """Construct menu widget with the parent MASTER.

        Valid resource names: activebackground, activeborderwidth,
        activeforeground, background, bd, bg, borderwidth, cursor,
        disabledforeground, fg, font, foreground, postcommand, relief,
        selectcolor, takefocus, tearoff, tearoffcommand, title, type.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeborderwidth: float | str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        postcommand: Callable[[], object] | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectcolor: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        tearoff: bool = ...,
        tearoffcommand: Callable[[str, str], object] | str = ...,
        title: str = ...,
        type: Literal["menubar", "tearoff", "normal"] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def tk_popup(self, x: int, y: int, entry: str | int = "") -> None:
        """Post the menu at position X,Y with entry ENTRY."""

    def activate(self, index: str | int) -> None:
        """Activate entry at INDEX."""

    def add(self, itemType, cnf={}, **kw):  # docstring says "Internal function."
        """Internal function."""

    def insert(self, index, itemType, cnf={}, **kw):  # docstring says "Internal function."
        """Internal function."""

    def add_cascade(
        self,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        label: str = ...,
        menu: Menu = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
    ) -> None:
        """Add hierarchical menu item."""

    def add_checkbutton(
        self,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        label: str = ...,
        offvalue: Any = ...,
        onvalue: Any = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
        variable: Variable = ...,
    ) -> None:
        """Add checkbutton menu item."""

    def add_command(
        self,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        label: str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
    ) -> None:
        """Add command menu item."""

    def add_radiobutton(
        self,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        label: str = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
        value: Any = ...,
        variable: Variable = ...,
    ) -> None:
        """Add radio menu item."""

    def add_separator(self, cnf: dict[str, Any] | None = {}, *, background: str = ...) -> None:
        """Add separator."""

    def insert_cascade(
        self,
        index: str | int,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        label: str = ...,
        menu: Menu = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
    ) -> None:
        """Add hierarchical menu item at INDEX."""

    def insert_checkbutton(
        self,
        index: str | int,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        label: str = ...,
        offvalue: Any = ...,
        onvalue: Any = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
        variable: Variable = ...,
    ) -> None:
        """Add checkbutton menu item at INDEX."""

    def insert_command(
        self,
        index: str | int,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        label: str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
    ) -> None:
        """Add command menu item at INDEX."""

    def insert_radiobutton(
        self,
        index: str | int,
        cnf: dict[str, Any] | None = {},
        *,
        accelerator: str = ...,
        activebackground: str = ...,
        activeforeground: str = ...,
        background: str = ...,
        bitmap: str = ...,
        columnbreak: int = ...,
        command: Callable[[], object] | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        hidemargin: bool = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        label: str = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        underline: int = ...,
        value: Any = ...,
        variable: Variable = ...,
    ) -> None:
        """Add radio menu item at INDEX."""

    def insert_separator(self, index: str | int, cnf: dict[str, Any] | None = {}, *, background: str = ...) -> None:
        """Add separator at INDEX."""

    def delete(self, index1: str | int, index2: str | int | None = None) -> None:
        """Delete menu items between INDEX1 and INDEX2 (included)."""

    def entrycget(self, index: str | int, option: str) -> Any:
        """Return the resource value of a menu item for OPTION at INDEX."""

    def entryconfigure(
        self, index: str | int, cnf: dict[str, Any] | None = None, **kw: Any
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure a menu item at INDEX."""
    entryconfig = entryconfigure
    def index(self, index: str | int) -> int | None:
        """Return the index of a menu item identified by INDEX."""

    def invoke(self, index: str | int) -> Any:
        """Invoke a menu item identified by INDEX and execute
        the associated command.
        """

    def post(self, x: int, y: int) -> None:
        """Display a menu at position X,Y."""

    def type(self, index: str | int) -> Literal["cascade", "checkbutton", "command", "radiobutton", "separator"]:
        """Return the type of the menu item at INDEX."""

    def unpost(self) -> None:
        """Unmap a menu."""

    def xposition(self, index: str | int) -> int:
        """Return the x-position of the leftmost pixel of the menu item
        at INDEX.
        """

    def yposition(self, index: str | int) -> int:
        """Return the y-position of the topmost pixel of the menu item at INDEX."""

class Menubutton(Widget):
    """Menubutton widget, obsolete since Tk8.0."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = "",
        border: float | str = ...,
        borderwidth: float | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = "none",
        cursor: _Cursor = "",
        direction: Literal["above", "below", "left", "right", "flush"] = "below",
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        image: _Image | str = "",
        indicatoron: bool = ...,
        justify: Literal["left", "center", "right"] = ...,
        menu: Menu = ...,
        name: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        text: float | str = "",
        textvariable: Variable = ...,
        underline: int = -1,
        width: float | str = 0,
        wraplength: float | str = 0,
    ) -> None: ...
    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: _Cursor = ...,
        direction: Literal["above", "below", "left", "right", "flush"] = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        justify: Literal["left", "center", "right"] = ...,
        menu: Menu = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        underline: int = ...,
        width: float | str = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Message(Widget):
    """Message widget to display multiline text. Obsolete since Label does it too."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = "center",
        aspect: int = 150,
        background: str = ...,
        bd: float | str = 1,
        bg: str = ...,
        border: float | str = 1,
        borderwidth: float | str = 1,
        cursor: _Cursor = "",
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        justify: Literal["left", "center", "right"] = "left",
        name: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        text: float | str = "",
        textvariable: Variable = ...,
        # there's width but no height
        width: float | str = 0,
    ) -> None: ...
    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        aspect: int = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        justify: Literal["left", "center", "right"] = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Radiobutton(Widget):
    """Radiobutton widget which shows only one of several buttons in on-state."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = "center",
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = "",
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = "",
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = "none",
        cursor: _Cursor = "",
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 1,
        image: _Image | str = "",
        indicatoron: bool = True,
        justify: Literal["left", "center", "right"] = "center",
        name: str = ...,
        offrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = "",
        padx: float | str = 1,
        pady: float | str = 1,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        selectcolor: str = ...,
        selectimage: _Image | str = "",
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        text: float | str = "",
        textvariable: Variable = ...,
        tristateimage: _Image | str = "",
        tristatevalue: Any = "",
        underline: int = -1,
        value: Any = "",
        variable: Variable | Literal[""] = ...,
        width: float | str = 0,
        wraplength: float | str = 0,
    ) -> None:
        """Construct a radiobutton widget with the parent MASTER.

        Valid resource names: activebackground, activeforeground, anchor,
        background, bd, bg, bitmap, borderwidth, command, cursor,
        disabledforeground, fg, font, foreground, height,
        highlightbackground, highlightcolor, highlightthickness, image,
        indicatoron, justify, padx, pady, relief, selectcolor, selectimage,
        state, takefocus, text, textvariable, underline, value, variable,
        width, wraplength.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activeforeground: str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bitmap: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[], Any] = ...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: _Cursor = ...,
        disabledforeground: str = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        image: _Image | str = ...,
        indicatoron: bool = ...,
        justify: Literal["left", "center", "right"] = ...,
        offrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        overrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove", ""] = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectcolor: str = ...,
        selectimage: _Image | str = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: Variable = ...,
        tristateimage: _Image | str = ...,
        tristatevalue: Any = ...,
        underline: int = ...,
        value: Any = ...,
        variable: Variable | Literal[""] = ...,
        width: float | str = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def deselect(self) -> None:
        """Put the button in off-state."""

    def flash(self) -> None:
        """Flash the button."""

    def invoke(self) -> Any:
        """Toggle the button and invoke a command if given as resource."""

    def select(self) -> None:
        """Put the button in on-state."""

class Scale(Widget):
    """Scale widget which can display a numerical scale."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        background: str = ...,
        bd: float | str = 1,
        bg: str = ...,
        bigincrement: float = 0.0,
        border: float | str = 1,
        borderwidth: float | str = 1,
        # don't know why the callback gets string instead of float
        command: str | Callable[[str], object] = "",
        cursor: _Cursor = "",
        digits: int = 0,
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        from_: float = 0.0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        label: str = "",
        length: float | str = 100,
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "vertical",
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        repeatdelay: int = 300,
        repeatinterval: int = 100,
        resolution: float = 1.0,
        showvalue: bool = True,
        sliderlength: float | str = 30,
        sliderrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "raised",
        state: Literal["normal", "active", "disabled"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        tickinterval: float = 0.0,
        to: float = 100.0,
        troughcolor: str = ...,
        variable: IntVar | DoubleVar = ...,
        width: float | str = 15,
    ) -> None:
        """Construct a scale widget with the parent MASTER.

        Valid resource names: activebackground, background, bigincrement, bd,
        bg, borderwidth, command, cursor, digits, fg, font, foreground, from,
        highlightbackground, highlightcolor, highlightthickness, label,
        length, orient, relief, repeatdelay, repeatinterval, resolution,
        showvalue, sliderlength, sliderrelief, state, takefocus,
        tickinterval, to, troughcolor, variable, width.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        bigincrement: float = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: str | Callable[[str], object] = ...,
        cursor: _Cursor = ...,
        digits: int = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        from_: float = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        label: str = ...,
        length: float | str = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = ...,
        repeatinterval: int = ...,
        resolution: float = ...,
        showvalue: bool = ...,
        sliderlength: float | str = ...,
        sliderrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        state: Literal["normal", "active", "disabled"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        tickinterval: float = ...,
        to: float = ...,
        troughcolor: str = ...,
        variable: IntVar | DoubleVar = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def get(self) -> float:
        """Get the current value as integer or float."""

    def set(self, value) -> None:
        """Set the value to VALUE."""

    def coords(self, value: float | None = None) -> tuple[int, int]:
        """Return a tuple (X,Y) of the point along the centerline of the
        trough that corresponds to VALUE or the current value if None is
        given.
        """

    def identify(self, x, y) -> Literal["", "slider", "trough1", "trough2"]:
        """Return where the point X,Y lies. Valid return values are "slider",
        "though1" and "though2".
        """

class Scrollbar(Widget):
    """Scrollbar widget which displays a slider at a certain position."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        activerelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "raised",
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        # There are many ways how the command may get called. Search for
        # 'SCROLLING COMMANDS' in scrollbar man page. There doesn't seem to
        # be any way to specify an overloaded callback function, so we say
        # that it can take any args while it can't in reality.
        command: Callable[..., tuple[float, float] | None] | str = "",
        cursor: _Cursor = "",
        elementborderwidth: float | str = -1,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        jump: bool = False,
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "vertical",
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = 300,
        repeatinterval: int = 100,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        troughcolor: str = ...,
        width: float | str = ...,
    ) -> None:
        """Construct a scrollbar widget with the parent MASTER.

        Valid resource names: activebackground, activerelief,
        background, bd, bg, borderwidth, command, cursor,
        elementborderwidth, highlightbackground,
        highlightcolor, highlightthickness, jump, orient,
        relief, repeatdelay, repeatinterval, takefocus,
        troughcolor, width.
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        activerelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        command: Callable[..., tuple[float, float] | None] | str = ...,
        cursor: _Cursor = ...,
        elementborderwidth: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        jump: bool = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = ...,
        repeatinterval: int = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        troughcolor: str = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def activate(self, index=None):
        """Marks the element indicated by index as active.
        The only index values understood by this method are "arrow1",
        "slider", or "arrow2".  If any other value is specified then no
        element of the scrollbar will be active.  If index is not specified,
        the method returns the name of the element that is currently active,
        or None if no element is active.
        """

    def delta(self, deltax: int, deltay: int) -> float:
        """Return the fractional change of the scrollbar setting if it
        would be moved by DELTAX or DELTAY pixels.
        """

    def fraction(self, x: int, y: int) -> float:
        """Return the fractional value which corresponds to a slider
        position of X,Y.
        """

    def identify(self, x: int, y: int) -> Literal["arrow1", "arrow2", "slider", "trough1", "trough2", ""]:
        """Return the element under position X,Y as one of
        "arrow1","slider","arrow2" or "".
        """

    def get(self) -> tuple[float, float, float, float] | tuple[float, float]:
        """Return the current fractional values (upper and lower end)
        of the slider position.
        """

    def set(self, first: float | str, last: float | str) -> None:
        """Set the fractional values of the slider position (upper and
        lower ends as value between 0 and 1).
        """

_WhatToCount: TypeAlias = Literal[
    "chars", "displaychars", "displayindices", "displaylines", "indices", "lines", "xpixels", "ypixels"
]

class Text(Widget, XView, YView):
    """Text widget which can display text in various forms."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        autoseparators: bool = True,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        blockcursor: bool = False,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = "xterm",
        endline: int | Literal[""] = "",
        exportselection: bool = True,
        fg: str = ...,
        font: _FontDescription = "TkFixedFont",
        foreground: str = ...,
        # width is always int, but height is allowed to be screen units.
        # This doesn't make any sense to me, and this isn't documented.
        # The docs seem to say that both should be integers.
        height: float | str = 24,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        inactiveselectbackground: str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = 0,
        insertofftime: int = 300,
        insertontime: int = 600,
        insertunfocussed: Literal["none", "hollow", "solid"] = "none",
        insertwidth: float | str = ...,
        maxundo: int = 0,
        name: str = ...,
        padx: float | str = 1,
        pady: float | str = 1,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        setgrid: bool = False,
        spacing1: float | str = 0,
        spacing2: float | str = 0,
        spacing3: float | str = 0,
        startline: int | Literal[""] = "",
        state: Literal["normal", "disabled"] = "normal",
        # Literal inside Tuple doesn't actually work
        tabs: float | str | tuple[float | str, ...] = "",
        tabstyle: Literal["tabular", "wordprocessor"] = "tabular",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        undo: bool = False,
        width: int = 80,
        wrap: Literal["none", "char", "word"] = "char",
        xscrollcommand: str | Callable[[float, float], object] = "",
        yscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct a text widget with the parent MASTER.

        STANDARD OPTIONS

            background, borderwidth, cursor,
            exportselection, font, foreground,
            highlightbackground, highlightcolor,
            highlightthickness, insertbackground,
            insertborderwidth, insertofftime,
            insertontime, insertwidth, padx, pady,
            relief, selectbackground,
            selectborderwidth, selectforeground,
            setgrid, takefocus,
            xscrollcommand, yscrollcommand,

        WIDGET-SPECIFIC OPTIONS

            autoseparators, height, maxundo,
            spacing1, spacing2, spacing3,
            state, tabs, undo, width, wrap,

        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        autoseparators: bool = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        blockcursor: bool = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        endline: int | Literal[""] = ...,
        exportselection: bool = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        inactiveselectbackground: str = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = ...,
        insertofftime: int = ...,
        insertontime: int = ...,
        insertunfocussed: Literal["none", "hollow", "solid"] = ...,
        insertwidth: float | str = ...,
        maxundo: int = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        setgrid: bool = ...,
        spacing1: float | str = ...,
        spacing2: float | str = ...,
        spacing3: float | str = ...,
        startline: int | Literal[""] = ...,
        state: Literal["normal", "disabled"] = ...,
        tabs: float | str | tuple[float | str, ...] = ...,
        tabstyle: Literal["tabular", "wordprocessor"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        undo: bool = ...,
        width: int = ...,
        wrap: Literal["none", "char", "word"] = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
        yscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def bbox(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> tuple[int, int, int, int] | None:  # type: ignore[override]
        """Return a tuple of (x,y,width,height) which gives the bounding
        box of the visible part of the character at the given index.
        """

    def compare(
        self,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        op: Literal["<", "<=", "==", ">=", ">", "!="],
        index2: str | float | _tkinter.Tcl_Obj | Widget,
    ) -> bool:
        """Return whether between index INDEX1 and index INDEX2 the
        relation OP is satisfied. OP is one of <, <=, ==, >=, >, or !=.
        """
    if sys.version_info >= (3, 13):
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            *,
            return_ints: Literal[True],
        ) -> int:
            """Counts the number of relevant things between the two indices.

            If INDEX1 is after INDEX2, the result will be a negative number
            (and this holds for each of the possible options).

            The actual items which are counted depends on the options given.
            The result is a tuple of integers, one for the result of each
            counting option given, if more than one option is specified or
            return_ints is false (default), otherwise it is an integer.
            Valid counting options are "chars", "displaychars",
            "displayindices", "displaylines", "indices", "lines", "xpixels"
            and "ypixels". The default value, if no option is specified, is
            "indices". There is an additional possible option "update",
            which if given then all subsequent options ensure that any
            possible out of date information is recalculated.
            """

        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg: _WhatToCount | Literal["update"],
            /,
            *,
            return_ints: Literal[True],
        ) -> int: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: Literal["update"],
            arg2: _WhatToCount,
            /,
            *,
            return_ints: Literal[True],
        ) -> int: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: Literal["update"],
            /,
            *,
            return_ints: Literal[True],
        ) -> int: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: _WhatToCount,
            /,
            *,
            return_ints: Literal[True],
        ) -> tuple[int, int]: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount | Literal["update"],
            arg2: _WhatToCount | Literal["update"],
            arg3: _WhatToCount | Literal["update"],
            /,
            *args: _WhatToCount | Literal["update"],
            return_ints: Literal[True],
        ) -> tuple[int, ...]: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            *,
            return_ints: Literal[False] = False,
        ) -> tuple[int] | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg: _WhatToCount | Literal["update"],
            /,
            *,
            return_ints: Literal[False] = False,
        ) -> tuple[int] | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: Literal["update"],
            arg2: _WhatToCount,
            /,
            *,
            return_ints: Literal[False] = False,
        ) -> int | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: Literal["update"],
            /,
            *,
            return_ints: Literal[False] = False,
        ) -> int | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: _WhatToCount,
            /,
            *,
            return_ints: Literal[False] = False,
        ) -> tuple[int, int]: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount | Literal["update"],
            arg2: _WhatToCount | Literal["update"],
            arg3: _WhatToCount | Literal["update"],
            /,
            *args: _WhatToCount | Literal["update"],
            return_ints: Literal[False] = False,
        ) -> tuple[int, ...]: ...
    else:
        @overload
        def count(
            self, index1: str | float | _tkinter.Tcl_Obj | Widget, index2: str | float | _tkinter.Tcl_Obj | Widget
        ) -> tuple[int] | None:
            """Counts the number of relevant things between the two indices.
            If index1 is after index2, the result will be a negative number
            (and this holds for each of the possible options).

            The actual items which are counted depends on the options given by
            args. The result is a list of integers, one for the result of each
            counting option given. Valid counting options are "chars",
            "displaychars", "displayindices", "displaylines", "indices",
            "lines", "xpixels" and "ypixels". There is an additional possible
            option "update", which if given then all subsequent options ensure
            that any possible out of date information is recalculated.
            """

        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg: _WhatToCount | Literal["update"],
            /,
        ) -> tuple[int] | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: Literal["update"],
            arg2: _WhatToCount,
            /,
        ) -> int | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: Literal["update"],
            /,
        ) -> int | None: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount,
            arg2: _WhatToCount,
            /,
        ) -> tuple[int, int]: ...
        @overload
        def count(
            self,
            index1: str | float | _tkinter.Tcl_Obj | Widget,
            index2: str | float | _tkinter.Tcl_Obj | Widget,
            arg1: _WhatToCount | Literal["update"],
            arg2: _WhatToCount | Literal["update"],
            arg3: _WhatToCount | Literal["update"],
            /,
            *args: _WhatToCount | Literal["update"],
        ) -> tuple[int, ...]: ...

    @overload
    def debug(self, boolean: None = None) -> bool:
        """Turn on the internal consistency checks of the B-Tree inside the text
        widget according to BOOLEAN.
        """

    @overload
    def debug(self, boolean: bool) -> None: ...
    def delete(
        self, index1: str | float | _tkinter.Tcl_Obj | Widget, index2: str | float | _tkinter.Tcl_Obj | Widget | None = None
    ) -> None:
        """Delete the characters between INDEX1 and INDEX2 (not included)."""

    def dlineinfo(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> tuple[int, int, int, int, int] | None:
        """Return tuple (x,y,width,height,baseline) giving the bounding box
        and baseline position of the visible part of the line containing
        the character at INDEX.
        """

    @overload
    def dump(
        self,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None = None,
        command: None = None,
        *,
        all: bool = ...,
        image: bool = ...,
        mark: bool = ...,
        tag: bool = ...,
        text: bool = ...,
        window: bool = ...,
    ) -> list[tuple[str, str, str]]:
        """Return the contents of the widget between index1 and index2.

        The type of contents returned in filtered based on the keyword
        parameters; if 'all', 'image', 'mark', 'tag', 'text', or 'window' are
        given and true, then the corresponding items are returned. The result
        is a list of triples of the form (key, value, index). If none of the
        keywords are true then 'all' is used by default.

        If the 'command' argument is given, it is called once for each element
        of the list of triples, with the values of each triple serving as the
        arguments to the function. In this case the list is not returned.
        """

    @overload
    def dump(
        self,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None,
        command: Callable[[str, str, str], object] | str,
        *,
        all: bool = ...,
        image: bool = ...,
        mark: bool = ...,
        tag: bool = ...,
        text: bool = ...,
        window: bool = ...,
    ) -> None: ...
    @overload
    def dump(
        self,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None = None,
        *,
        command: Callable[[str, str, str], object] | str,
        all: bool = ...,
        image: bool = ...,
        mark: bool = ...,
        tag: bool = ...,
        text: bool = ...,
        window: bool = ...,
    ) -> None: ...
    def edit(self, *args):  # docstring says "Internal method"
        """Internal method

        This method controls the undo mechanism and
        the modified flag. The exact behavior of the
        command depends on the option argument that
        follows the edit argument. The following forms
        of the command are currently supported:

        edit_modified, edit_redo, edit_reset, edit_separator
        and edit_undo

        """

    @overload
    def edit_modified(self, arg: None = None) -> bool:  # actually returns Literal[0, 1]
        """Get or Set the modified flag

        If arg is not specified, returns the modified
        flag of the widget. The insert, delete, edit undo and
        edit redo commands or the user can set or clear the
        modified flag. If boolean is specified, sets the
        modified flag of the widget to arg.
        """

    @overload
    def edit_modified(self, arg: bool) -> None: ...  # actually returns empty string
    def edit_redo(self) -> None:  # actually returns empty string
        """Redo the last undone edit

        When the undo option is true, reapplies the last
        undone edits provided no other edits were done since
        then. Generates an error when the redo stack is empty.
        Does nothing when the undo option is false.
        """

    def edit_reset(self) -> None:  # actually returns empty string
        """Clears the undo and redo stacks"""

    def edit_separator(self) -> None:  # actually returns empty string
        """Inserts a separator (boundary) on the undo stack.

        Does nothing when the undo option is false
        """

    def edit_undo(self) -> None:  # actually returns empty string
        """Undoes the last edit action

        If the undo option is true. An edit action is defined
        as all the insert and delete commands that are recorded
        on the undo stack in between two separators. Generates
        an error when the undo stack is empty. Does nothing
        when the undo option is false
        """

    def get(
        self, index1: str | float | _tkinter.Tcl_Obj | Widget, index2: str | float | _tkinter.Tcl_Obj | Widget | None = None
    ) -> str:
        """Return the text from INDEX1 to INDEX2 (not included)."""

    @overload
    def image_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["image", "name"]) -> str:
        """Return the value of OPTION of an embedded image at INDEX."""

    @overload
    def image_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["padx", "pady"]) -> int: ...
    @overload
    def image_cget(
        self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["align"]
    ) -> Literal["baseline", "bottom", "center", "top"]: ...
    @overload
    def image_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: str) -> Any: ...
    @overload
    def image_configure(self, index: str | float | _tkinter.Tcl_Obj | Widget, cnf: str) -> tuple[str, str, str, str, str | int]:
        """Configure an embedded image at INDEX."""

    @overload
    def image_configure(
        self,
        index: str | float | _tkinter.Tcl_Obj | Widget,
        cnf: dict[str, Any] | None = None,
        *,
        align: Literal["baseline", "bottom", "center", "top"] = ...,
        image: _Image | str = ...,
        name: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
    ) -> dict[str, tuple[str, str, str, str, str | int]] | None: ...
    def image_create(
        self,
        index: str | float | _tkinter.Tcl_Obj | Widget,
        cnf: dict[str, Any] | None = {},
        *,
        align: Literal["baseline", "bottom", "center", "top"] = ...,
        image: _Image | str = ...,
        name: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
    ) -> str:
        """Create an embedded image at INDEX."""

    def image_names(self) -> tuple[str, ...]:
        """Return all names of embedded images in this widget."""

    def index(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> str:
        """Return the index in the form line.char for INDEX."""

    def insert(
        self, index: str | float | _tkinter.Tcl_Obj | Widget, chars: str, *args: str | list[str] | tuple[str, ...]
    ) -> None:
        """Insert CHARS before the characters at INDEX. An additional
        tag can be given in ARGS. Additional CHARS and tags can follow in ARGS.
        """

    @overload
    def mark_gravity(self, markName: str, direction: None = None) -> Literal["left", "right"]:
        """Change the gravity of a mark MARKNAME to DIRECTION (LEFT or RIGHT).
        Return the current value if None is given for DIRECTION.
        """

    @overload
    def mark_gravity(self, markName: str, direction: Literal["left", "right"]) -> None: ...  # actually returns empty string
    def mark_names(self) -> tuple[str, ...]:
        """Return all mark names."""

    def mark_set(self, markName: str, index: str | float | _tkinter.Tcl_Obj | Widget) -> None:
        """Set mark MARKNAME before the character at INDEX."""

    def mark_unset(self, *markNames: str) -> None:
        """Delete all marks in MARKNAMES."""

    def mark_next(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> str | None:
        """Return the name of the next mark after INDEX."""

    def mark_previous(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> str | None:
        """Return the name of the previous mark before INDEX."""
    # **kw of peer_create is same as the kwargs of Text.__init__
    def peer_create(self, newPathName: str | Text, cnf: dict[str, Any] = {}, **kw) -> None:
        """Creates a peer text widget with the given newPathName, and any
        optional standard configuration options. By default the peer will
        have the same start and end line as the parent widget, but
        these can be overridden with the standard configuration options.
        """

    def peer_names(self) -> tuple[_tkinter.Tcl_Obj, ...]:
        """Returns a list of peers of this widget (this does not include
        the widget itself).
        """

    def replace(
        self,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget,
        chars: str,
        *args: str | list[str] | tuple[str, ...],
    ) -> None:
        """Replaces the range of characters between index1 and index2 with
        the given characters and tags specified by args.

        See the method insert for some more information about args, and the
        method delete for information about the indices.
        """

    def scan_mark(self, x: int, y: int) -> None:
        """Remember the current X, Y coordinates."""

    def scan_dragto(self, x: int, y: int) -> None:
        """Adjust the view of the text to 10 times the
        difference between X and Y and the coordinates given in
        scan_mark.
        """

    def search(
        self,
        pattern: str,
        index: str | float | _tkinter.Tcl_Obj | Widget,
        stopindex: str | float | _tkinter.Tcl_Obj | Widget | None = None,
        forwards: bool | None = None,
        backwards: bool | None = None,
        exact: bool | None = None,
        regexp: bool | None = None,
        nocase: bool | None = None,
        count: Variable | None = None,
        elide: bool | None = None,
    ) -> str:  # returns empty string for not found
        """Search PATTERN beginning from INDEX until STOPINDEX.
        Return the index of the first character of a match or an
        empty string.
        """

    def see(self, index: str | float | _tkinter.Tcl_Obj | Widget) -> None:
        """Scroll such that the character at INDEX is visible."""

    def tag_add(
        self, tagName: str, index1: str | float | _tkinter.Tcl_Obj | Widget, *args: str | float | _tkinter.Tcl_Obj | Widget
    ) -> None:
        """Add tag TAGNAME to all characters between INDEX1 and index2 in ARGS.
        Additional pairs of indices may follow in ARGS.
        """
    # tag_bind stuff is very similar to Canvas
    @overload
    def tag_bind(
        self,
        tagName: str,
        sequence: str | None,
        func: Callable[[Event[Text]], object] | None,
        add: Literal["", "+"] | bool | None = None,
    ) -> str:
        """Bind to all characters with TAGNAME at event SEQUENCE a call to function FUNC.

        An additional boolean parameter ADD specifies whether FUNC will be
        called additionally to the other bound function or whether it will
        replace the previous function. See bind for the return value.
        """

    @overload
    def tag_bind(self, tagName: str, sequence: str | None, func: str, add: Literal["", "+"] | bool | None = None) -> None: ...
    def tag_unbind(self, tagName: str, sequence: str, funcid: str | None = None) -> None:
        """Unbind for all characters with TAGNAME for event SEQUENCE  the
        function identified with FUNCID.
        """
    # allowing any string for cget instead of just Literals because there's no other way to look up tag options
    def tag_cget(self, tagName: str, option: str):
        """Return the value of OPTION for tag TAGNAME."""

    @overload
    def tag_configure(
        self,
        tagName: str,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bgstipple: str = ...,
        borderwidth: float | str = ...,
        border: float | str = ...,  # alias for borderwidth
        elide: bool = ...,
        fgstipple: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        justify: Literal["left", "right", "center"] = ...,
        lmargin1: float | str = ...,
        lmargin2: float | str = ...,
        lmargincolor: str = ...,
        offset: float | str = ...,
        overstrike: bool = ...,
        overstrikefg: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        rmargin: float | str = ...,
        rmargincolor: str = ...,
        selectbackground: str = ...,
        selectforeground: str = ...,
        spacing1: float | str = ...,
        spacing2: float | str = ...,
        spacing3: float | str = ...,
        tabs: Any = ...,  # the exact type is kind of complicated, see manual page
        tabstyle: Literal["tabular", "wordprocessor"] = ...,
        underline: bool = ...,
        underlinefg: str = ...,
        wrap: Literal["none", "char", "word"] = ...,  # be careful with "none" vs None
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure a tag TAGNAME."""

    @overload
    def tag_configure(self, tagName: str, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    tag_config = tag_configure
    def tag_delete(self, first_tag_name: str, /, *tagNames: str) -> None:  # error if no tag names given
        """Delete all tags in TAGNAMES."""

    def tag_lower(self, tagName: str, belowThis: str | None = None) -> None:
        """Change the priority of tag TAGNAME such that it is lower
        than the priority of BELOWTHIS.
        """

    def tag_names(self, index: str | float | _tkinter.Tcl_Obj | Widget | None = None) -> tuple[str, ...]:
        """Return a list of all tag names."""

    def tag_nextrange(
        self,
        tagName: str,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None = None,
    ) -> tuple[str, str] | tuple[()]:
        """Return a list of start and end index for the first sequence of
        characters between INDEX1 and INDEX2 which all have tag TAGNAME.
        The text is searched forward from INDEX1.
        """

    def tag_prevrange(
        self,
        tagName: str,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None = None,
    ) -> tuple[str, str] | tuple[()]:
        """Return a list of start and end index for the first sequence of
        characters between INDEX1 and INDEX2 which all have tag TAGNAME.
        The text is searched backwards from INDEX1.
        """

    def tag_raise(self, tagName: str, aboveThis: str | None = None) -> None:
        """Change the priority of tag TAGNAME such that it is higher
        than the priority of ABOVETHIS.
        """

    def tag_ranges(self, tagName: str) -> tuple[_tkinter.Tcl_Obj, ...]:
        """Return a list of ranges of text which have tag TAGNAME."""
    # tag_remove and tag_delete are different
    def tag_remove(
        self,
        tagName: str,
        index1: str | float | _tkinter.Tcl_Obj | Widget,
        index2: str | float | _tkinter.Tcl_Obj | Widget | None = None,
    ) -> None:
        """Remove tag TAGNAME from all characters between INDEX1 and INDEX2."""

    @overload
    def window_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["padx", "pady"]) -> int:
        """Return the value of OPTION of an embedded window at INDEX."""

    @overload
    def window_cget(
        self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["stretch"]
    ) -> bool: ...  # actually returns Literal[0, 1]
    @overload
    def window_cget(
        self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["align"]
    ) -> Literal["baseline", "bottom", "center", "top"]: ...
    @overload  # window is set to a widget, but read as the string name.
    def window_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: Literal["create", "window"]) -> str: ...
    @overload
    def window_cget(self, index: str | float | _tkinter.Tcl_Obj | Widget, option: str) -> Any: ...
    @overload
    def window_configure(self, index: str | float | _tkinter.Tcl_Obj | Widget, cnf: str) -> tuple[str, str, str, str, str | int]:
        """Configure an embedded window at INDEX."""

    @overload
    def window_configure(
        self,
        index: str | float | _tkinter.Tcl_Obj | Widget,
        cnf: dict[str, Any] | None = None,
        *,
        align: Literal["baseline", "bottom", "center", "top"] = ...,
        create: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        stretch: bool | Literal[0, 1] = ...,
        window: Misc | str = ...,
    ) -> dict[str, tuple[str, str, str, str, str | int]] | None: ...
    window_config = window_configure
    def window_create(
        self,
        index: str | float | _tkinter.Tcl_Obj | Widget,
        cnf: dict[str, Any] | None = {},
        *,
        align: Literal["baseline", "bottom", "center", "top"] = ...,
        create: str = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        stretch: bool | Literal[0, 1] = ...,
        window: Misc | str = ...,
    ) -> None:
        """Create a window at INDEX."""

    def window_names(self) -> tuple[str, ...]:
        """Return all names of embedded windows in this widget."""

    def yview_pickplace(self, *what):  # deprecated
        """Obsolete function, use see."""

class _setit:
    """Internal class. It wraps the command in the widget OptionMenu."""

    def __init__(self, var, value, callback=None) -> None: ...
    def __call__(self, *args) -> None: ...

# manual page: tk_optionMenu
class OptionMenu(Menubutton):
    """OptionMenu which allows the user to select a value from a menu."""

    menuname: Incomplete
    def __init__(
        # differs from other widgets
        self,
        master: Misc | None,
        variable: StringVar,
        value: str,
        *values: str,
        # kwarg only from now on
        command: Callable[[StringVar], object] | None = ...,
    ) -> None:
        """Construct an optionmenu widget with the parent MASTER, with
        the resource textvariable set to VARIABLE, the initially selected
        value VALUE, the other menu values VALUES and an additional
        keyword argument command.
        """
    # configure, config, cget are inherited from Menubutton
    # destroy and __getitem__ are overridden, signature does not change

# This matches tkinter's image classes (PhotoImage and BitmapImage)
# and PIL's tkinter-compatible class (PIL.ImageTk.PhotoImage),
# but not a plain PIL image that isn't tkinter compatible.
# The reason is that PIL has width and height attributes, not methods.
@type_check_only
class _Image(Protocol):
    def width(self) -> int: ...
    def height(self) -> int: ...

@type_check_only
class _BitmapImageLike(_Image): ...

@type_check_only
class _PhotoImageLike(_Image): ...

class Image(_Image):
    """Base class for images."""

    name: Incomplete
    tk: _tkinter.TkappType
    def __init__(self, imgtype, name=None, cnf={}, master: Misc | _tkinter.TkappType | None = None, **kw) -> None: ...
    def __del__(self) -> None: ...
    def __setitem__(self, key, value) -> None: ...
    def __getitem__(self, key): ...
    configure: Incomplete
    config: Incomplete
    def type(self):
        """Return the type of the image, e.g. "photo" or "bitmap"."""

class PhotoImage(Image, _PhotoImageLike):
    """Widget which can display images in PGM, PPM, GIF, PNG format."""

    # This should be kept in sync with PIL.ImageTK.PhotoImage.__init__()
    def __init__(
        self,
        name: str | None = None,
        cnf: dict[str, Any] = {},
        master: Misc | _tkinter.TkappType | None = None,
        *,
        data: str | bytes = ...,  # not same as data argument of put()
        format: str = ...,
        file: StrOrBytesPath = ...,
        gamma: float = ...,
        height: int = ...,
        palette: int | str = ...,
        width: int = ...,
    ) -> None:
        """Create an image with NAME.

        Valid resource names: data, format, file, gamma, height, palette,
        width.
        """

    def configure(
        self,
        *,
        data: str | bytes = ...,
        format: str = ...,
        file: StrOrBytesPath = ...,
        gamma: float = ...,
        height: int = ...,
        palette: int | str = ...,
        width: int = ...,
    ) -> None:
        """Configure the image."""
    config = configure
    def blank(self) -> None:
        """Display a transparent image."""

    def cget(self, option: str) -> str:
        """Return the value of OPTION."""

    def __getitem__(self, key: str) -> str: ...  # always string: image['height'] can be '0'
    if sys.version_info >= (3, 13):
        def copy(
            self,
            *,
            from_coords: Iterable[int] | None = None,
            zoom: int | tuple[int, int] | list[int] | None = None,
            subsample: int | tuple[int, int] | list[int] | None = None,
        ) -> PhotoImage:
            """Return a new PhotoImage with the same image as this widget.

            The FROM_COORDS option specifies a rectangular sub-region of the
            source image to be copied. It must be a tuple or a list of 1 to 4
            integers (x1, y1, x2, y2).  (x1, y1) and (x2, y2) specify diagonally
            opposite corners of the rectangle.  If x2 and y2 are not specified,
            the default value is the bottom-right corner of the source image.
            The pixels copied will include the left and top edges of the
            specified rectangle but not the bottom or right edges.  If the
            FROM_COORDS option is not given, the default is the whole source
            image.

            If SUBSAMPLE or ZOOM are specified, the image is transformed as in
            the subsample() or zoom() methods.  The value must be a single
            integer or a pair of integers.
            """

        def subsample(self, x: int, y: Literal[""] = "", *, from_coords: Iterable[int] | None = None) -> PhotoImage:
            """Return a new PhotoImage based on the same image as this widget
            but use only every Xth or Yth pixel.  If Y is not given, the
            default value is the same as X.

            The FROM_COORDS option specifies a rectangular sub-region of the
            source image to be copied, as in the copy() method.
            """

        def zoom(self, x: int, y: Literal[""] = "", *, from_coords: Iterable[int] | None = None) -> PhotoImage:
            """Return a new PhotoImage with the same image as this widget
            but zoom it with a factor of X in the X direction and Y in the Y
            direction.  If Y is not given, the default value is the same as X.

            The FROM_COORDS option specifies a rectangular sub-region of the
            source image to be copied, as in the copy() method.
            """

        def copy_replace(
            self,
            sourceImage: PhotoImage | str,
            *,
            from_coords: Iterable[int] | None = None,
            to: Iterable[int] | None = None,
            shrink: bool = False,
            zoom: int | tuple[int, int] | list[int] | None = None,
            subsample: int | tuple[int, int] | list[int] | None = None,
            # `None` defaults to overlay.
            compositingrule: Literal["overlay", "set"] | None = None,
        ) -> None:
            """Copy a region from the source image (which must be a PhotoImage) to
            this image, possibly with pixel zooming and/or subsampling.  If no
            options are specified, this command copies the whole of the source
            image into this image, starting at coordinates (0, 0).

            The FROM_COORDS option specifies a rectangular sub-region of the
            source image to be copied. It must be a tuple or a list of 1 to 4
            integers (x1, y1, x2, y2).  (x1, y1) and (x2, y2) specify diagonally
            opposite corners of the rectangle.  If x2 and y2 are not specified,
            the default value is the bottom-right corner of the source image.
            The pixels copied will include the left and top edges of the
            specified rectangle but not the bottom or right edges.  If the
            FROM_COORDS option is not given, the default is the whole source
            image.

            The TO option specifies a rectangular sub-region of the destination
            image to be affected.  It must be a tuple or a list of 1 to 4
            integers (x1, y1, x2, y2).  (x1, y1) and (x2, y2) specify diagonally
            opposite corners of the rectangle.  If x2 and y2 are not specified,
            the default value is (x1,y1) plus the size of the source region
            (after subsampling and zooming, if specified).  If x2 and y2 are
            specified, the source region will be replicated if necessary to fill
            the destination region in a tiled fashion.

            If SHRINK is true, the size of the destination image should be
            reduced, if necessary, so that the region being copied into is at
            the bottom-right corner of the image.

            If SUBSAMPLE or ZOOM are specified, the image is transformed as in
            the subsample() or zoom() methods.  The value must be a single
            integer or a pair of integers.

            The COMPOSITINGRULE option specifies how transparent pixels in the
            source image are combined with the destination image.  When a
            compositing rule of 'overlay' is set, the old contents of the
            destination image are visible, as if the source image were printed
            on a piece of transparent film and placed over the top of the
            destination.  When a compositing rule of 'set' is set, the old
            contents of the destination image are discarded and the source image
            is used as-is.  The default compositing rule is 'overlay'.
            """
    else:
        def copy(self) -> PhotoImage:
            """Return a new PhotoImage with the same image as this widget."""

        def zoom(self, x: int, y: int | Literal[""] = "") -> PhotoImage:
            """Return a new PhotoImage with the same image as this widget
            but zoom it with a factor of x in the X direction and y in the Y
            direction.  If y is not given, the default value is the same as x.
            """

        def subsample(self, x: int, y: int | Literal[""] = "") -> PhotoImage:
            """Return a new PhotoImage based on the same image as this widget
            but use only every Xth or Yth pixel.  If y is not given, the
            default value is the same as x.
            """

    def get(self, x: int, y: int) -> tuple[int, int, int]:
        """Return the color (red, green, blue) of the pixel at X,Y."""

    def put(
        self,
        data: (
            str
            | bytes
            | list[str]
            | list[list[str]]
            | list[tuple[str, ...]]
            | tuple[str, ...]
            | tuple[list[str], ...]
            | tuple[tuple[str, ...], ...]
        ),
        to: tuple[int, int] | tuple[int, int, int, int] | None = None,
    ) -> None:
        """Put row formatted colors to image starting from
        position TO, e.g. image.put("{red green} {blue yellow}", to=(4,6))
        """
    if sys.version_info >= (3, 13):
        def read(
            self,
            filename: StrOrBytesPath,
            format: str | None = None,
            *,
            from_coords: Iterable[int] | None = None,
            to: Iterable[int] | None = None,
            shrink: bool = False,
        ) -> None:
            """Reads image data from the file named FILENAME into the image.

            The FORMAT option specifies the format of the image data in the
            file.

            The FROM_COORDS option specifies a rectangular sub-region of the image
            file data to be copied to the destination image.  It must be a tuple
            or a list of 1 to 4 integers (x1, y1, x2, y2).  (x1, y1) and
            (x2, y2) specify diagonally opposite corners of the rectangle.  If
            x2 and y2 are not specified, the default value is the bottom-right
            corner of the source image.  The default, if this option is not
            specified, is the whole of the image in the image file.

            The TO option specifies the coordinates of the top-left corner of
            the region of the image into which data from filename are to be
            read.  The default is (0, 0).

            If SHRINK is true, the size of the destination image will be
            reduced, if necessary, so that the region into which the image file
            data are read is at the bottom-right corner of the image.
            """

        def write(
            self,
            filename: StrOrBytesPath,
            format: str | None = None,
            from_coords: Iterable[int] | None = None,
            *,
            background: str | None = None,
            grayscale: bool = False,
        ) -> None:
            """Writes image data from the image to a file named FILENAME.

            The FORMAT option specifies the name of the image file format
            handler to be used to write the data to the file.  If this option
            is not given, the format is guessed from the file extension.

            The FROM_COORDS option specifies a rectangular region of the image
            to be written to the image file.  It must be a tuple or a list of 1
            to 4 integers (x1, y1, x2, y2).  If only x1 and y1 are specified,
            the region extends from (x1,y1) to the bottom-right corner of the
            image.  If all four coordinates are given, they specify diagonally
            opposite corners of the rectangular region.  The default, if this
            option is not given, is the whole image.

            If BACKGROUND is specified, the data will not contain any
            transparency information.  In all transparent pixels the color will
            be replaced by the specified color.

            If GRAYSCALE is true, the data will not contain color information.
            All pixel data will be transformed into grayscale.
            """

        @overload
        def data(
            self, format: str, *, from_coords: Iterable[int] | None = None, background: str | None = None, grayscale: bool = False
        ) -> bytes:
            """Returns image data.

            The FORMAT option specifies the name of the image file format
            handler to be used.  If this option is not given, this method uses
            a format that consists of a tuple (one element per row) of strings
            containing space-separated (one element per pixel/column) colors
            in “#RRGGBB” format (where RR is a pair of hexadecimal digits for
            the red channel, GG for green, and BB for blue).

            The FROM_COORDS option specifies a rectangular region of the image
            to be returned.  It must be a tuple or a list of 1 to 4 integers
            (x1, y1, x2, y2).  If only x1 and y1 are specified, the region
            extends from (x1,y1) to the bottom-right corner of the image.  If
            all four coordinates are given, they specify diagonally opposite
            corners of the rectangular region, including (x1, y1) and excluding
            (x2, y2).  The default, if this option is not given, is the whole
            image.

            If BACKGROUND is specified, the data will not contain any
            transparency information.  In all transparent pixels the color will
            be replaced by the specified color.

            If GRAYSCALE is true, the data will not contain color information.
            All pixel data will be transformed into grayscale.
            """

        @overload
        def data(
            self,
            format: None = None,
            *,
            from_coords: Iterable[int] | None = None,
            background: str | None = None,
            grayscale: bool = False,
        ) -> tuple[str, ...]: ...

    else:
        def write(self, filename: StrOrBytesPath, format: str | None = None, from_coords: tuple[int, int] | None = None) -> None:
            """Write image to file FILENAME in FORMAT starting from
            position FROM_COORDS.
            """

    def transparency_get(self, x: int, y: int) -> bool:
        """Return True if the pixel at x,y is transparent."""

    def transparency_set(self, x: int, y: int, boolean: bool) -> None:
        """Set the transparency of the pixel at x,y."""

class BitmapImage(Image, _BitmapImageLike):
    """Widget which can display images in XBM format."""

    # This should be kept in sync with PIL.ImageTK.BitmapImage.__init__()
    def __init__(
        self,
        name=None,
        cnf: dict[str, Any] = {},
        master: Misc | _tkinter.TkappType | None = None,
        *,
        background: str = ...,
        data: str | bytes = ...,
        file: StrOrBytesPath = ...,
        foreground: str = ...,
        maskdata: str = ...,
        maskfile: StrOrBytesPath = ...,
    ) -> None:
        """Create a bitmap with NAME.

        Valid resource names: background, data, file, foreground, maskdata, maskfile.
        """

def image_names() -> tuple[str, ...]: ...
def image_types() -> tuple[str, ...]: ...

class Spinbox(Widget, XView):
    """spinbox widget."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        activebackground: str = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        buttonbackground: str = ...,
        buttoncursor: _Cursor = "",
        buttondownrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        buttonuprelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        # percent substitutions don't seem to be supported, it's similar to Entry's validation stuff
        command: Callable[[], object] | str | list[str] | tuple[str, ...] = "",
        cursor: _Cursor = "xterm",
        disabledbackground: str = ...,
        disabledforeground: str = ...,
        exportselection: bool = True,
        fg: str = ...,
        font: _FontDescription = "TkTextFont",
        foreground: str = ...,
        format: str = "",
        from_: float = 0.0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        increment: float = 1.0,
        insertbackground: str = ...,
        insertborderwidth: float | str = 0,
        insertofftime: int = 300,
        insertontime: int = 600,
        insertwidth: float | str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        invcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        justify: Literal["left", "center", "right"] = "left",
        name: str = ...,
        readonlybackground: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "sunken",
        repeatdelay: int = 400,
        repeatinterval: int = 100,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        state: Literal["normal", "disabled", "readonly"] = "normal",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        textvariable: Variable = ...,
        to: float = 0.0,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = "none",
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        vcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        values: list[str] | tuple[str, ...] = ...,
        width: int = 20,
        wrap: bool = False,
        xscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct a spinbox widget with the parent MASTER.

        STANDARD OPTIONS

            activebackground, background, borderwidth,
            cursor, exportselection, font, foreground,
            highlightbackground, highlightcolor,
            highlightthickness, insertbackground,
            insertborderwidth, insertofftime,
            insertontime, insertwidth, justify, relief,
            repeatdelay, repeatinterval,
            selectbackground, selectborderwidth
            selectforeground, takefocus, textvariable
            xscrollcommand.

        WIDGET-SPECIFIC OPTIONS

            buttonbackground, buttoncursor,
            buttondownrelief, buttonuprelief,
            command, disabledbackground,
            disabledforeground, format, from,
            invalidcommand, increment,
            readonlybackground, state, to,
            validate, validatecommand values,
            width, wrap,
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        activebackground: str = ...,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        buttonbackground: str = ...,
        buttoncursor: _Cursor = ...,
        buttondownrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        buttonuprelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        command: Callable[[], object] | str | list[str] | tuple[str, ...] = ...,
        cursor: _Cursor = ...,
        disabledbackground: str = ...,
        disabledforeground: str = ...,
        exportselection: bool = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        format: str = ...,
        from_: float = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        increment: float = ...,
        insertbackground: str = ...,
        insertborderwidth: float | str = ...,
        insertofftime: int = ...,
        insertontime: int = ...,
        insertwidth: float | str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        invcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        readonlybackground: str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        repeatdelay: int = ...,
        repeatinterval: int = ...,
        selectbackground: str = ...,
        selectborderwidth: float | str = ...,
        selectforeground: str = ...,
        state: Literal["normal", "disabled", "readonly"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: Variable = ...,
        to: float = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        vcmd: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        values: list[str] | tuple[str, ...] = ...,
        width: int = ...,
        wrap: bool = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def bbox(self, index) -> tuple[int, int, int, int] | None:  # type: ignore[override]
        """Return a tuple of X1,Y1,X2,Y2 coordinates for a
        rectangle which encloses the character given by index.

        The first two elements of the list give the x and y
        coordinates of the upper-left corner of the screen
        area covered by the character (in pixels relative
        to the widget) and the last two elements give the
        width and height of the character, in pixels. The
        bounding box may refer to a region outside the
        visible area of the window.
        """

    def delete(self, first, last=None) -> Literal[""]:
        """Delete one or more elements of the spinbox.

        First is the index of the first character to delete,
        and last is the index of the character just after
        the last one to delete. If last isn't specified it
        defaults to first+1, i.e. a single character is
        deleted.  This command returns an empty string.
        """

    def get(self) -> str:
        """Returns the spinbox's string"""

    def icursor(self, index):
        """Alter the position of the insertion cursor.

        The insertion cursor will be displayed just before
        the character given by index. Returns an empty string
        """

    def identify(self, x: int, y: int) -> Literal["", "buttondown", "buttonup", "entry"]:
        """Returns the name of the widget at position x, y

        Return value is one of: none, buttondown, buttonup, entry
        """

    def index(self, index: str | int) -> int:
        """Returns the numerical index corresponding to index"""

    def insert(self, index: str | int, s: str) -> Literal[""]:
        """Insert string s at index

        Returns an empty string.
        """
    # spinbox.invoke("asdf") gives error mentioning .invoke("none"), but it's not documented
    def invoke(self, element: Literal["none", "buttonup", "buttondown"]) -> Literal[""]:
        """Causes the specified element to be invoked

        The element could be buttondown or buttonup
        triggering the action associated with it.
        """

    def scan(self, *args):
        """Internal function."""

    def scan_mark(self, x):
        """Records x and the current view in the spinbox window;

        used in conjunction with later scan dragto commands.
        Typically this command is associated with a mouse button
        press in the widget. It returns an empty string.
        """

    def scan_dragto(self, x):
        """Compute the difference between the given x argument
        and the x argument to the last scan mark command

        It then adjusts the view left or right by 10 times the
        difference in x-coordinates. This command is typically
        associated with mouse motion events in the widget, to
        produce the effect of dragging the spinbox at high speed
        through the window. The return value is an empty string.
        """

    def selection(self, *args) -> tuple[int, ...]:
        """Internal function."""

    def selection_adjust(self, index):
        """Locate the end of the selection nearest to the character
        given by index,

        Then adjust that end of the selection to be at index
        (i.e including but not going beyond index). The other
        end of the selection is made the anchor point for future
        select to commands. If the selection isn't currently in
        the spinbox, then a new selection is created to include
        the characters between index and the most recent selection
        anchor point, inclusive.
        """

    def selection_clear(self):  # type: ignore[override]
        """Clear the selection

        If the selection isn't in this widget then the
        command has no effect.
        """

    def selection_element(self, element=None):
        """Sets or gets the currently selected element.

        If a spinbutton element is specified, it will be
        displayed depressed.
        """

    def selection_from(self, index: int) -> None:
        """Set the fixed end of a selection to INDEX."""

    def selection_present(self) -> None:
        """Return True if there are characters selected in the spinbox, False
        otherwise.
        """

    def selection_range(self, start: int, end: int) -> None:
        """Set the selection from START to END (not included)."""

    def selection_to(self, index: int) -> None:
        """Set the variable end of a selection to INDEX."""

class LabelFrame(Widget):
    """labelframe widget."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = 2,
        bg: str = ...,
        border: float | str = 2,
        borderwidth: float | str = 2,
        class_: str = "Labelframe",  # can't be changed with configure()
        colormap: Literal["new", ""] | Misc = "",  # can't be changed with configure()
        container: bool = False,  # undocumented, can't be changed with configure()
        cursor: _Cursor = "",
        fg: str = ...,
        font: _FontDescription = "TkDefaultFont",
        foreground: str = ...,
        height: float | str = 0,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = 0,
        # 'ne' and 'en' are valid labelanchors, but only 'ne' is a valid _Anchor.
        labelanchor: Literal["nw", "n", "ne", "en", "e", "es", "se", "s", "sw", "ws", "w", "wn"] = "nw",
        labelwidget: Misc = ...,
        name: str = ...,
        padx: float | str = 0,
        pady: float | str = 0,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "groove",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = 0,
        text: float | str = "",
        visual: str | tuple[str, int] = "",  # can't be changed with configure()
        width: float | str = 0,
    ) -> None:
        """Construct a labelframe widget with the parent MASTER.

        STANDARD OPTIONS

            borderwidth, cursor, font, foreground,
            highlightbackground, highlightcolor,
            highlightthickness, padx, pady, relief,
            takefocus, text

        WIDGET-SPECIFIC OPTIONS

            background, class, colormap, container,
            height, labelanchor, labelwidget,
            visual, width
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        fg: str = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: float | str = ...,
        highlightbackground: str = ...,
        highlightcolor: str = ...,
        highlightthickness: float | str = ...,
        labelanchor: Literal["nw", "n", "ne", "en", "e", "es", "se", "s", "sw", "ws", "w", "wn"] = ...,
        labelwidget: Misc = ...,
        padx: float | str = ...,
        pady: float | str = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class PanedWindow(Widget):
    """panedwindow widget."""

    def __init__(
        self,
        master: Misc | None = None,
        cnf: dict[str, Any] | None = {},
        *,
        background: str = ...,
        bd: float | str = 1,
        bg: str = ...,
        border: float | str = 1,
        borderwidth: float | str = 1,
        cursor: _Cursor = "",
        handlepad: float | str = 8,
        handlesize: float | str = 8,
        height: float | str = "",
        name: str = ...,
        opaqueresize: bool = True,
        orient: Literal["horizontal", "vertical"] = "horizontal",
        proxybackground: str = "",
        proxyborderwidth: float | str = 2,
        proxyrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        sashcursor: _Cursor = "",
        sashpad: float | str = 0,
        sashrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = "flat",
        sashwidth: float | str = 3,
        showhandle: bool = False,
        width: float | str = "",
    ) -> None:
        """Construct a panedwindow widget with the parent MASTER.

        STANDARD OPTIONS

            background, borderwidth, cursor, height,
            orient, relief, width

        WIDGET-SPECIFIC OPTIONS

            handlepad, handlesize, opaqueresize,
            sashcursor, sashpad, sashrelief,
            sashwidth, showhandle,
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        bd: float | str = ...,
        bg: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: _Cursor = ...,
        handlepad: float | str = ...,
        handlesize: float | str = ...,
        height: float | str = ...,
        opaqueresize: bool = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        proxybackground: str = ...,
        proxyborderwidth: float | str = ...,
        proxyrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        sashcursor: _Cursor = ...,
        sashpad: float | str = ...,
        sashrelief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        sashwidth: float | str = ...,
        showhandle: bool = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Configure resources of a widget.

        The values for resources are specified as keyword
        arguments. To get an overview about
        the allowed keyword arguments call the method keys.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def add(self, child: Widget, **kw) -> None:
        """Add a child widget to the panedwindow in a new pane.

        The child argument is the name of the child widget
        followed by pairs of arguments that specify how to
        manage the windows. The possible options and values
        are the ones accepted by the paneconfigure method.
        """

    def remove(self, child) -> None:
        """Remove the pane containing child from the panedwindow

        All geometry management options for child will be forgotten.
        """
    forget = remove  # type: ignore[assignment]
    def identify(self, x: int, y: int):
        """Identify the panedwindow component at point x, y

        If the point is over a sash or a sash handle, the result
        is a two element list containing the index of the sash or
        handle, and a word indicating whether it is over a sash
        or a handle, such as {0 sash} or {2 handle}. If the point
        is over any other part of the panedwindow, the result is
        an empty list.
        """

    def proxy(self, *args) -> tuple[Incomplete, ...]:
        """Internal function."""

    def proxy_coord(self) -> tuple[Incomplete, ...]:
        """Return the x and y pair of the most recent proxy location"""

    def proxy_forget(self) -> tuple[Incomplete, ...]:
        """Remove the proxy from the display."""

    def proxy_place(self, x, y) -> tuple[Incomplete, ...]:
        """Place the proxy at the given x and y coordinates."""

    def sash(self, *args) -> tuple[Incomplete, ...]:
        """Internal function."""

    def sash_coord(self, index) -> tuple[Incomplete, ...]:
        """Return the current x and y pair for the sash given by index.

        Index must be an integer between 0 and 1 less than the
        number of panes in the panedwindow. The coordinates given are
        those of the top left corner of the region containing the sash.
        pathName sash dragto index x y This command computes the
        difference between the given coordinates and the coordinates
        given to the last sash coord command for the given sash. It then
        moves that sash the computed difference. The return value is the
        empty string.
        """

    def sash_mark(self, index) -> tuple[Incomplete, ...]:
        """Records x and y for the sash given by index;

        Used in conjunction with later dragto commands to move the sash.
        """

    def sash_place(self, index, x, y) -> tuple[Incomplete, ...]:
        """Place the sash given by index at the given coordinates"""

    def panecget(self, child, option):
        """Query a management option for window.

        Option may be any value allowed by the paneconfigure subcommand
        """

    def paneconfigure(self, tagOrId, cnf=None, **kw):
        """Query or modify the management options for window.

        If no option is specified, returns a list describing all
        of the available options for pathName.  If option is
        specified with no value, then the command returns a list
        describing the one named option (this list will be identical
        to the corresponding sublist of the value returned if no
        option is specified). If one or more option-value pairs are
        specified, then the command modifies the given widget
        option(s) to have the given value(s); in this case the
        command returns an empty string. The following options
        are supported:

        after window
            Insert the window after the window specified. window
            should be the name of a window already managed by pathName.
        before window
            Insert the window before the window specified. window
            should be the name of a window already managed by pathName.
        height size
            Specify a height for the window. The height will be the
            outer dimension of the window including its border, if
            any. If size is an empty string, or if -height is not
            specified, then the height requested internally by the
            window will be used initially; the height may later be
            adjusted by the movement of sashes in the panedwindow.
            Size may be any value accepted by Tk_GetPixels.
        minsize n
            Specifies that the size of the window cannot be made
            less than n. This constraint only affects the size of
            the widget in the paned dimension -- the x dimension
            for horizontal panedwindows, the y dimension for
            vertical panedwindows. May be any value accepted by
            Tk_GetPixels.
        padx n
            Specifies a non-negative value indicating how much
            extra space to leave on each side of the window in
            the X-direction. The value may have any of the forms
            accepted by Tk_GetPixels.
        pady n
            Specifies a non-negative value indicating how much
            extra space to leave on each side of the window in
            the Y-direction. The value may have any of the forms
            accepted by Tk_GetPixels.
        sticky style
            If a window's pane is larger than the requested
            dimensions of the window, this option may be used
            to position (or stretch) the window within its pane.
            Style is a string that contains zero or more of the
            characters n, s, e or w. The string can optionally
            contains spaces or commas, but they are ignored. Each
            letter refers to a side (north, south, east, or west)
            that the window will "stick" to. If both n and s
            (or e and w) are specified, the window will be
            stretched to fill the entire height (or width) of
            its cavity.
        width size
            Specify a width for the window. The width will be
            the outer dimension of the window including its
            border, if any. If size is an empty string, or
            if -width is not specified, then the width requested
            internally by the window will be used initially; the
            width may later be adjusted by the movement of sashes
            in the panedwindow. Size may be any value accepted by
            Tk_GetPixels.

        """
    paneconfig = paneconfigure
    def panes(self):
        """Returns an ordered list of the child panes."""

def _test() -> None: ...
