"""Ttk wrapper.

This module provides classes to allow using Tk themed widget set.

Ttk is based on a revised and enhanced version of
TIP #48 (http://tip.tcl.tk/48) specified style engine.

Its basic idea is to separate, to the extent possible, the code
implementing a widget's behavior from the code implementing its
appearance. Widget class bindings are primarily responsible for
maintaining the widget state and invoking callbacks, all aspects
of the widgets appearance lies at Themes.
"""

import _tkinter
import sys
import tkinter
from _typeshed import MaybeNone
from collections.abc import Callable, Iterable
from tkinter.font import _FontDescription
from typing import Any, Literal, TypedDict, overload, type_check_only
from typing_extensions import Never, TypeAlias, Unpack

__all__ = [
    "Button",
    "Checkbutton",
    "Combobox",
    "Entry",
    "Frame",
    "Label",
    "Labelframe",
    "LabelFrame",
    "Menubutton",
    "Notebook",
    "Panedwindow",
    "PanedWindow",
    "Progressbar",
    "Radiobutton",
    "Scale",
    "Scrollbar",
    "Separator",
    "Sizegrip",
    "Style",
    "Treeview",
    "LabeledScale",
    "OptionMenu",
    "tclobjs_to_py",
    "setup_master",
    "Spinbox",
]

def tclobjs_to_py(adict: dict[Any, Any]) -> dict[Any, Any]:
    """Returns adict with its values converted from Tcl objects to Python
    objects.
    """

def setup_master(master: tkinter.Misc | None = None):
    """If master is not None, itself is returned. If master is None,
    the default master is returned if there is one, otherwise a new
    master is created and returned.

    If it is not allowed to use the default root and master is None,
    RuntimeError is raised.
    """

_Padding: TypeAlias = (
    float
    | str
    | tuple[float | str]
    | tuple[float | str, float | str]
    | tuple[float | str, float | str, float | str]
    | tuple[float | str, float | str, float | str, float | str]
)

# Last item (option value to apply) varies between different options so use Any.
# It could also be any iterable with items matching the tuple, but that case
# hasn't been added here for consistency with _Padding above.
_Statespec: TypeAlias = tuple[Unpack[tuple[str, ...]], Any]
_ImageStatespec: TypeAlias = tuple[Unpack[tuple[str, ...]], tkinter._Image | str]
_VsapiStatespec: TypeAlias = tuple[Unpack[tuple[str, ...]], int]

class _Layout(TypedDict, total=False):
    side: Literal["left", "right", "top", "bottom"]
    sticky: str  # consists of letters 'n', 's', 'w', 'e', may contain repeats, may be empty
    unit: Literal[0, 1] | bool
    children: _LayoutSpec
    # Note: there seem to be some other undocumented keys sometimes

# This could be any sequence when passed as a parameter but will always be a list when returned.
_LayoutSpec: TypeAlias = list[tuple[str, _Layout | None]]

# Keep these in sync with the appropriate methods in Style
class _ElementCreateImageKwargs(TypedDict, total=False):
    border: _Padding
    height: float | str
    padding: _Padding
    sticky: str
    width: float | str

_ElementCreateArgsCrossPlatform: TypeAlias = (
    # Could be any sequence here but types are not homogenous so just type it as tuple
    tuple[Literal["image"], tkinter._Image | str, Unpack[tuple[_ImageStatespec, ...]], _ElementCreateImageKwargs]
    | tuple[Literal["from"], str, str]
    | tuple[Literal["from"], str]  # (fromelement is optional)
)
if sys.platform == "win32" and sys.version_info >= (3, 13):
    class _ElementCreateVsapiKwargsPadding(TypedDict, total=False):
        padding: _Padding

    class _ElementCreateVsapiKwargsMargin(TypedDict, total=False):
        padding: _Padding

    class _ElementCreateVsapiKwargsSize(TypedDict):
        width: float | str
        height: float | str

    _ElementCreateVsapiKwargsDict: TypeAlias = (
        _ElementCreateVsapiKwargsPadding | _ElementCreateVsapiKwargsMargin | _ElementCreateVsapiKwargsSize
    )
    _ElementCreateArgs: TypeAlias = (  # noqa: Y047  # It doesn't recognise the usage below for whatever reason
        _ElementCreateArgsCrossPlatform
        | tuple[Literal["vsapi"], str, int, _ElementCreateVsapiKwargsDict]
        | tuple[Literal["vsapi"], str, int, _VsapiStatespec, _ElementCreateVsapiKwargsDict]
    )
else:
    _ElementCreateArgs: TypeAlias = _ElementCreateArgsCrossPlatform
_ThemeSettingsValue = TypedDict(
    "_ThemeSettingsValue",
    {
        "configure": dict[str, Any],
        "map": dict[str, Iterable[_Statespec]],
        "layout": _LayoutSpec,
        "element create": _ElementCreateArgs,
    },
    total=False,
)
_ThemeSettings: TypeAlias = dict[str, _ThemeSettingsValue]

