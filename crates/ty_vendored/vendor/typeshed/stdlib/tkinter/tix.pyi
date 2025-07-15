import tkinter
from _typeshed import Incomplete
from typing import Any, Final

WINDOW: Final = "window"
TEXT: Final = "text"
STATUS: Final = "status"
IMMEDIATE: Final = "immediate"
IMAGE: Final = "image"
IMAGETEXT: Final = "imagetext"
BALLOON: Final = "balloon"
AUTO: Final = "auto"
ACROSSTOP: Final = "acrosstop"

ASCII: Final = "ascii"
CELL: Final = "cell"
COLUMN: Final = "column"
DECREASING: Final = "decreasing"
INCREASING: Final = "increasing"
INTEGER: Final = "integer"
MAIN: Final = "main"
MAX: Final = "max"
REAL: Final = "real"
ROW: Final = "row"
S_REGION: Final = "s-region"
X_REGION: Final = "x-region"
Y_REGION: Final = "y-region"

# These should be kept in sync with _tkinter constants, except TCL_ALL_EVENTS which doesn't match ALL_EVENTS
TCL_DONT_WAIT: Final = 2
TCL_WINDOW_EVENTS: Final = 4
TCL_FILE_EVENTS: Final = 8
TCL_TIMER_EVENTS: Final = 16
TCL_IDLE_EVENTS: Final = 32
TCL_ALL_EVENTS: Final = 0

class tixCommand:
    """The tix commands provide access to miscellaneous  elements
    of  Tix's  internal state and the Tix application context.
    Most of the information manipulated by these  commands pertains
    to  the  application  as a whole, or to a screen or
    display, rather than to a particular window.

    This is a mixin class, assumed to be mixed to Tkinter.Tk
    that supports the self.tk.call method.
    """

    def tix_addbitmapdir(self, directory: str) -> None:
        """Tix maintains a list of directories under which
        the  tix_getimage  and tix_getbitmap commands will
        search for image files. The standard bitmap  directory
        is $TIX_LIBRARY/bitmaps. The addbitmapdir command
        adds directory into this list. By  using  this
        command, the  image  files  of an applications can
        also be located using the tix_getimage or tix_getbitmap
        command.
        """

    def tix_cget(self, option: str) -> Any:
        """Returns  the  current  value  of the configuration
        option given by option. Option may be  any  of  the
        options described in the CONFIGURATION OPTIONS section.
        """

    def tix_configure(self, cnf: dict[str, Any] | None = None, **kw: Any) -> Any:
        """Query or modify the configuration options of the Tix application
        context. If no option is specified, returns a dictionary all of the
        available options.  If option is specified with no value, then the
        command returns a list describing the one named option (this list
        will be identical to the corresponding sublist of the value
        returned if no option is specified).  If one or more option-value
        pairs are specified, then the command modifies the given option(s)
        to have the given value(s); in this case the command returns an
        empty string. Option may be any of the configuration options.
        """

    def tix_filedialog(self, dlgclass: str | None = None) -> str:
        """Returns the file selection dialog that may be shared among
        different calls from this application.  This command will create a
        file selection dialog widget when it is called the first time. This
        dialog will be returned by all subsequent calls to tix_filedialog.
        An optional dlgclass parameter can be passed to specified what type
        of file selection dialog widget is desired. Possible options are
        tix FileSelectDialog or tixExFileSelectDialog.
        """

    def tix_getbitmap(self, name: str) -> str:
        """Locates a bitmap file of the name name.xpm or name in one of the
        bitmap directories (see the tix_addbitmapdir command above).  By
        using tix_getbitmap, you can avoid hard coding the pathnames of the
        bitmap files in your application. When successful, it returns the
        complete pathname of the bitmap file, prefixed with the character
        '@'.  The returned value can be used to configure the -bitmap
        option of the TK and Tix widgets.
        """

    def tix_getimage(self, name: str) -> str:
        """Locates an image file of the name name.xpm, name.xbm or name.ppm
        in one of the bitmap directories (see the addbitmapdir command
        above). If more than one file with the same name (but different
        extensions) exist, then the image type is chosen according to the
        depth of the X display: xbm images are chosen on monochrome
        displays and color images are chosen on color displays. By using
        tix_ getimage, you can avoid hard coding the pathnames of the
        image files in your application. When successful, this command
        returns the name of the newly created image, which can be used to
        configure the -image option of the Tk and Tix widgets.
        """

    def tix_option_get(self, name: str) -> Any:
        """Gets  the options  maintained  by  the  Tix
        scheme mechanism. Available options include:

            active_bg       active_fg      bg
            bold_font       dark1_bg       dark1_fg
            dark2_bg        dark2_fg       disabled_fg
            fg              fixed_font     font
            inactive_bg     inactive_fg    input1_bg
            input2_bg       italic_font    light1_bg
            light1_fg       light2_bg      light2_fg
            menu_font       output1_bg     output2_bg
            select_bg       select_fg      selector
        """

    def tix_resetoptions(self, newScheme: str, newFontSet: str, newScmPrio: str | None = None) -> None:
        """Resets the scheme and fontset of the Tix application to
        newScheme and newFontSet, respectively.  This affects only those
        widgets created after this call. Therefore, it is best to call the
        resetoptions command before the creation of any widgets in a Tix
        application.

        The optional parameter newScmPrio can be given to reset the
        priority level of the Tk options set by the Tix schemes.

        Because of the way Tk handles the X option database, after Tix has
        been has imported and inited, it is not possible to reset the color
        schemes and font sets using the tix config command.  Instead, the
        tix_resetoptions command must be used.
        """