class Style:
    """Manipulate style database."""

    master: tkinter.Misc
    tk: _tkinter.TkappType
    def __init__(self, master: tkinter.Misc | None = None) -> None: ...
    # For these methods, values given vary between options. Returned values
    # seem to be str, but this might not always be the case.
    @overload
    def configure(self, style: str) -> dict[str, Any] | None:  # Returns None if no configuration.
        """Query or sets the default value of the specified option(s) in
        style.

        Each key in kw is an option and each value is either a string or
        a sequence identifying the value for that option.
        """

    @overload
    def configure(self, style: str, query_opt: str, **kw: Any) -> Any: ...
    @overload
    def configure(self, style: str, query_opt: None = None, **kw: Any) -> None: ...
    @overload
    def map(self, style: str, query_opt: str) -> _Statespec:
        """Query or sets dynamic values of the specified option(s) in
        style.

        Each key in kw is an option and each value should be a list or a
        tuple (usually) containing statespecs grouped in tuples, or list,
        or something else of your preference. A statespec is compound of
        one or more states and then a value.
        """

    @overload
    def map(self, style: str, query_opt: None = None, **kw: Iterable[_Statespec]) -> dict[str, _Statespec]: ...
    def lookup(self, style: str, option: str, state: Iterable[str] | None = None, default: Any | None = None) -> Any:
        """Returns the value specified for option in style.

        If state is specified it is expected to be a sequence of one
        or more states. If the default argument is set, it is used as
        a fallback value in case no specification for option is found.
        """

    @overload
    def layout(self, style: str, layoutspec: _LayoutSpec) -> list[Never]:  # Always seems to return an empty list
        """Define the widget layout for given style. If layoutspec is
        omitted, return the layout specification for given style.

        layoutspec is expected to be a list or an object different than
        None that evaluates to False if you want to "turn off" that style.
        If it is a list (or tuple, or something else), each item should be
        a tuple where the first item is the layout name and the second item
        should have the format described below:

        LAYOUTS

            A layout can contain the value None, if takes no options, or
            a dict of options specifying how to arrange the element.
            The layout mechanism uses a simplified version of the pack
            geometry manager: given an initial cavity, each element is
            allocated a parcel. Valid options/values are:

                side: whichside
                    Specifies which side of the cavity to place the
                    element; one of top, right, bottom or left. If
                    omitted, the element occupies the entire cavity.

                sticky: nswe
                    Specifies where the element is placed inside its
                    allocated parcel.

                children: [sublayout... ]
                    Specifies a list of elements to place inside the
                    element. Each element is a tuple (or other sequence)
                    where the first item is the layout name, and the other
                    is a LAYOUT.
        """

    @overload
    def layout(self, style: str, layoutspec: None = None) -> _LayoutSpec: ...
    @overload
    def element_create(
        self,
        elementname: str,
        etype: Literal["image"],
        default_image: tkinter._Image | str,
        /,
        *imagespec: _ImageStatespec,
        border: _Padding = ...,
        height: float | str = ...,
        padding: _Padding = ...,
        sticky: str = ...,
        width: float | str = ...,
    ) -> None:
        """Create a new element in the current theme of given etype."""

    @overload
    def element_create(self, elementname: str, etype: Literal["from"], themename: str, fromelement: str = ..., /) -> None: ...
    if sys.platform == "win32" and sys.version_info >= (3, 13):  # and tk version >= 8.6
        # margin, padding, and (width + height) are mutually exclusive. width
        # and height must either both be present or not present at all. Note:
        # There are other undocumented options if you look at ttk's source code.
        @overload
        def element_create(
            self,
            elementname: str,
            etype: Literal["vsapi"],
            class_: str,
            part: int,
            vs_statespec: _VsapiStatespec = ...,
            /,
            *,
            padding: _Padding = ...,
        ) -> None:
            """Create a new element in the current theme of given etype."""

        @overload
        def element_create(
            self,
            elementname: str,
            etype: Literal["vsapi"],
            class_: str,
            part: int,
            vs_statespec: _VsapiStatespec = ...,
            /,
            *,
            margin: _Padding = ...,
        ) -> None: ...
        @overload
        def element_create(
            self,
            elementname: str,
            etype: Literal["vsapi"],
            class_: str,
            part: int,
            vs_statespec: _VsapiStatespec = ...,
            /,
            *,
            width: float | str,
            height: float | str,
        ) -> None: ...

    def element_names(self) -> tuple[str, ...]:
        """Returns the list of elements defined in the current theme."""

    def element_options(self, elementname: str) -> tuple[str, ...]:
        """Return the list of elementname's options."""

    def theme_create(self, themename: str, parent: str | None = None, settings: _ThemeSettings | None = None) -> None:
        """Creates a new theme.

        It is an error if themename already exists. If parent is
        specified, the new theme will inherit styles, elements and
        layouts from the specified parent theme. If settings are present,
        they are expected to have the same syntax used for theme_settings.
        """

    def theme_settings(self, themename: str, settings: _ThemeSettings) -> None:
        """Temporarily sets the current theme to themename, apply specified
        settings and then restore the previous theme.

        Each key in settings is a style and each value may contain the
        keys 'configure', 'map', 'layout' and 'element create' and they
        are expected to have the same format as specified by the methods
        configure, map, layout and element_create respectively.
        """

    def theme_names(self) -> tuple[str, ...]:
        """Returns a list of all known themes."""

    @overload
    def theme_use(self, themename: str) -> None:
        """If themename is None, returns the theme in use, otherwise, set
        the current theme to themename, refreshes all widgets and emits
        a <<ThemeChanged>> event.
        """

    @overload
    def theme_use(self, themename: None = None) -> str: ...

class Widget(tkinter.Widget):
    """Base class for Tk themed widgets."""

    def __init__(self, master: tkinter.Misc | None, widgetname, kw=None) -> None:
        """Constructs a Ttk Widget with the parent master.

        STANDARD OPTIONS

            class, cursor, takefocus, style

        SCROLLABLE WIDGET OPTIONS

            xscrollcommand, yscrollcommand

        LABEL WIDGET OPTIONS

            text, textvariable, underline, image, compound, width

        WIDGET STATES

            active, disabled, focus, pressed, selected, background,
            readonly, alternate, invalid
        """

    def identify(self, x: int, y: int) -> str:
        """Returns the name of the element at position x, y, or the empty
        string if the point does not lie within any element.

        x and y are pixel coordinates relative to the widget.
        """

    def instate(self, statespec, callback=None, *args, **kw):
        """Test the widget's state.

        If callback is not specified, returns True if the widget state
        matches statespec and False otherwise. If callback is specified,
        then it will be invoked with *args, **kw if the widget state
        matches statespec. statespec is expected to be a sequence.
        """

    def state(self, statespec=None):
        """Modify or inquire widget state.

        Widget state is returned if statespec is None, otherwise it is
        set according to the statespec flags and then a new state spec
        is returned indicating which flags were changed. statespec is
        expected to be a sequence.
        """

class Button(Widget):
    """Ttk Button widget, displays a textual label and/or image, and
    evaluates a command when pressed.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        command: str | Callable[[], Any] = "",
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = "",
        cursor: tkinter._Cursor = "",
        default: Literal["normal", "active", "disabled"] = "normal",
        image: tkinter._Image | str = "",
        name: str = ...,
        padding=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = "",
        textvariable: tkinter.Variable = ...,
        underline: int = -1,
        width: int | Literal[""] = "",
    ) -> None:
        """Construct a Ttk Button widget with the parent master.

        STANDARD OPTIONS

            class, compound, cursor, image, state, style, takefocus,
            text, textvariable, underline, width

        WIDGET-SPECIFIC OPTIONS

            command, default, width
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: str | Callable[[], Any] = ...,
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: tkinter._Cursor = ...,
        default: Literal["normal", "active", "disabled"] = ...,
        image: tkinter._Image | str = ...,
        padding=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: tkinter.Variable = ...,
        underline: int = ...,
        width: int | Literal[""] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def invoke(self) -> Any:
        """Invokes the command associated with the button."""

class Checkbutton(Widget):
    """Ttk Checkbutton widget which is either in on- or off-state."""

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        command: str | Callable[[], Any] = "",
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = "",
        cursor: tkinter._Cursor = "",
        image: tkinter._Image | str = "",
        name: str = ...,
        offvalue: Any = 0,
        onvalue: Any = 1,
        padding=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = "",
        textvariable: tkinter.Variable = ...,
        underline: int = -1,
        # Seems like variable can be empty string, but actually setting it to
        # empty string segfaults before Tcl 8.6.9. Search for ttk::checkbutton
        # here: https://sourceforge.net/projects/tcl/files/Tcl/8.6.9/tcltk-release-notes-8.6.9.txt/view
        variable: tkinter.Variable = ...,
        width: int | Literal[""] = "",
    ) -> None:
        """Construct a Ttk Checkbutton widget with the parent master.

        STANDARD OPTIONS

            class, compound, cursor, image, state, style, takefocus,
            text, textvariable, underline, width

        WIDGET-SPECIFIC OPTIONS

            command, offvalue, onvalue, variable
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: str | Callable[[], Any] = ...,
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: tkinter._Cursor = ...,
        image: tkinter._Image | str = ...,
        offvalue: Any = ...,
        onvalue: Any = ...,
        padding=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: tkinter.Variable = ...,
        underline: int = ...,
        variable: tkinter.Variable = ...,
        width: int | Literal[""] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def invoke(self) -> Any:
        """Toggles between the selected and deselected states and
        invokes the associated command. If the widget is currently
        selected, sets the option variable to the offvalue option
        and deselects the widget; otherwise, sets the option variable
        to the option onvalue.

        Returns the result of the associated command.
        """

class Entry(Widget, tkinter.Entry):
    """Ttk Entry widget displays a one-line text string and allows that
    string to be edited by the user.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        widget: str | None = None,
        *,
        background: str = ...,  # undocumented
        class_: str = "",
        cursor: tkinter._Cursor = ...,
        exportselection: bool = True,
        font: _FontDescription = "TkTextFont",
        foreground: str = "",
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        justify: Literal["left", "center", "right"] = "left",
        name: str = ...,
        show: str = "",
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = "none",
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        width: int = 20,
        xscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Constructs a Ttk Entry widget with the parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus, xscrollcommand

        WIDGET-SPECIFIC OPTIONS

            exportselection, invalidcommand, justify, show, state,
            textvariable, validate, validatecommand, width

        VALIDATION MODES

            none, key, focus, focusin, focusout, all
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        cursor: tkinter._Cursor = ...,
        exportselection: bool = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        show: str = ...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    # config must be copy/pasted, otherwise ttk.Entry().config is mypy error (don't know why)
    @overload  # type: ignore[override]
    def config(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        cursor: tkinter._Cursor = ...,
        exportselection: bool = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        show: str = ...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def config(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    def bbox(self, index) -> tuple[int, int, int, int]:  # type: ignore[override]
        """Return a tuple of (x, y, width, height) which describes the
        bounding box of the character given by index.
        """

    def identify(self, x: int, y: int) -> str:
        """Returns the name of the element at position x, y, or the
        empty string if the coordinates are outside the window.
        """

    def validate(self):
        """Force revalidation, independent of the conditions specified
        by the validate option. Returns False if validation fails, True
        if it succeeds. Sets or clears the invalid state accordingly.
        """

class Combobox(Entry):
    """Ttk Combobox widget combines a text field with a pop-down list of
    values.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        background: str = ...,  # undocumented
        class_: str = "",
        cursor: tkinter._Cursor = "",
        exportselection: bool = True,
        font: _FontDescription = ...,  # undocumented
        foreground: str = ...,  # undocumented
        height: int = 10,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,  # undocumented
        justify: Literal["left", "center", "right"] = "left",
        name: str = ...,
        postcommand: Callable[[], object] | str = "",
        show=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,  # undocumented
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,  # undocumented
        values: list[str] | tuple[str, ...] = ...,
        width: int = 20,
        xscrollcommand: str | Callable[[float, float], object] = ...,  # undocumented
    ) -> None:
        """Construct a Ttk Combobox widget with the parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            exportselection, justify, height, postcommand, state,
            textvariable, values, width
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        cursor: tkinter._Cursor = ...,
        exportselection: bool = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: int = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        postcommand: Callable[[], object] | str = ...,
        show=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        values: list[str] | tuple[str, ...] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    # config must be copy/pasted, otherwise ttk.Combobox().config is mypy error (don't know why)
    @overload  # type: ignore[override]
    def config(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        cursor: tkinter._Cursor = ...,
        exportselection: bool = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        height: int = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        postcommand: Callable[[], object] | str = ...,
        show=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        values: list[str] | tuple[str, ...] = ...,
        width: int = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def config(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    def current(self, newindex: int | None = None) -> int:
        """If newindex is supplied, sets the combobox value to the
        element at position newindex in the list of values. Otherwise,
        returns the index of the current value in the list of values
        or -1 if the current value does not appear in the list.
        """

    def set(self, value: Any) -> None:
        """Sets the value of the combobox to value."""

class Frame(Widget):
    """Ttk Frame widget is a container, used to group other widgets
    together.
    """

    # This should be kept in sync with tkinter.ttk.LabeledScale.__init__()
    # (all of these keyword-only arguments are also present there)
    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        border: float | str = ...,
        borderwidth: float | str = ...,
        class_: str = "",
        cursor: tkinter._Cursor = "",
        height: float | str = 0,
        name: str = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        width: float | str = 0,
    ) -> None:
        """Construct a Ttk Frame with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            borderwidth, relief, padding, width, height
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: tkinter._Cursor = ...,
        height: float | str = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Label(Widget):
    """Ttk Label widget displays a textual label and/or image."""

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = "",
        border: float | str = ...,  # alias for borderwidth
        borderwidth: float | str = ...,  # undocumented
        class_: str = "",
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = "",
        cursor: tkinter._Cursor = "",
        font: _FontDescription = ...,
        foreground: str = "",
        image: tkinter._Image | str = "",
        justify: Literal["left", "center", "right"] = ...,
        name: str = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        text: float | str = "",
        textvariable: tkinter.Variable = ...,
        underline: int = -1,
        width: int | Literal[""] = "",
        wraplength: float | str = ...,
    ) -> None:
        """Construct a Ttk Label with parent master.

        STANDARD OPTIONS

            class, compound, cursor, image, style, takefocus, text,
            textvariable, underline, width

        WIDGET-SPECIFIC OPTIONS

            anchor, background, font, foreground, justify, padding,
            relief, text, wraplength
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        background: str = ...,
        border: float | str = ...,
        borderwidth: float | str = ...,
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: tkinter._Cursor = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        image: tkinter._Image | str = ...,
        justify: Literal["left", "center", "right"] = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: tkinter.Variable = ...,
        underline: int = ...,
        width: int | Literal[""] = ...,
        wraplength: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Labelframe(Widget):
    """Ttk Labelframe widget is a container used to group other widgets
    together. It has an optional label, which may be a plain text string
    or another widget.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        border: float | str = ...,
        borderwidth: float | str = ...,  # undocumented
        class_: str = "",
        cursor: tkinter._Cursor = "",
        height: float | str = 0,
        labelanchor: Literal["nw", "n", "ne", "en", "e", "es", "se", "s", "sw", "ws", "w", "wn"] = ...,
        labelwidget: tkinter.Misc = ...,
        name: str = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,  # undocumented
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        text: float | str = "",
        underline: int = -1,
        width: float | str = 0,
    ) -> None:
        """Construct a Ttk Labelframe with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS
            labelanchor, text, underline, padding, labelwidget, width,
            height
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        border: float | str = ...,
        borderwidth: float | str = ...,
        cursor: tkinter._Cursor = ...,
        height: float | str = ...,
        labelanchor: Literal["nw", "n", "ne", "en", "e", "es", "se", "s", "sw", "ws", "w", "wn"] = ...,
        labelwidget: tkinter.Misc = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        underline: int = ...,
        width: float | str = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

LabelFrame = Labelframe

class Menubutton(Widget):
    """Ttk Menubutton widget displays a textual label and/or image, and
    displays a menu when pressed.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = "",
        cursor: tkinter._Cursor = "",
        direction: Literal["above", "below", "left", "right", "flush"] = "below",
        image: tkinter._Image | str = "",
        menu: tkinter.Menu = ...,
        name: str = ...,
        padding=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = "",
        textvariable: tkinter.Variable = ...,
        underline: int = -1,
        width: int | Literal[""] = "",
    ) -> None:
        """Construct a Ttk Menubutton with parent master.

        STANDARD OPTIONS

            class, compound, cursor, image, state, style, takefocus,
            text, textvariable, underline, width

        WIDGET-SPECIFIC OPTIONS

            direction, menu
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: tkinter._Cursor = ...,
        direction: Literal["above", "below", "left", "right", "flush"] = ...,
        image: tkinter._Image | str = ...,
        menu: tkinter.Menu = ...,
        padding=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: tkinter.Variable = ...,
        underline: int = ...,
        width: int | Literal[""] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Notebook(Widget):
    """Ttk Notebook widget manages a collection of windows and displays
    a single one at a time. Each child window is associated with a tab,
    which the user may select to change the currently-displayed window.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        cursor: tkinter._Cursor = "",
        height: int = 0,
        name: str = ...,
        padding: _Padding = ...,
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: int = 0,
    ) -> None:
        """Construct a Ttk Notebook with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            height, padding, width

        TAB OPTIONS

            state, sticky, padding, text, image, compound, underline

        TAB IDENTIFIERS (tab_id)

            The tab_id argument found in several methods may take any of
            the following forms:

                * An integer between zero and the number of tabs
                * The name of a child window
                * A positional specification of the form "@x,y", which
                  defines the tab
                * The string "current", which identifies the
                  currently-selected tab
                * The string "end", which returns the number of tabs (only
                  valid for method index)
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        height: int = ...,
        padding: _Padding = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: int = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def add(
        self,
        child: tkinter.Widget,
        *,
        state: Literal["normal", "disabled", "hidden"] = ...,
        sticky: str = ...,  # consists of letters 'n', 's', 'w', 'e', no repeats, may be empty
        padding: _Padding = ...,
        text: str = ...,
        # `image` is a sequence of an image name, followed by zero or more
        # (sequences of one or more state names followed by an image name)
        image=...,
        compound: Literal["top", "left", "center", "right", "bottom", "none"] = ...,
        underline: int = ...,
    ) -> None:
        """Adds a new tab to the notebook.

        If window is currently managed by the notebook but hidden, it is
        restored to its previous position.
        """

    def forget(self, tab_id) -> None:  # type: ignore[override]
        """Removes the tab specified by tab_id, unmaps and unmanages the
        associated window.
        """

    def hide(self, tab_id) -> None:
        """Hides the tab specified by tab_id.

        The tab will not be displayed, but the associated window remains
        managed by the notebook and its configuration remembered. Hidden
        tabs may be restored with the add command.
        """

    def identify(self, x: int, y: int) -> str:
        """Returns the name of the tab element at position x, y, or the
        empty string if none.
        """

    def index(self, tab_id):
        """Returns the numeric index of the tab specified by tab_id, or
        the total number of tabs if tab_id is the string "end".
        """

    def insert(self, pos, child, **kw) -> None:
        """Inserts a pane at the specified position.

        pos is either the string end, an integer index, or the name of
        a managed child. If child is already managed by the notebook,
        moves it to the specified position.
        """

    def select(self, tab_id=None):
        """Selects the specified tab.

        The associated child window will be displayed, and the
        previously-selected window (if different) is unmapped. If tab_id
        is omitted, returns the widget name of the currently selected
        pane.
        """

    def tab(self, tab_id, option=None, **kw):
        """Query or modify the options of the specific tab_id.

        If kw is not given, returns a dict of the tab option values. If option
        is specified, returns the value of that option. Otherwise, sets the
        options to the corresponding values.
        """

    def tabs(self):
        """Returns a list of windows managed by the notebook."""

    def enable_traversal(self) -> None:
        """Enable keyboard traversal for a toplevel window containing
        this notebook.

        This will extend the bindings for the toplevel window containing
        this notebook as follows:

            Control-Tab: selects the tab following the currently selected
                         one

            Shift-Control-Tab: selects the tab preceding the currently
                               selected one

            Alt-K: where K is the mnemonic (underlined) character of any
                   tab, will select that tab.

        Multiple notebooks in a single toplevel may be enabled for
        traversal, including nested notebooks. However, notebook traversal
        only works properly if all panes are direct children of the
        notebook.
        """

class Panedwindow(Widget, tkinter.PanedWindow):
    """Ttk Panedwindow widget displays a number of subwindows, stacked
    either vertically or horizontally.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        cursor: tkinter._Cursor = "",
        # width and height for tkinter.ttk.Panedwindow are int but for tkinter.PanedWindow they are screen units
        height: int = 0,
        name: str = ...,
        orient: Literal["vertical", "horizontal"] = "vertical",  # can't be changed with configure()
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        width: int = 0,
    ) -> None:
        """Construct a Ttk Panedwindow with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            orient, width, height

        PANE OPTIONS

            weight
        """

    def add(self, child: tkinter.Widget, *, weight: int = ..., **kw) -> None:
        """Add a child widget to the panedwindow in a new pane.

        The child argument is the name of the child widget
        followed by pairs of arguments that specify how to
        manage the windows. The possible options and values
        are the ones accepted by the paneconfigure method.
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        height: int = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: int = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    # config must be copy/pasted, otherwise ttk.Panedwindow().config is mypy error (don't know why)
    @overload  # type: ignore[override]
    def config(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        height: int = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        width: int = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def config(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    forget = tkinter.PanedWindow.forget
    def insert(self, pos, child, **kw) -> None:
        """Inserts a pane at the specified positions.

        pos is either the string end, and integer index, or the name
        of a child. If child is already managed by the paned window,
        moves it to the specified position.
        """

    def pane(self, pane, option=None, **kw):
        """Query or modify the options of the specified pane.

        pane is either an integer index or the name of a managed subwindow.
        If kw is not given, returns a dict of the pane option values. If
        option is specified then the value for that option is returned.
        Otherwise, sets the options to the corresponding values.
        """

    def sashpos(self, index, newpos=None):
        """If newpos is specified, sets the position of sash number index.

        May adjust the positions of adjacent sashes to ensure that
        positions are monotonically increasing. Sash positions are further
        constrained to be between 0 and the total size of the widget.

        Returns the new position of sash number index.
        """

PanedWindow = Panedwindow

class Progressbar(Widget):
    """Ttk Progressbar widget shows the status of a long-running
    operation. They can operate in two modes: determinate mode shows the
    amount completed relative to the total amount of work to be done, and
    indeterminate mode provides an animated display to let the user know
    that something is happening.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        cursor: tkinter._Cursor = "",
        length: float | str = 100,
        maximum: float = 100,
        mode: Literal["determinate", "indeterminate"] = "determinate",
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "horizontal",
        phase: int = 0,  # docs say read-only but assigning int to this works
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        value: float = 0.0,
        variable: tkinter.IntVar | tkinter.DoubleVar = ...,
    ) -> None:
        """Construct a Ttk Progressbar with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            orient, length, mode, maximum, value, variable, phase
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        length: float | str = ...,
        maximum: float = ...,
        mode: Literal["determinate", "indeterminate"] = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        phase: int = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        value: float = ...,
        variable: tkinter.IntVar | tkinter.DoubleVar = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def start(self, interval: Literal["idle"] | int | None = None) -> None:
        """Begin autoincrement mode: schedules a recurring timer event
        that calls method step every interval milliseconds.

        interval defaults to 50 milliseconds (20 steps/second) if omitted.
        """

    def step(self, amount: float | None = None) -> None:
        """Increments the value option by amount.

        amount defaults to 1.0 if omitted.
        """

    def stop(self) -> None:
        """Stop autoincrement mode: cancels any recurring timer event
        initiated by start.
        """

class Radiobutton(Widget):
    """Ttk Radiobutton widgets are used in groups to show or change a
    set of mutually-exclusive options.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        command: str | Callable[[], Any] = "",
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = "",
        cursor: tkinter._Cursor = "",
        image: tkinter._Image | str = "",
        name: str = ...,
        padding=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = "",
        textvariable: tkinter.Variable = ...,
        underline: int = -1,
        value: Any = "1",
        variable: tkinter.Variable | Literal[""] = ...,
        width: int | Literal[""] = "",
    ) -> None:
        """Construct a Ttk Radiobutton with parent master.

        STANDARD OPTIONS

            class, compound, cursor, image, state, style, takefocus,
            text, textvariable, underline, width

        WIDGET-SPECIFIC OPTIONS

            command, value, variable
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: str | Callable[[], Any] = ...,
        compound: Literal["", "text", "image", "top", "left", "center", "right", "bottom", "none"] = ...,
        cursor: tkinter._Cursor = ...,
        image: tkinter._Image | str = ...,
        padding=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        text: float | str = ...,
        textvariable: tkinter.Variable = ...,
        underline: int = ...,
        value: Any = ...,
        variable: tkinter.Variable | Literal[""] = ...,
        width: int | Literal[""] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def invoke(self) -> Any:
        """Sets the option variable to the option value, selects the
        widget, and invokes the associated command.

        Returns the result of the command, or an empty string if
        no command is specified.
        """

# type ignore, because identify() methods of Widget and tkinter.Scale are incompatible
class Scale(Widget, tkinter.Scale):  # type: ignore[misc]
    """Ttk Scale widget is typically used to control the numeric value of
    a linked variable that varies uniformly over some range.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        command: str | Callable[[str], object] = "",
        cursor: tkinter._Cursor = "",
        from_: float = 0,
        length: float | str = 100,
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "horizontal",
        state: str = ...,  # undocumented
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        to: float = 1.0,
        value: float = 0,
        variable: tkinter.IntVar | tkinter.DoubleVar = ...,
    ) -> None:
        """Construct a Ttk Scale with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            command, from, length, orient, to, value, variable
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: str | Callable[[str], object] = ...,
        cursor: tkinter._Cursor = ...,
        from_: float = ...,
        length: float | str = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        to: float = ...,
        value: float = ...,
        variable: tkinter.IntVar | tkinter.DoubleVar = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Modify or query scale options.

        Setting a value for any of the "from", "from_" or "to" options
        generates a <<RangeChanged>> event.
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    # config must be copy/pasted, otherwise ttk.Scale().config is mypy error (don't know why)
    @overload  # type: ignore[override]
    def config(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: str | Callable[[str], object] = ...,
        cursor: tkinter._Cursor = ...,
        from_: float = ...,
        length: float | str = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        to: float = ...,
        value: float = ...,
        variable: tkinter.IntVar | tkinter.DoubleVar = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def config(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    def get(self, x: int | None = None, y: int | None = None) -> float:
        """Get the current value of the value option, or the value
        corresponding to the coordinates x, y if they are specified.

        x and y are pixel coordinates relative to the scale widget
        origin.
        """

# type ignore, because identify() methods of Widget and tkinter.Scale are incompatible
class Scrollbar(Widget, tkinter.Scrollbar):  # type: ignore[misc]
    """Ttk Scrollbar controls the viewport of a scrollable widget."""

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        command: Callable[..., tuple[float, float] | None] | str = "",
        cursor: tkinter._Cursor = "",
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "vertical",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
    ) -> None:
        """Construct a Ttk Scrollbar with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            command, orient
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: Callable[..., tuple[float, float] | None] | str = ...,
        cursor: tkinter._Cursor = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    # config must be copy/pasted, otherwise ttk.Scrollbar().config is mypy error (don't know why)
    @overload  # type: ignore[override]
    def config(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        command: Callable[..., tuple[float, float] | None] | str = ...,
        cursor: tkinter._Cursor = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def config(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...

class Separator(Widget):
    """Ttk Separator widget displays a horizontal or vertical separator
    bar.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        cursor: tkinter._Cursor = "",
        name: str = ...,
        orient: Literal["horizontal", "vertical"] = "horizontal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
    ) -> None:
        """Construct a Ttk Separator with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus

        WIDGET-SPECIFIC OPTIONS

            orient
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        orient: Literal["horizontal", "vertical"] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Sizegrip(Widget):
    """Ttk Sizegrip allows the user to resize the containing toplevel
    window by pressing and dragging the grip.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        cursor: tkinter._Cursor = ...,
        name: str = ...,
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
    ) -> None:
        """Construct a Ttk Sizegrip with parent master.

        STANDARD OPTIONS

            class, cursor, state, style, takefocus
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        cursor: tkinter._Cursor = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure

class Spinbox(Entry):
    """Ttk Spinbox is an Entry with increment and decrement arrows

    It is commonly used for number entry or to select from a list of
    string values.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        background: str = ...,  # undocumented
        class_: str = "",
        command: Callable[[], object] | str | list[str] | tuple[str, ...] = "",
        cursor: tkinter._Cursor = "",
        exportselection: bool = ...,  # undocumented
        font: _FontDescription = ...,  # undocumented
        foreground: str = ...,  # undocumented
        format: str = "",
        from_: float = 0,
        increment: float = 1,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,  # undocumented
        justify: Literal["left", "center", "right"] = ...,  # undocumented
        name: str = ...,
        show=...,  # undocumented
        state: str = "normal",
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,  # undocumented
        to: float = 0,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = "none",
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = "",
        values: list[str] | tuple[str, ...] = ...,
        width: int = ...,  # undocumented
        wrap: bool = False,
        xscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct a Ttk Spinbox widget with the parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus, validate,
            validatecommand, xscrollcommand, invalidcommand

        WIDGET-SPECIFIC OPTIONS

            to, from_, increment, values, wrap, format, command
        """

    @overload  # type: ignore[override]
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        background: str = ...,
        command: Callable[[], object] | str | list[str] | tuple[str, ...] = ...,
        cursor: tkinter._Cursor = ...,
        exportselection: bool = ...,
        font: _FontDescription = ...,
        foreground: str = ...,
        format: str = ...,
        from_: float = ...,
        increment: float = ...,
        invalidcommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        justify: Literal["left", "center", "right"] = ...,
        show=...,
        state: str = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        textvariable: tkinter.Variable = ...,
        to: float = ...,
        validate: Literal["none", "focus", "focusin", "focusout", "key", "all"] = ...,
        validatecommand: str | list[str] | tuple[str, ...] | Callable[[], bool] = ...,
        values: list[str] | tuple[str, ...] = ...,
        width: int = ...,
        wrap: bool = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure  # type: ignore[assignment]
    def set(self, value: Any) -> None:
        """Sets the value of the Spinbox to value."""

@type_check_only
class _TreeviewItemDict(TypedDict):
    text: str
    image: list[str] | Literal[""]  # no idea why it's wrapped in list
    values: list[Any] | Literal[""]
    open: bool  # actually 0 or 1
    tags: list[str] | Literal[""]

@type_check_only
class _TreeviewTagDict(TypedDict):
    # There is also 'text' and 'anchor', but they don't seem to do anything, using them is likely a bug
    foreground: str
    background: str
    font: _FontDescription
    image: str  # not wrapped in list :D

@type_check_only
class _TreeviewHeaderDict(TypedDict):
    text: str
    image: list[str] | Literal[""]
    anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"]
    command: str
    state: str  # Doesn't seem to appear anywhere else than in these dicts

@type_check_only
class _TreeviewColumnDict(TypedDict):
    width: int
    minwidth: int
    stretch: bool  # actually 0 or 1
    anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"]
    id: str

class Treeview(Widget, tkinter.XView, tkinter.YView):
    """Ttk Treeview widget displays a hierarchical collection of items.

    Each item has a textual label, an optional image, and an optional list
    of data values. The data values are displayed in successive columns
    after the tree label.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        *,
        class_: str = "",
        columns: str | list[str] | list[int] | list[str | int] | tuple[str | int, ...] = "",
        cursor: tkinter._Cursor = "",
        displaycolumns: str | int | list[str] | tuple[str, ...] | list[int] | tuple[int, ...] = ("#all",),
        height: int = 10,
        name: str = ...,
        padding: _Padding = ...,
        selectmode: Literal["extended", "browse", "none"] = "extended",
        # list/tuple of Literal don't actually work in mypy
        #
        # 'tree headings' is same as ['tree', 'headings'], and I wouldn't be
        # surprised if someone is using it.
        show: Literal["tree", "headings", "tree headings", ""] | list[str] | tuple[str, ...] = ("tree", "headings"),
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        xscrollcommand: str | Callable[[float, float], object] = "",
        yscrollcommand: str | Callable[[float, float], object] = "",
    ) -> None:
        """Construct a Ttk Treeview with parent master.

        STANDARD OPTIONS

            class, cursor, style, takefocus, xscrollcommand,
            yscrollcommand

        WIDGET-SPECIFIC OPTIONS

            columns, displaycolumns, height, padding, selectmode, show

        ITEM OPTIONS

            text, image, values, open, tags

        TAG OPTIONS

            foreground, background, font, image
        """

    @overload
    def configure(
        self,
        cnf: dict[str, Any] | None = None,
        *,
        columns: str | list[str] | list[int] | list[str | int] | tuple[str | int, ...] = ...,
        cursor: tkinter._Cursor = ...,
        displaycolumns: str | int | list[str] | tuple[str, ...] | list[int] | tuple[int, ...] = ...,
        height: int = ...,
        padding: _Padding = ...,
        selectmode: Literal["extended", "browse", "none"] = ...,
        show: Literal["tree", "headings", "tree headings", ""] | list[str] | tuple[str, ...] = ...,
        style: str = ...,
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = ...,
        xscrollcommand: str | Callable[[float, float], object] = ...,
        yscrollcommand: str | Callable[[float, float], object] = ...,
    ) -> dict[str, tuple[str, str, str, Any, Any]] | None:
        """Query or modify the configuration options of the widget.

        If no arguments are specified, return a dictionary describing
        all of the available options for the widget.

        If an option name is specified, then return a tuple describing
        the one named option.

        If one or more keyword arguments are specified or a dictionary
        is specified, then modify the widget option(s) to have the given
        value(s).
        """

    @overload
    def configure(self, cnf: str) -> tuple[str, str, str, Any, Any]: ...
    config = configure
    def bbox(self, item: str | int, column: str | int | None = None) -> tuple[int, int, int, int] | Literal[""]:  # type: ignore[override]
        """Returns the bounding box (relative to the treeview widget's
        window) of the specified item in the form x y width height.

        If column is specified, returns the bounding box of that cell.
        If the item is not visible (i.e., if it is a descendant of a
        closed item or is scrolled offscreen), returns an empty string.
        """

    def get_children(self, item: str | int | None = None) -> tuple[str, ...]:
        """Returns a tuple of children belonging to item.

        If item is not specified, returns root children.
        """

    def set_children(self, item: str | int, *newchildren: str | int) -> None:
        """Replaces item's child with newchildren.

        Children present in item that are not present in newchildren
        are detached from tree. No items in newchildren may be an
        ancestor of item.
        """

    @overload
    def column(self, column: str | int, option: Literal["width", "minwidth"]) -> int:
        """Query or modify the options for the specified column.

        If kw is not given, returns a dict of the column option values. If
        option is specified then the value for that option is returned.
        Otherwise, sets the options to the corresponding values.
        """

    @overload
    def column(self, column: str | int, option: Literal["stretch"]) -> bool: ...  # actually 0 or 1
    @overload
    def column(self, column: str | int, option: Literal["anchor"]) -> _tkinter.Tcl_Obj: ...
    @overload
    def column(self, column: str | int, option: Literal["id"]) -> str: ...
    @overload
    def column(self, column: str | int, option: str) -> Any: ...
    @overload
    def column(
        self,
        column: str | int,
        option: None = None,
        *,
        width: int = ...,
        minwidth: int = ...,
        stretch: bool = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        # id is read-only
    ) -> _TreeviewColumnDict | None: ...
    def delete(self, *items: str | int) -> None:
        """Delete all specified items and all their descendants. The root
        item may not be deleted.
        """

    def detach(self, *items: str | int) -> None:
        """Unlinks all of the specified items from the tree.

        The items and all of their descendants are still present, and may
        be reinserted at another point in the tree, but will not be
        displayed. The root item may not be detached.
        """

    def exists(self, item: str | int) -> bool:
        """Returns True if the specified item is present in the tree,
        False otherwise.
        """

    @overload  # type: ignore[override]
    def focus(self, item: None = None) -> str:  # can return empty string
        """If item is specified, sets the focus item to item. Otherwise,
        returns the current focus item, or '' if there is none.
        """

    @overload
    def focus(self, item: str | int) -> Literal[""]: ...
    @overload
    def heading(self, column: str | int, option: Literal["text"]) -> str:
        """Query or modify the heading options for the specified column.

        If kw is not given, returns a dict of the heading option values. If
        option is specified then the value for that option is returned.
        Otherwise, sets the options to the corresponding values.

        Valid options/values are:
            text: text
                The text to display in the column heading
            image: image_name
                Specifies an image to display to the right of the column
                heading
            anchor: anchor
                Specifies how the heading text should be aligned. One of
                the standard Tk anchor values
            command: callback
                A callback to be invoked when the heading label is
                pressed.

        To configure the tree column heading, call this with column = "#0"
        """

    @overload
    def heading(self, column: str | int, option: Literal["image"]) -> tuple[str] | str: ...
    @overload
    def heading(self, column: str | int, option: Literal["anchor"]) -> _tkinter.Tcl_Obj: ...
    @overload
    def heading(self, column: str | int, option: Literal["command"]) -> str: ...
    @overload
    def heading(self, column: str | int, option: str) -> Any: ...
    @overload
    def heading(self, column: str | int, option: None = None) -> _TreeviewHeaderDict: ...
    @overload
    def heading(
        self,
        column: str | int,
        option: None = None,
        *,
        text: str = ...,
        image: tkinter._Image | str = ...,
        anchor: Literal["nw", "n", "ne", "w", "center", "e", "sw", "s", "se"] = ...,
        command: str | Callable[[], object] = ...,
    ) -> None: ...
    # Internal Method. Leave untyped:
    def identify(self, component, x, y):  # type: ignore[override]
        """Returns a description of the specified component under the
        point given by x and y, or the empty string if no such component
        is present at that position.
        """

    def identify_row(self, y: int) -> str:
        """Returns the item ID of the item at position y."""

    def identify_column(self, x: int) -> str:
        """Returns the data column identifier of the cell at position x.

        The tree column has ID #0.
        """

    def identify_region(self, x: int, y: int) -> Literal["heading", "separator", "tree", "cell", "nothing"]:
        """Returns one of:

        heading: Tree heading area.
        separator: Space between two columns headings;
        tree: The tree area.
        cell: A data cell.

        * Availability: Tk 8.6
        """

    def identify_element(self, x: int, y: int) -> str:  # don't know what possible return values are
        """Returns the element at position x, y.

        * Availability: Tk 8.6
        """

    def index(self, item: str | int) -> int:
        """Returns the integer index of item within its parent's list
        of children.
        """

    def insert(
        self,
        parent: str,
        index: int | Literal["end"],
        iid: str | int | None = None,
        *,
        id: str | int = ...,  # same as iid
        text: str = ...,
        image: tkinter._Image | str = ...,
        values: list[Any] | tuple[Any, ...] = ...,
        open: bool = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
    ) -> str:
        """Creates a new item and return the item identifier of the newly
        created item.

        parent is the item ID of the parent item, or the empty string
        to create a new top-level item. index is an integer, or the value
        end, specifying where in the list of parent's children to insert
        the new item. If index is less than or equal to zero, the new node
        is inserted at the beginning, if index is greater than or equal to
        the current number of children, it is inserted at the end. If iid
        is specified, it is used as the item identifier, iid must not
        already exist in the tree. Otherwise, a new unique identifier
        is generated.
        """

    @overload
    def item(self, item: str | int, option: Literal["text"]) -> str:
        """Query or modify the options for the specified item.

        If no options are given, a dict with options/values for the item
        is returned. If option is specified then the value for that option
        is returned. Otherwise, sets the options to the corresponding
        values as given by kw.
        """

    @overload
    def item(self, item: str | int, option: Literal["image"]) -> tuple[str] | Literal[""]: ...
    @overload
    def item(self, item: str | int, option: Literal["values"]) -> tuple[Any, ...] | Literal[""]: ...
    @overload
    def item(self, item: str | int, option: Literal["open"]) -> bool: ...  # actually 0 or 1
    @overload
    def item(self, item: str | int, option: Literal["tags"]) -> tuple[str, ...] | Literal[""]: ...
    @overload
    def item(self, item: str | int, option: str) -> Any: ...
    @overload
    def item(self, item: str | int, option: None = None) -> _TreeviewItemDict: ...
    @overload
    def item(
        self,
        item: str | int,
        option: None = None,
        *,
        text: str = ...,
        image: tkinter._Image | str = ...,
        values: list[Any] | tuple[Any, ...] | Literal[""] = ...,
        open: bool = ...,
        tags: str | list[str] | tuple[str, ...] = ...,
    ) -> None: ...
    def move(self, item: str | int, parent: str, index: int | Literal["end"]) -> None:
        """Moves item to position index in parent's list of children.

        It is illegal to move an item under one of its descendants. If
        index is less than or equal to zero, item is moved to the
        beginning, if greater than or equal to the number of children,
        it is moved to the end. If item was detached it is reattached.
        """
    reattach = move
    def next(self, item: str | int) -> str:  # returning empty string means last item
        """Returns the identifier of item's next sibling, or '' if item
        is the last child of its parent.
        """

    def parent(self, item: str | int) -> str:
        """Returns the ID of the parent of item, or '' if item is at the
        top level of the hierarchy.
        """

    def prev(self, item: str | int) -> str:  # returning empty string means first item
        """Returns the identifier of item's previous sibling, or '' if
        item is the first child of its parent.
        """

    def see(self, item: str | int) -> None:
        """Ensure that item is visible.

        Sets all of item's ancestors open option to True, and scrolls
        the widget if necessary so that item is within the visible
        portion of the tree.
        """

    def selection(self) -> tuple[str, ...]:
        """Returns the tuple of selected items."""

    @overload
    def selection_set(self, items: list[str] | tuple[str, ...] | list[int] | tuple[int, ...], /) -> None:
        """The specified items becomes the new selection."""

    @overload
    def selection_set(self, *items: str | int) -> None: ...
    @overload
    def selection_add(self, items: list[str] | tuple[str, ...] | list[int] | tuple[int, ...], /) -> None:
        """Add all of the specified items to the selection."""

    @overload
    def selection_add(self, *items: str | int) -> None: ...
    @overload
    def selection_remove(self, items: list[str] | tuple[str, ...] | list[int] | tuple[int, ...], /) -> None:
        """Remove all of the specified items from the selection."""

    @overload
    def selection_remove(self, *items: str | int) -> None: ...
    @overload
    def selection_toggle(self, items: list[str] | tuple[str, ...] | list[int] | tuple[int, ...], /) -> None:
        """Toggle the selection state of each specified item."""

    @overload
    def selection_toggle(self, *items: str | int) -> None: ...
    @overload
    def set(self, item: str | int, column: None = None, value: None = None) -> dict[str, Any]:
        """Query or set the value of given item.

        With one argument, return a dictionary of column/value pairs
        for the specified item. With two arguments, return the current
        value of the specified column. With three arguments, set the
        value of given column in given item to the specified value.
        """

    @overload
    def set(self, item: str | int, column: str | int, value: None = None) -> Any: ...
    @overload
    def set(self, item: str | int, column: str | int, value: Any) -> Literal[""]: ...
    # There's no tag_unbind() or 'add' argument for whatever reason.
    # Also, it's 'callback' instead of 'func' here.
    @overload
    def tag_bind(
        self, tagname: str, sequence: str | None = None, callback: Callable[[tkinter.Event[Treeview]], object] | None = None
    ) -> str:
        """Bind a callback for the given event sequence to the tag tagname.
        When an event is delivered to an item, the callbacks for each
        of the item's tags option are called.
        """

    @overload
    def tag_bind(self, tagname: str, sequence: str | None, callback: str) -> None: ...
    @overload
    def tag_bind(self, tagname: str, *, callback: str) -> None: ...
    @overload
    def tag_configure(self, tagname: str, option: Literal["foreground", "background"]) -> str:
        """Query or modify the options for the specified tagname.

        If kw is not given, returns a dict of the option settings for tagname.
        If option is specified, returns the value for that option for the
        specified tagname. Otherwise, sets the options to the corresponding
        values for the given tagname.
        """

    @overload
    def tag_configure(self, tagname: str, option: Literal["font"]) -> _FontDescription: ...
    @overload
    def tag_configure(self, tagname: str, option: Literal["image"]) -> str: ...
    @overload
    def tag_configure(
        self,
        tagname: str,
        option: None = None,
        *,
        # There is also 'text' and 'anchor', but they don't seem to do anything, using them is likely a bug
        foreground: str = ...,
        background: str = ...,
        font: _FontDescription = ...,
        image: tkinter._Image | str = ...,
    ) -> _TreeviewTagDict | MaybeNone: ...  # can be None but annoying to check
    @overload
    def tag_has(self, tagname: str, item: None = None) -> tuple[str, ...]:
        """If item is specified, returns 1 or 0 depending on whether the
        specified item has the given tagname. Otherwise, returns a list of
        all items which have the specified tag.

        * Availability: Tk 8.6
        """

    @overload
    def tag_has(self, tagname: str, item: str | int) -> bool: ...

class LabeledScale(Frame):
    """A Ttk Scale widget with a Ttk Label widget indicating its
    current value.

    The Ttk Scale can be accessed through instance.scale, and Ttk Label
    can be accessed through instance.label
    """

    label: Label
    scale: Scale
    # This should be kept in sync with tkinter.ttk.Frame.__init__()
    # (all the keyword-only args except compound are from there)
    def __init__(
        self,
        master: tkinter.Misc | None = None,
        variable: tkinter.IntVar | tkinter.DoubleVar | None = None,
        from_: float = 0,
        to: float = 10,
        *,
        border: float | str = ...,
        borderwidth: float | str = ...,
        class_: str = "",
        compound: Literal["top", "bottom"] = "top",
        cursor: tkinter._Cursor = "",
        height: float | str = 0,
        name: str = ...,
        padding: _Padding = ...,
        relief: Literal["raised", "sunken", "flat", "ridge", "solid", "groove"] = ...,
        style: str = "",
        takefocus: bool | Literal[0, 1, ""] | Callable[[str], bool | None] = "",
        width: float | str = 0,
    ) -> None:
        """Construct a horizontal LabeledScale with parent master, a
        variable to be associated with the Ttk Scale widget and its range.
        If variable is not specified, a tkinter.IntVar is created.

        WIDGET-SPECIFIC OPTIONS

            compound: 'top' or 'bottom'
                Specifies how to display the label relative to the scale.
                Defaults to 'top'.
        """
    # destroy is overridden, signature does not change
    value: Any

class OptionMenu(Menubutton):
    """Themed OptionMenu, based after tkinter's OptionMenu, which allows
    the user to select a value from a menu.
    """

    def __init__(
        self,
        master: tkinter.Misc | None,
        variable: tkinter.StringVar,
        default: str | None = None,
        *values: str,
        # rest of these are keyword-only because *args syntax used above
        style: str = "",
        direction: Literal["above", "below", "left", "right", "flush"] = "below",
        command: Callable[[tkinter.StringVar], object] | None = None,
    ) -> None:
        """Construct a themed OptionMenu widget with master as the parent,
        the option textvariable set to variable, the initially selected
        value specified by the default parameter, the menu values given by
        *values and additional keywords.

        WIDGET-SPECIFIC OPTIONS

            style: stylename
                Menubutton style.
            direction: 'above', 'below', 'left', 'right', or 'flush'
                Menubutton direction.
            command: callback
                A callback that will be invoked after selecting an item.
        """
    # configure, config, cget, destroy are inherited from Menubutton
    # destroy and __setitem__ are overridden, signature does not change
    def set_menu(self, default: str | None = None, *values: str) -> None:
        """Build a new menu of radiobuttons with *values and optionally
        a default value.
        """