class Tk(tkinter.Tk, tixCommand):
    """Toplevel widget of Tix which represents mostly the main window
    of an application. It has an associated Tcl interpreter.
    """

    def __init__(self, screenName: str | None = None, baseName: str | None = None, className: str = "Tix") -> None: ...

class TixWidget(tkinter.Widget):
    """A TixWidget class is used to package all (or most) Tix widgets.

    Widget initialization is extended in two ways:
       1) It is possible to give a list of options which must be part of
       the creation command (so called Tix 'static' options). These cannot be
       given as a 'config' command later.
       2) It is possible to give the name of an existing TK widget. These are
       child widgets created automatically by a Tix mega-widget. The Tk call
       to create these widgets is therefore bypassed in TixWidget.__init__

    Both options are for use by subclasses only.
    """

    def __init__(
        self,
        master: tkinter.Misc | None = None,
        widgetName: str | None = None,
        static_options: list[str] | None = None,
        cnf: dict[str, Any] = {},
        kw: dict[str, Any] = {},
    ) -> None: ...
    def __getattr__(self, name: str): ...
    def set_silent(self, value: str) -> None:
        """Set a variable without calling its action routine"""

    def subwidget(self, name: str) -> tkinter.Widget:
        """Return the named subwidget (which must have been created by
        the sub-class).
        """

    def subwidgets_all(self) -> list[tkinter.Widget]:
        """Return all subwidgets."""

    def config_all(self, option: Any, value: Any) -> None:
        """Set configuration options for all subwidgets (and self)."""

    def image_create(self, imgtype: str, cnf: dict[str, Any] = {}, master: tkinter.Widget | None = None, **kw) -> None: ...
    def image_delete(self, imgname: str) -> None: ...

class TixSubWidget(TixWidget):
    """Subwidget class.

    This is used to mirror child widgets automatically created
    by Tix/Tk as part of a mega-widget in Python (which is not informed
    of this)
    """

    def __init__(self, master: tkinter.Widget, name: str, destroy_physically: int = 1, check_intermediate: int = 1) -> None: ...

class DisplayStyle:
    """DisplayStyle - handle configuration options shared by
    (multiple) Display Items
    """

    def __init__(self, itemtype: str, cnf: dict[str, Any] = {}, *, master: tkinter.Widget | None = None, **kw) -> None: ...
    def __getitem__(self, key: str): ...
    def __setitem__(self, key: str, value: Any) -> None: ...
    def delete(self) -> None: ...
    def config(self, cnf: dict[str, Any] = {}, **kw): ...

class Balloon(TixWidget):
    """Balloon help widget.

    Subwidget       Class
    ---------       -----
    label           Label
    message         Message
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def bind_widget(self, widget: tkinter.Widget, cnf: dict[str, Any] = {}, **kw) -> None:
        """Bind balloon widget to another.
        One balloon widget may be bound to several widgets at the same time
        """

    def unbind_widget(self, widget: tkinter.Widget) -> None: ...

class ButtonBox(TixWidget):
    """ButtonBox - A container for pushbuttons.
    Subwidgets are the buttons added with the add method.
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, name: str, cnf: dict[str, Any] = {}, **kw) -> tkinter.Widget:
        """Add a button with given name to box."""

    def invoke(self, name: str) -> None: ...

class ComboBox(TixWidget):
    """ComboBox - an Entry field with a dropdown menu. The user can select a
    choice by either typing in the entry subwidget or selecting from the
    listbox subwidget.

    Subwidget       Class
    ---------       -----
    entry       Entry
    arrow       Button
    slistbox    ScrolledListBox
    tick        Button
    cross       Button : present if created with the fancy option
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add_history(self, str: str) -> None: ...
    def append_history(self, str: str) -> None: ...
    def insert(self, index: int, str: str) -> None: ...
    def pick(self, index: int) -> None: ...

class Control(TixWidget):
    """Control - An entry field with value change arrows.  The user can
    adjust the value by pressing the two arrow buttons or by entering
    the value directly into the entry. The new value will be checked
    against the user-defined upper and lower limits.

    Subwidget       Class
    ---------       -----
    incr       Button
    decr       Button
    entry       Entry
    label       Label
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def decrement(self) -> None: ...
    def increment(self) -> None: ...
    def invoke(self) -> None: ...

class LabelEntry(TixWidget):
    """LabelEntry - Entry field with label. Packages an entry widget
    and a label into one mega widget. It can be used to simplify the creation
    of ``entry-form'' type of interface.

    Subwidgets       Class
    ----------       -----
    label       Label
    entry       Entry
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...

class LabelFrame(TixWidget):
    """LabelFrame - Labelled Frame container. Packages a frame widget
    and a label into one mega widget. To create widgets inside a
    LabelFrame widget, one creates the new widgets relative to the
    frame subwidget and manage them inside the frame subwidget.

    Subwidgets       Class
    ----------       -----
    label       Label
    frame       Frame
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...

class Meter(TixWidget):
    """The Meter widget can be used to show the progress of a background
    job which may take a long time to execute.
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...

class OptionMenu(TixWidget):
    """OptionMenu - creates a menu button of options.

    Subwidget       Class
    ---------       -----
    menubutton      Menubutton
    menu            Menu
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add_command(self, name: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add_separator(self, name: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def delete(self, name: str) -> None: ...
    def disable(self, name: str) -> None: ...
    def enable(self, name: str) -> None: ...

class PopupMenu(TixWidget):
    """PopupMenu widget can be used as a replacement of the tk_popup command.
    The advantage of the Tix PopupMenu widget is it requires less application
    code to manipulate.


    Subwidgets       Class
    ----------       -----
    menubutton       Menubutton
    menu       Menu
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def bind_widget(self, widget: tkinter.Widget) -> None: ...
    def unbind_widget(self, widget: tkinter.Widget) -> None: ...
    def post_widget(self, widget: tkinter.Widget, x: int, y: int) -> None: ...

class Select(TixWidget):
    """Select - Container of button subwidgets. It can be used to provide
    radio-box or check-box style of selection options for the user.

    Subwidgets are buttons added dynamically using the add method.
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, name: str, cnf: dict[str, Any] = {}, **kw) -> tkinter.Widget: ...
    def invoke(self, name: str) -> None: ...

class StdButtonBox(TixWidget):
    """StdButtonBox - Standard Button Box (OK, Apply, Cancel and Help)"""

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def invoke(self, name: str) -> None: ...

class DirList(TixWidget):
    """DirList - displays a list view of a directory, its previous
    directories and its sub-directories. The user can choose one of
    the directories displayed in the list or change to another directory.

    Subwidget       Class
    ---------       -----
    hlist       HList
    hsb              Scrollbar
    vsb              Scrollbar
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def chdir(self, dir: str) -> None: ...

class DirTree(TixWidget):
    """DirTree - Directory Listing in a hierarchical view.
    Displays a tree view of a directory, its previous directories and its
    sub-directories. The user can choose one of the directories displayed
    in the list or change to another directory.

    Subwidget       Class
    ---------       -----
    hlist           HList
    hsb             Scrollbar
    vsb             Scrollbar
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def chdir(self, dir: str) -> None: ...

class DirSelectDialog(TixWidget):
    """The DirSelectDialog widget presents the directories in the file
    system in a dialog window. The user can use this dialog window to
    navigate through the file system to select the desired directory.

    Subwidgets       Class
    ----------       -----
    dirbox       DirSelectDialog
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def popup(self) -> None: ...
    def popdown(self) -> None: ...

class DirSelectBox(TixWidget):
    """DirSelectBox - Motif style file select box.
    It is generally used for
    the user to choose a file. FileSelectBox stores the files mostly
    recently selected into a ComboBox widget so that they can be quickly
    selected again.

    Subwidget       Class
    ---------       -----
    selection       ComboBox
    filter          ComboBox
    dirlist         ScrolledListBox
    filelist        ScrolledListBox
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...

class ExFileSelectBox(TixWidget):
    """ExFileSelectBox - MS Windows style file select box.
    It provides a convenient method for the user to select files.

    Subwidget       Class
    ---------       -----
    cancel       Button
    ok              Button
    hidden       Checkbutton
    types       ComboBox
    dir              ComboBox
    file       ComboBox
    dirlist       ScrolledListBox
    filelist       ScrolledListBox
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def filter(self) -> None: ...
    def invoke(self) -> None: ...

class FileSelectBox(TixWidget):
    """ExFileSelectBox - Motif style file select box.
    It is generally used for
    the user to choose a file. FileSelectBox stores the files mostly
    recently selected into a ComboBox widget so that they can be quickly
    selected again.

    Subwidget       Class
    ---------       -----
    selection       ComboBox
    filter          ComboBox
    dirlist         ScrolledListBox
    filelist        ScrolledListBox
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def apply_filter(self) -> None: ...
    def invoke(self) -> None: ...

class FileEntry(TixWidget):
    """FileEntry - Entry field with button that invokes a FileSelectDialog.
    The user can type in the filename manually. Alternatively, the user can
    press the button widget that sits next to the entry, which will bring
    up a file selection dialog.

    Subwidgets       Class
    ----------       -----
    button       Button
    entry       Entry
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def invoke(self) -> None: ...
    def file_dialog(self) -> None: ...

class HList(TixWidget, tkinter.XView, tkinter.YView):
    """HList - Hierarchy display  widget can be used to display any data
    that have a hierarchical structure, for example, file system directory
    trees. The list entries are indented and connected by branch lines
    according to their places in the hierarchy.

    Subwidgets - None
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, entry: str, cnf: dict[str, Any] = {}, **kw) -> tkinter.Widget: ...
    def add_child(self, parent: str | None = None, cnf: dict[str, Any] = {}, **kw) -> tkinter.Widget: ...
    def anchor_set(self, entry: str) -> None: ...
    def anchor_clear(self) -> None: ...
    # FIXME: Overload, certain combos return, others don't
    def column_width(self, col: int = 0, width: int | None = None, chars: int | None = None) -> int | None: ...
    def delete_all(self) -> None: ...
    def delete_entry(self, entry: str) -> None: ...
    def delete_offsprings(self, entry: str) -> None: ...
    def delete_siblings(self, entry: str) -> None: ...
    def dragsite_set(self, index: int) -> None: ...
    def dragsite_clear(self) -> None: ...
    def dropsite_set(self, index: int) -> None: ...
    def dropsite_clear(self) -> None: ...
    def header_create(self, col: int, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def header_configure(self, col: int, cnf: dict[str, Any] = {}, **kw) -> Incomplete | None: ...
    def header_cget(self, col: int, opt): ...
    def header_exists(self, col: int) -> bool: ...
    def header_exist(self, col: int) -> bool: ...
    def header_delete(self, col: int) -> None: ...
    def header_size(self, col: int) -> int: ...
    def hide_entry(self, entry: str) -> None: ...
    def indicator_create(self, entry: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def indicator_configure(self, entry: str, cnf: dict[str, Any] = {}, **kw) -> Incomplete | None: ...
    def indicator_cget(self, entry: str, opt): ...
    def indicator_exists(self, entry: str) -> bool: ...
    def indicator_delete(self, entry: str) -> None: ...
    def indicator_size(self, entry: str) -> int: ...
    def info_anchor(self) -> str: ...
    def info_bbox(self, entry: str) -> tuple[int, int, int, int]: ...
    def info_children(self, entry: str | None = None) -> tuple[str, ...]: ...
    def info_data(self, entry: str) -> Any: ...
    def info_dragsite(self) -> str: ...
    def info_dropsite(self) -> str: ...
    def info_exists(self, entry: str) -> bool: ...
    def info_hidden(self, entry: str) -> bool: ...
    def info_next(self, entry: str) -> str: ...
    def info_parent(self, entry: str) -> str: ...
    def info_prev(self, entry: str) -> str: ...
    def info_selection(self) -> tuple[str, ...]: ...
    def item_cget(self, entry: str, col: int, opt): ...
    def item_configure(self, entry: str, col: int, cnf: dict[str, Any] = {}, **kw) -> Incomplete | None: ...
    def item_create(self, entry: str, col: int, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def item_exists(self, entry: str, col: int) -> bool: ...
    def item_delete(self, entry: str, col: int) -> None: ...
    def entrycget(self, entry: str, opt): ...
    def entryconfigure(self, entry: str, cnf: dict[str, Any] = {}, **kw) -> Incomplete | None: ...
    def nearest(self, y: int) -> str: ...
    def see(self, entry: str) -> None: ...
    def selection_clear(self, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def selection_includes(self, entry: str) -> bool: ...
    def selection_set(self, first: str, last: str | None = None) -> None: ...
    def show_entry(self, entry: str) -> None: ...

class CheckList(TixWidget):
    """The CheckList widget
    displays a list of items to be selected by the user. CheckList acts
    similarly to the Tk checkbutton or radiobutton widgets, except it is
    capable of handling many more items than checkbuttons or radiobuttons.
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def autosetmode(self) -> None:
        """This command calls the setmode method for all the entries in this
        Tree widget: if an entry has no child entries, its mode is set to
        none. Otherwise, if the entry has any hidden child entries, its mode is
        set to open; otherwise its mode is set to close.
        """

    def close(self, entrypath: str) -> None:
        """Close the entry given by entryPath if its mode is close."""

    def getmode(self, entrypath: str) -> str:
        """Returns the current mode of the entry given by entryPath."""

    def open(self, entrypath: str) -> None:
        """Open the entry given by entryPath if its mode is open."""

    def getselection(self, mode: str = "on") -> tuple[str, ...]:
        """Returns a list of items whose status matches status. If status is
        not specified, the list of items in the "on" status will be returned.
        Mode can be on, off, default
        """

    def getstatus(self, entrypath: str) -> str:
        """Returns the current status of entryPath."""

    def setstatus(self, entrypath: str, mode: str = "on") -> None:
        """Sets the status of entryPath to be status. A bitmap will be
        displayed next to the entry its status is on, off or default.
        """

class Tree(TixWidget):
    """Tree - The tixTree widget can be used to display hierarchical
    data in a tree form. The user can adjust
    the view of the tree by opening or closing parts of the tree.
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def autosetmode(self) -> None:
        """This command calls the setmode method for all the entries in this
        Tree widget: if an entry has no child entries, its mode is set to
        none. Otherwise, if the entry has any hidden child entries, its mode is
        set to open; otherwise its mode is set to close.
        """

    def close(self, entrypath: str) -> None:
        """Close the entry given by entryPath if its mode is close."""

    def getmode(self, entrypath: str) -> str:
        """Returns the current mode of the entry given by entryPath."""

    def open(self, entrypath: str) -> None:
        """Open the entry given by entryPath if its mode is open."""

    def setmode(self, entrypath: str, mode: str = "none") -> None:
        """This command is used to indicate whether the entry given by
        entryPath has children entries and whether the children are visible. mode
        must be one of open, close or none. If mode is set to open, a (+)
        indicator is drawn next the entry. If mode is set to close, a (-)
        indicator is drawn next the entry. If mode is set to none, no
        indicators will be drawn for this entry. The default mode is none. The
        open mode indicates the entry has hidden children and this entry can be
        opened by the user. The close mode indicates that all the children of the
        entry are now visible and the entry can be closed by the user.
        """

class TList(TixWidget, tkinter.XView, tkinter.YView):
    """TList - Hierarchy display widget which can be
    used to display data in a tabular format. The list entries of a TList
    widget are similar to the entries in the Tk listbox widget. The main
    differences are (1) the TList widget can display the list entries in a
    two dimensional format and (2) you can use graphical images as well as
    multiple colors and fonts for the list entries.

    Subwidgets - None
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def active_set(self, index: int) -> None: ...
    def active_clear(self) -> None: ...
    def anchor_set(self, index: int) -> None: ...
    def anchor_clear(self) -> None: ...
    def delete(self, from_: int, to: int | None = None) -> None: ...
    def dragsite_set(self, index: int) -> None: ...
    def dragsite_clear(self) -> None: ...
    def dropsite_set(self, index: int) -> None: ...
    def dropsite_clear(self) -> None: ...
    def insert(self, index: int, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def info_active(self) -> int: ...
    def info_anchor(self) -> int: ...
    def info_down(self, index: int) -> int: ...
    def info_left(self, index: int) -> int: ...
    def info_right(self, index: int) -> int: ...
    def info_selection(self) -> tuple[int, ...]: ...
    def info_size(self) -> int: ...
    def info_up(self, index: int) -> int: ...
    def nearest(self, x: int, y: int) -> int: ...
    def see(self, index: int) -> None: ...
    def selection_clear(self, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def selection_includes(self, index: int) -> bool: ...
    def selection_set(self, first: int, last: int | None = None) -> None: ...

class PanedWindow(TixWidget):
    """PanedWindow - Multi-pane container widget
    allows the user to interactively manipulate the sizes of several
    panes. The panes can be arranged either vertically or horizontally.The
    user changes the sizes of the panes by dragging the resize handle
    between two panes.

    Subwidgets       Class
    ----------       -----
    <panes>       g/p widgets added dynamically with the add method.
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, name: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def delete(self, name: str) -> None: ...
    def forget(self, name: str) -> None: ...  # type: ignore[override]
    def panecget(self, entry: str, opt): ...
    def paneconfigure(self, entry: str, cnf: dict[str, Any] = {}, **kw) -> Incomplete | None: ...
    def panes(self) -> list[tkinter.Widget]: ...

class ListNoteBook(TixWidget):
    """A ListNoteBook widget is very similar to the TixNoteBook widget:
    it can be used to display many windows in a limited space using a
    notebook metaphor. The notebook is divided into a stack of pages
    (windows). At one time only one of these pages can be shown.
    The user can navigate through these pages by
    choosing the name of the desired page in the hlist subwidget.
    """

    def __init__(self, master: tkinter.Widget | None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, name: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def page(self, name: str) -> tkinter.Widget: ...
    def pages(self) -> list[tkinter.Widget]: ...
    def raise_page(self, name: str) -> None: ...

class NoteBook(TixWidget):
    """NoteBook - Multi-page container widget (tabbed notebook metaphor).

    Subwidgets       Class
    ----------       -----
    nbframe       NoteBookFrame
    <pages>       page widgets added dynamically with the add method
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def add(self, name: str, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def delete(self, name: str) -> None: ...
    def page(self, name: str) -> tkinter.Widget: ...
    def pages(self) -> list[tkinter.Widget]: ...
    def raise_page(self, name: str) -> None: ...
    def raised(self) -> bool: ...

class InputOnly(TixWidget):
    """InputOnly - Invisible widget. Unix only.

    Subwidgets - None
    """

    def __init__(self, master: tkinter.Widget | None = None, cnf: dict[str, Any] = {}, **kw) -> None: ...

class Form:
    """The Tix Form geometry manager

    Widgets can be arranged by specifying attachments to other widgets.
    See Tix documentation for complete details
    """

    def __setitem__(self, key: str, value: Any) -> None: ...
    def config(self, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def form(self, cnf: dict[str, Any] = {}, **kw) -> None: ...
    def check(self) -> bool: ...
    def forget(self) -> None: ...
    def grid(self, xsize: int = 0, ysize: int = 0) -> tuple[int, int] | None: ...
    def info(self, option: str | None = None): ...
    def slaves(self) -> list[tkinter.Widget]: ...
