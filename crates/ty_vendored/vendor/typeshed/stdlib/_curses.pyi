import sys
from _typeshed import ReadOnlyBuffer, SupportsRead, SupportsWrite
from curses import _ncurses_version
from typing import Any, Final, final, overload
from typing_extensions import TypeAlias

# NOTE: This module is ordinarily only available on Unix, but the windows-curses
# package makes it available on Windows as well with the same contents.

# Handled by PyCurses_ConvertToChtype in _cursesmodule.c.
_ChType: TypeAlias = str | bytes | int

# ACS codes are only initialized after initscr is called
ACS_BBSS: Final[int]
ACS_BLOCK: Final[int]
ACS_BOARD: Final[int]
ACS_BSBS: Final[int]
ACS_BSSB: Final[int]
ACS_BSSS: Final[int]
ACS_BTEE: Final[int]
ACS_BULLET: Final[int]
ACS_CKBOARD: Final[int]
ACS_DARROW: Final[int]
ACS_DEGREE: Final[int]
ACS_DIAMOND: Final[int]
ACS_GEQUAL: Final[int]
ACS_HLINE: Final[int]
ACS_LANTERN: Final[int]
ACS_LARROW: Final[int]
ACS_LEQUAL: Final[int]
ACS_LLCORNER: Final[int]
ACS_LRCORNER: Final[int]
ACS_LTEE: Final[int]
ACS_NEQUAL: Final[int]
ACS_PI: Final[int]
ACS_PLMINUS: Final[int]
ACS_PLUS: Final[int]
ACS_RARROW: Final[int]
ACS_RTEE: Final[int]
ACS_S1: Final[int]
ACS_S3: Final[int]
ACS_S7: Final[int]
ACS_S9: Final[int]
ACS_SBBS: Final[int]
ACS_SBSB: Final[int]
ACS_SBSS: Final[int]
ACS_SSBB: Final[int]
ACS_SSBS: Final[int]
ACS_SSSB: Final[int]
ACS_SSSS: Final[int]
ACS_STERLING: Final[int]
ACS_TTEE: Final[int]
ACS_UARROW: Final[int]
ACS_ULCORNER: Final[int]
ACS_URCORNER: Final[int]
ACS_VLINE: Final[int]
ALL_MOUSE_EVENTS: Final[int]
A_ALTCHARSET: Final[int]
A_ATTRIBUTES: Final[int]
A_BLINK: Final[int]
A_BOLD: Final[int]
A_CHARTEXT: Final[int]
A_COLOR: Final[int]
A_DIM: Final[int]
A_HORIZONTAL: Final[int]
A_INVIS: Final[int]
A_ITALIC: Final[int]
A_LEFT: Final[int]
A_LOW: Final[int]
A_NORMAL: Final[int]
A_PROTECT: Final[int]
A_REVERSE: Final[int]
A_RIGHT: Final[int]
A_STANDOUT: Final[int]
A_TOP: Final[int]
A_UNDERLINE: Final[int]
A_VERTICAL: Final[int]
BUTTON1_CLICKED: Final[int]
BUTTON1_DOUBLE_CLICKED: Final[int]
BUTTON1_PRESSED: Final[int]
BUTTON1_RELEASED: Final[int]
BUTTON1_TRIPLE_CLICKED: Final[int]
BUTTON2_CLICKED: Final[int]
BUTTON2_DOUBLE_CLICKED: Final[int]
BUTTON2_PRESSED: Final[int]
BUTTON2_RELEASED: Final[int]
BUTTON2_TRIPLE_CLICKED: Final[int]
BUTTON3_CLICKED: Final[int]
BUTTON3_DOUBLE_CLICKED: Final[int]
BUTTON3_PRESSED: Final[int]
BUTTON3_RELEASED: Final[int]
BUTTON3_TRIPLE_CLICKED: Final[int]
BUTTON4_CLICKED: Final[int]
BUTTON4_DOUBLE_CLICKED: Final[int]
BUTTON4_PRESSED: Final[int]
BUTTON4_RELEASED: Final[int]
BUTTON4_TRIPLE_CLICKED: Final[int]
# Darwin ncurses doesn't provide BUTTON5_* constants prior to 3.12.10 and 3.13.3
if sys.version_info >= (3, 10):
    if sys.version_info >= (3, 12) or sys.platform != "darwin":
        BUTTON5_PRESSED: Final[int]
        BUTTON5_RELEASED: Final[int]
        BUTTON5_CLICKED: Final[int]
        BUTTON5_DOUBLE_CLICKED: Final[int]
        BUTTON5_TRIPLE_CLICKED: Final[int]
BUTTON_ALT: Final[int]
BUTTON_CTRL: Final[int]
BUTTON_SHIFT: Final[int]
COLOR_BLACK: Final[int]
COLOR_BLUE: Final[int]
COLOR_CYAN: Final[int]
COLOR_GREEN: Final[int]
COLOR_MAGENTA: Final[int]
COLOR_RED: Final[int]
COLOR_WHITE: Final[int]
COLOR_YELLOW: Final[int]
ERR: Final[int]
KEY_A1: Final[int]
KEY_A3: Final[int]
KEY_B2: Final[int]
KEY_BACKSPACE: Final[int]
KEY_BEG: Final[int]
KEY_BREAK: Final[int]
KEY_BTAB: Final[int]
KEY_C1: Final[int]
KEY_C3: Final[int]
KEY_CANCEL: Final[int]
KEY_CATAB: Final[int]
KEY_CLEAR: Final[int]
KEY_CLOSE: Final[int]
KEY_COMMAND: Final[int]
KEY_COPY: Final[int]
KEY_CREATE: Final[int]
KEY_CTAB: Final[int]
KEY_DC: Final[int]
KEY_DL: Final[int]
KEY_DOWN: Final[int]
KEY_EIC: Final[int]
KEY_END: Final[int]
KEY_ENTER: Final[int]
KEY_EOL: Final[int]
KEY_EOS: Final[int]
KEY_EXIT: Final[int]
KEY_F0: Final[int]
KEY_F1: Final[int]
KEY_F10: Final[int]
KEY_F11: Final[int]
KEY_F12: Final[int]
KEY_F13: Final[int]
KEY_F14: Final[int]
KEY_F15: Final[int]
KEY_F16: Final[int]
KEY_F17: Final[int]
KEY_F18: Final[int]
KEY_F19: Final[int]
KEY_F2: Final[int]
KEY_F20: Final[int]
KEY_F21: Final[int]
KEY_F22: Final[int]
KEY_F23: Final[int]
KEY_F24: Final[int]
KEY_F25: Final[int]
KEY_F26: Final[int]
KEY_F27: Final[int]
KEY_F28: Final[int]
KEY_F29: Final[int]
KEY_F3: Final[int]
KEY_F30: Final[int]
KEY_F31: Final[int]
KEY_F32: Final[int]
KEY_F33: Final[int]
KEY_F34: Final[int]
KEY_F35: Final[int]
KEY_F36: Final[int]
KEY_F37: Final[int]
KEY_F38: Final[int]
KEY_F39: Final[int]
KEY_F4: Final[int]
KEY_F40: Final[int]
KEY_F41: Final[int]
KEY_F42: Final[int]
KEY_F43: Final[int]
KEY_F44: Final[int]
KEY_F45: Final[int]
KEY_F46: Final[int]
KEY_F47: Final[int]
KEY_F48: Final[int]
KEY_F49: Final[int]
KEY_F5: Final[int]
KEY_F50: Final[int]
KEY_F51: Final[int]
KEY_F52: Final[int]
KEY_F53: Final[int]
KEY_F54: Final[int]
KEY_F55: Final[int]
KEY_F56: Final[int]
KEY_F57: Final[int]
KEY_F58: Final[int]
KEY_F59: Final[int]
KEY_F6: Final[int]
KEY_F60: Final[int]
KEY_F61: Final[int]
KEY_F62: Final[int]
KEY_F63: Final[int]
KEY_F7: Final[int]
KEY_F8: Final[int]
KEY_F9: Final[int]
KEY_FIND: Final[int]
KEY_HELP: Final[int]
KEY_HOME: Final[int]
KEY_IC: Final[int]
KEY_IL: Final[int]
KEY_LEFT: Final[int]
KEY_LL: Final[int]
KEY_MARK: Final[int]
KEY_MAX: Final[int]
KEY_MESSAGE: Final[int]
KEY_MIN: Final[int]
KEY_MOUSE: Final[int]
KEY_MOVE: Final[int]
KEY_NEXT: Final[int]
KEY_NPAGE: Final[int]
KEY_OPEN: Final[int]
KEY_OPTIONS: Final[int]
KEY_PPAGE: Final[int]
KEY_PREVIOUS: Final[int]
KEY_PRINT: Final[int]
KEY_REDO: Final[int]
KEY_REFERENCE: Final[int]
KEY_REFRESH: Final[int]
KEY_REPLACE: Final[int]
KEY_RESET: Final[int]
KEY_RESIZE: Final[int]
KEY_RESTART: Final[int]
KEY_RESUME: Final[int]
KEY_RIGHT: Final[int]
KEY_SAVE: Final[int]
KEY_SBEG: Final[int]
KEY_SCANCEL: Final[int]
KEY_SCOMMAND: Final[int]
KEY_SCOPY: Final[int]
KEY_SCREATE: Final[int]
KEY_SDC: Final[int]
KEY_SDL: Final[int]
KEY_SELECT: Final[int]
KEY_SEND: Final[int]
KEY_SEOL: Final[int]
KEY_SEXIT: Final[int]
KEY_SF: Final[int]
KEY_SFIND: Final[int]
KEY_SHELP: Final[int]
KEY_SHOME: Final[int]
KEY_SIC: Final[int]
KEY_SLEFT: Final[int]
KEY_SMESSAGE: Final[int]
KEY_SMOVE: Final[int]
KEY_SNEXT: Final[int]
KEY_SOPTIONS: Final[int]
KEY_SPREVIOUS: Final[int]
KEY_SPRINT: Final[int]
KEY_SR: Final[int]
KEY_SREDO: Final[int]
KEY_SREPLACE: Final[int]
KEY_SRESET: Final[int]
KEY_SRIGHT: Final[int]
KEY_SRSUME: Final[int]
KEY_SSAVE: Final[int]
KEY_SSUSPEND: Final[int]
KEY_STAB: Final[int]
KEY_SUNDO: Final[int]
KEY_SUSPEND: Final[int]
KEY_UNDO: Final[int]
KEY_UP: Final[int]
OK: Final[int]
REPORT_MOUSE_POSITION: Final[int]
_C_API: Any
version: Final[bytes]

def baudrate() -> int:
    """Return the output speed of the terminal in bits per second."""

def beep() -> None:
    """Emit a short attention sound."""

def can_change_color() -> bool:
    """Return True if the programmer can change the colors displayed by the terminal."""

def cbreak(flag: bool = True, /) -> None:
    """Enter cbreak mode.

      flag
        If false, the effect is the same as calling nocbreak().

    In cbreak mode (sometimes called "rare" mode) normal tty line buffering is
    turned off and characters are available to be read one by one.  However,
    unlike raw mode, special characters (interrupt, quit, suspend, and flow
    control) retain their effects on the tty driver and calling program.
    Calling first raw() then cbreak() leaves the terminal in cbreak mode.
    """

def color_content(color_number: int, /) -> tuple[int, int, int]:
    """Return the red, green, and blue (RGB) components of the specified color.

      color_number
        The number of the color (0 - (COLORS-1)).

    A 3-tuple is returned, containing the R, G, B values for the given color,
    which will be between 0 (no component) and 1000 (maximum amount of component).
    """

def color_pair(pair_number: int, /) -> int:
    """Return the attribute value for displaying text in the specified color.

      pair_number
        The number of the color pair.

    This attribute value can be combined with A_STANDOUT, A_REVERSE, and the
    other A_* attributes.  pair_number() is the counterpart to this function.
    """

def curs_set(visibility: int, /) -> int:
    """Set the cursor state.

      visibility
        0 for invisible, 1 for normal visible, or 2 for very visible.

    If the terminal supports the visibility requested, the previous cursor
    state is returned; otherwise, an exception is raised.  On many terminals,
    the "visible" mode is an underline cursor and the "very visible" mode is
    a block cursor.
    """

def def_prog_mode() -> None:
    """Save the current terminal mode as the "program" mode.

    The "program" mode is the mode when the running program is using curses.

    Subsequent calls to reset_prog_mode() will restore this mode.
    """

def def_shell_mode() -> None:
    """Save the current terminal mode as the "shell" mode.

    The "shell" mode is the mode when the running program is not using curses.

    Subsequent calls to reset_shell_mode() will restore this mode.
    """

def delay_output(ms: int, /) -> None:
    """Insert a pause in output.

    ms
      Duration in milliseconds.
    """

def doupdate() -> None:
    """Update the physical screen to match the virtual screen."""

def echo(flag: bool = True, /) -> None:
    """Enter echo mode.

      flag
        If false, the effect is the same as calling noecho().

    In echo mode, each character input is echoed to the screen as it is entered.
    """

def endwin() -> None:
    """De-initialize the library, and return terminal to normal status."""

def erasechar() -> bytes:
    """Return the user's current erase character."""

def filter() -> None: ...
def flash() -> None:
    """Flash the screen.

    That is, change it to reverse-video and then change it back in a short interval.
    """

def flushinp() -> None:
    """Flush all input buffers.

    This throws away any typeahead that has been typed by the user and has not
    yet been processed by the program.
    """

def get_escdelay() -> int:
    """Gets the curses ESCDELAY setting.

    Gets the number of milliseconds to wait after reading an escape character,
    to distinguish between an individual escape character entered on the
    keyboard from escape sequences sent by cursor and function keys.
    """

def get_tabsize() -> int:
    """Gets the curses TABSIZE setting.

    Gets the number of columns used by the curses library when converting a tab
    character to spaces as it adds the tab to a window.
    """

def getmouse() -> tuple[int, int, int, int, int]:
    """Retrieve the queued mouse event.

    After getch() returns KEY_MOUSE to signal a mouse event, this function
    returns a 5-tuple (id, x, y, z, bstate).
    """

def getsyx() -> tuple[int, int]:
    """Return the current coordinates of the virtual screen cursor.

    Return a (y, x) tuple.  If leaveok is currently true, return (-1, -1).
    """

def getwin(file: SupportsRead[bytes], /) -> window:
    """Read window related data stored in the file by an earlier putwin() call.

    The routine then creates and initializes a new window using that data,
    returning the new window object.
    """

def halfdelay(tenths: int, /) -> None:
    """Enter half-delay mode.

      tenths
        Maximal blocking delay in tenths of seconds (1 - 255).

    Use nocbreak() to leave half-delay mode.
    """

def has_colors() -> bool:
    """Return True if the terminal can display colors; otherwise, return False."""

if sys.version_info >= (3, 10):
    def has_extended_color_support() -> bool:
        """Return True if the module supports extended colors; otherwise, return False.

        Extended color support allows more than 256 color-pairs for terminals
        that support more than 16 colors (e.g. xterm-256color).
        """

if sys.version_info >= (3, 14):
    def assume_default_colors(fg: int, bg: int, /) -> None:
        """Allow use of default values for colors on terminals supporting this feature.

        Assign terminal default foreground/background colors to color number -1.
        Change the definition of the color-pair 0 to (fg, bg).

        Use this to support transparency in your application.
        """

def has_ic() -> bool:
    """Return True if the terminal has insert- and delete-character capabilities."""

def has_il() -> bool:
    """Return True if the terminal has insert- and delete-line capabilities."""

def has_key(key: int, /) -> bool:
    """Return True if the current terminal type recognizes a key with that value.

    key
      Key number.
    """

def init_color(color_number: int, r: int, g: int, b: int, /) -> None:
    """Change the definition of a color.

      color_number
        The number of the color to be changed (0 - (COLORS-1)).
      r
        Red component (0 - 1000).
      g
        Green component (0 - 1000).
      b
        Blue component (0 - 1000).

    When init_color() is used, all occurrences of that color on the screen
    immediately change to the new definition.  This function is a no-op on
    most terminals; it is active only if can_change_color() returns true.
    """

def init_pair(pair_number: int, fg: int, bg: int, /) -> None:
    """Change the definition of a color-pair.

      pair_number
        The number of the color-pair to be changed (1 - (COLOR_PAIRS-1)).
      fg
        Foreground color number (-1 - (COLORS-1)).
      bg
        Background color number (-1 - (COLORS-1)).

    If the color-pair was previously initialized, the screen is refreshed and
    all occurrences of that color-pair are changed to the new definition.
    """

def initscr() -> window:
    """Initialize the library.

    Return a WindowObject which represents the whole screen.
    """

def intrflush(flag: bool, /) -> None: ...
def is_term_resized(nlines: int, ncols: int, /) -> bool:
    """Return True if resize_term() would modify the window structure, False otherwise.

    nlines
      Height.
    ncols
      Width.
    """

def isendwin() -> bool:
    """Return True if endwin() has been called."""

def keyname(key: int, /) -> bytes:
    """Return the name of specified key.

    key
      Key number.
    """

def killchar() -> bytes:
    """Return the user's current line kill character."""

def longname() -> bytes:
    """Return the terminfo long name field describing the current terminal.

    The maximum length of a verbose description is 128 characters.  It is defined
    only after the call to initscr().
    """

def meta(yes: bool, /) -> None:
    """Enable/disable meta keys.

    If yes is True, allow 8-bit characters to be input.  If yes is False,
    allow only 7-bit characters.
    """

def mouseinterval(interval: int, /) -> None:
    """Set and retrieve the maximum time between press and release in a click.

      interval
        Time in milliseconds.

    Set the maximum time that can elapse between press and release events in
    order for them to be recognized as a click, and return the previous interval
    value.
    """

def mousemask(newmask: int, /) -> tuple[int, int]:
    """Set the mouse events to be reported, and return a tuple (availmask, oldmask).

    Return a tuple (availmask, oldmask).  availmask indicates which of the
    specified mouse events can be reported; on complete failure it returns 0.
    oldmask is the previous value of the given window's mouse event mask.
    If this function is never called, no mouse events are ever reported.
    """

def napms(ms: int, /) -> int:
    """Sleep for specified time.

    ms
      Duration in milliseconds.
    """

def newpad(nlines: int, ncols: int, /) -> window:
    """Create and return a pointer to a new pad data structure.

    nlines
      Height.
    ncols
      Width.
    """

def newwin(nlines: int, ncols: int, begin_y: int = 0, begin_x: int = 0, /) -> window:
    """newwin(nlines, ncols, [begin_y=0, begin_x=0])
    Return a new window.

      nlines
        Height.
      ncols
        Width.
      begin_y
        Top side y-coordinate.
      begin_x
        Left side x-coordinate.

    By default, the window will extend from the specified position to the lower
    right corner of the screen.
    """

def nl(flag: bool = True, /) -> None:
    """Enter newline mode.

      flag
        If false, the effect is the same as calling nonl().

    This mode translates the return key into newline on input, and translates
    newline into return and line-feed on output.  Newline mode is initially on.
    """

def nocbreak() -> None:
    """Leave cbreak mode.

    Return to normal "cooked" mode with line buffering.
    """

def noecho() -> None:
    """Leave echo mode.

    Echoing of input characters is turned off.
    """

def nonl() -> None:
    """Leave newline mode.

    Disable translation of return into newline on input, and disable low-level
    translation of newline into newline/return on output.
    """

def noqiflush() -> None:
    """Disable queue flushing.

    When queue flushing is disabled, normal flush of input and output queues
    associated with the INTR, QUIT and SUSP characters will not be done.
    """

def noraw() -> None:
    """Leave raw mode.

    Return to normal "cooked" mode with line buffering.
    """

def pair_content(pair_number: int, /) -> tuple[int, int]:
    """Return a tuple (fg, bg) containing the colors for the requested color pair.

    pair_number
      The number of the color pair (0 - (COLOR_PAIRS-1)).
    """

def pair_number(attr: int, /) -> int:
    """Return the number of the color-pair set by the specified attribute value.

    color_pair() is the counterpart to this function.
    """

def putp(string: ReadOnlyBuffer, /) -> None:
    """Emit the value of a specified terminfo capability for the current terminal.

    Note that the output of putp() always goes to standard output.
    """

def qiflush(flag: bool = True, /) -> None:
    """Enable queue flushing.

      flag
        If false, the effect is the same as calling noqiflush().

    If queue flushing is enabled, all output in the display driver queue
    will be flushed when the INTR, QUIT and SUSP characters are read.
    """

def raw(flag: bool = True, /) -> None:
    """Enter raw mode.

      flag
        If false, the effect is the same as calling noraw().

    In raw mode, normal line buffering and processing of interrupt, quit,
    suspend, and flow control keys are turned off; characters are presented to
    curses input functions one by one.
    """

def reset_prog_mode() -> None:
    """Restore the terminal to "program" mode, as previously saved by def_prog_mode()."""

def reset_shell_mode() -> None:
    """Restore the terminal to "shell" mode, as previously saved by def_shell_mode()."""

def resetty() -> None:
    """Restore terminal mode."""

def resize_term(nlines: int, ncols: int, /) -> None:
    """Backend function used by resizeterm(), performing most of the work.

      nlines
        Height.
      ncols
        Width.

    When resizing the windows, resize_term() blank-fills the areas that are
    extended.  The calling application should fill in these areas with appropriate
    data.  The resize_term() function attempts to resize all windows.  However,
    due to the calling convention of pads, it is not possible to resize these
    without additional interaction with the application.
    """

def resizeterm(nlines: int, ncols: int, /) -> None:
    """Resize the standard and current windows to the specified dimensions.

      nlines
        Height.
      ncols
        Width.

    Adjusts other bookkeeping data used by the curses library that record the
    window dimensions (in particular the SIGWINCH handler).
    """

def savetty() -> None:
    """Save terminal mode."""

def set_escdelay(ms: int, /) -> None:
    """Sets the curses ESCDELAY setting.

      ms
        length of the delay in milliseconds.

    Sets the number of milliseconds to wait after reading an escape character,
    to distinguish between an individual escape character entered on the
    keyboard from escape sequences sent by cursor and function keys.
    """

def set_tabsize(size: int, /) -> None:
    """Sets the curses TABSIZE setting.

      size
        rendered cell width of a tab character.

    Sets the number of columns used by the curses library when converting a tab
    character to spaces as it adds the tab to a window.
    """

def setsyx(y: int, x: int, /) -> None:
    """Set the virtual screen cursor.

      y
        Y-coordinate.
      x
        X-coordinate.

    If y and x are both -1, then leaveok is set.
    """

def setupterm(term: str | None = None, fd: int = -1) -> None:
    """Initialize the terminal.

    term
      Terminal name.
      If omitted, the value of the TERM environment variable will be used.
    fd
      File descriptor to which any initialization sequences will be sent.
      If not supplied, the file descriptor for sys.stdout will be used.
    """

def start_color() -> None:
    """Initializes eight basic colors and global variables COLORS and COLOR_PAIRS.

    Must be called if the programmer wants to use colors, and before any other
    color manipulation routine is called.  It is good practice to call this
    routine right after initscr().

    It also restores the colors on the terminal to the values they had when the
    terminal was just turned on.
    """

def termattrs() -> int:
    """Return a logical OR of all video attributes supported by the terminal."""

def termname() -> bytes:
    """Return the value of the environment variable TERM, truncated to 14 characters."""

def tigetflag(capname: str, /) -> int:
    """Return the value of the Boolean capability.

      capname
        The terminfo capability name.

    The value -1 is returned if capname is not a Boolean capability, or 0 if
    it is canceled or absent from the terminal description.
    """

def tigetnum(capname: str, /) -> int:
    """Return the value of the numeric capability.

      capname
        The terminfo capability name.

    The value -2 is returned if capname is not a numeric capability, or -1 if
    it is canceled or absent from the terminal description.
    """

def tigetstr(capname: str, /) -> bytes | None:
    """Return the value of the string capability.

      capname
        The terminfo capability name.

    None is returned if capname is not a string capability, or is canceled or
    absent from the terminal description.
    """

def tparm(
    str: ReadOnlyBuffer,
    i1: int = 0,
    i2: int = 0,
    i3: int = 0,
    i4: int = 0,
    i5: int = 0,
    i6: int = 0,
    i7: int = 0,
    i8: int = 0,
    i9: int = 0,
    /,
) -> bytes:
    """Instantiate the specified byte string with the supplied parameters.

    str
      Parameterized byte string obtained from the terminfo database.
    """

def typeahead(fd: int, /) -> None:
    """Specify that the file descriptor fd be used for typeahead checking.

      fd
        File descriptor.

    If fd is -1, then no typeahead checking is done.
    """

def unctrl(ch: _ChType, /) -> bytes:
    """Return a string which is a printable representation of the character ch.

    Control characters are displayed as a caret followed by the character,
    for example as ^C.  Printing characters are left as they are.
    """

def unget_wch(ch: int | str, /) -> None:
    """Push ch so the next get_wch() will return it."""

def ungetch(ch: _ChType, /) -> None:
    """Push ch so the next getch() will return it."""

def ungetmouse(id: int, x: int, y: int, z: int, bstate: int, /) -> None:
    """Push a KEY_MOUSE event onto the input queue.

    The following getmouse() will return the given state data.
    """

def update_lines_cols() -> None: ...
def use_default_colors() -> None:
    """Equivalent to assume_default_colors(-1, -1)."""

def use_env(flag: bool, /) -> None:
    """Use environment variables LINES and COLUMNS.

    If used, this function should be called before initscr() or newterm() are
    called.

    When flag is False, the values of lines and columns specified in the terminfo
    database will be used, even if environment variables LINES and COLUMNS (used
    by default) are set, or if curses is running in a window (in which case
    default behavior would be to use the window size if LINES and COLUMNS are
    not set).
    """

class error(Exception): ...

@final
class window:  # undocumented
    encoding: str
    @overload
    def addch(self, ch: _ChType, attr: int = ...) -> None:
        """addch([y, x,] ch, [attr=_curses.A_NORMAL])
        Paint the character.

          y
            Y-coordinate.
          x
            X-coordinate.
          ch
            Character to add.
          attr
            Attributes for the character.

        Paint character ch at (y, x) with attributes attr,
        overwriting any character previously painted at that location.
        By default, the character position and attributes are the
        current settings for the window object.
        """

    @overload
    def addch(self, y: int, x: int, ch: _ChType, attr: int = ...) -> None: ...
    @overload
    def addnstr(self, str: str, n: int, attr: int = ...) -> None:
        """addnstr([y, x,] str, n, [attr])
        Paint at most n characters of the string.

          y
            Y-coordinate.
          x
            X-coordinate.
          str
            String to add.
          n
            Maximal number of characters.
          attr
            Attributes for characters.

        Paint at most n characters of the string str at (y, x) with
        attributes attr, overwriting anything previously on the display.
        By default, the character position and attributes are the
        current settings for the window object.
        """

    @overload
    def addnstr(self, y: int, x: int, str: str, n: int, attr: int = ...) -> None: ...
    @overload
    def addstr(self, str: str, attr: int = ...) -> None:
        """addstr([y, x,] str, [attr])
        Paint the string.

          y
            Y-coordinate.
          x
            X-coordinate.
          str
            String to add.
          attr
            Attributes for characters.

        Paint the string str at (y, x) with attributes attr,
        overwriting anything previously on the display.
        By default, the character position and attributes are the
        current settings for the window object.
        """

    @overload
    def addstr(self, y: int, x: int, str: str, attr: int = ...) -> None: ...
    def attroff(self, attr: int, /) -> None:
        """Remove attribute attr from the "background" set."""

    def attron(self, attr: int, /) -> None:
        """Add attribute attr to the "background" set."""

    def attrset(self, attr: int, /) -> None:
        """Set the "background" set of attributes."""

    def bkgd(self, ch: _ChType, attr: int = 0, /) -> None:
        """Set the background property of the window.

        ch
          Background character.
        attr
          Background attributes.
        """

    def bkgdset(self, ch: _ChType, attr: int = 0, /) -> None:
        """Set the window's background.

        ch
          Background character.
        attr
          Background attributes.
        """

    def border(
        self,
        ls: _ChType = ...,
        rs: _ChType = ...,
        ts: _ChType = ...,
        bs: _ChType = ...,
        tl: _ChType = ...,
        tr: _ChType = ...,
        bl: _ChType = ...,
        br: _ChType = ...,
    ) -> None:
        """Draw a border around the edges of the window.

          ls
            Left side.
          rs
            Right side.
          ts
            Top side.
          bs
            Bottom side.
          tl
            Upper-left corner.
          tr
            Upper-right corner.
          bl
            Bottom-left corner.
          br
            Bottom-right corner.

        Each parameter specifies the character to use for a specific part of the
        border.  The characters can be specified as integers or as one-character
        strings.  A 0 value for any parameter will cause the default character to be
        used for that parameter.
        """

    @overload
    def box(self) -> None:
        """box([verch=0, horch=0])
        Draw a border around the edges of the window.

          verch
            Left and right side.
          horch
            Top and bottom side.

        Similar to border(), but both ls and rs are verch and both ts and bs are
        horch.  The default corner characters are always used by this function.
        """

    @overload
    def box(self, vertch: _ChType = 0, horch: _ChType = 0) -> None: ...
    @overload
    def chgat(self, attr: int) -> None:
        """chgat([y, x,] [n=-1,] attr)
        Set the attributes of characters.

          y
            Y-coordinate.
          x
            X-coordinate.
          n
            Number of characters.
          attr
            Attributes for characters.

        Set the attributes of num characters at the current cursor position, or at
        position (y, x) if supplied.  If no value of num is given or num = -1, the
        attribute will be set on all the characters to the end of the line.  This
        function does not move the cursor.  The changed line will be touched using
        the touchline() method so that the contents will be redisplayed by the next
        window refresh.
        """

    @overload
    def chgat(self, num: int, attr: int) -> None: ...
    @overload
    def chgat(self, y: int, x: int, attr: int) -> None: ...
    @overload
    def chgat(self, y: int, x: int, num: int, attr: int) -> None: ...
    def clear(self) -> None: ...
    def clearok(self, yes: int) -> None: ...
    def clrtobot(self) -> None: ...
    def clrtoeol(self) -> None: ...
    def cursyncup(self) -> None: ...
    @overload
    def delch(self) -> None:
        """delch([y, x])
        Delete any character at (y, x).

          y
            Y-coordinate.
          x
            X-coordinate.
        """

    @overload
    def delch(self, y: int, x: int) -> None: ...
    def deleteln(self) -> None: ...
    @overload
    def derwin(self, begin_y: int, begin_x: int) -> window:
        """derwin([nlines=0, ncols=0,] begin_y, begin_x)
        Create a sub-window (window-relative coordinates).

          nlines
            Height.
          ncols
            Width.
          begin_y
            Top side y-coordinate.
          begin_x
            Left side x-coordinate.

        derwin() is the same as calling subwin(), except that begin_y and begin_x
        are relative to the origin of the window, rather than relative to the entire
        screen.
        """

    @overload
    def derwin(self, nlines: int, ncols: int, begin_y: int, begin_x: int) -> window: ...
    def echochar(self, ch: _ChType, attr: int = 0, /) -> None:
        """Add character ch with attribute attr, and refresh.

        ch
          Character to add.
        attr
          Attributes for the character.
        """

    def enclose(self, y: int, x: int, /) -> bool:
        """Return True if the screen-relative coordinates are enclosed by the window.

        y
          Y-coordinate.
        x
          X-coordinate.
        """

    def erase(self) -> None: ...
    def getbegyx(self) -> tuple[int, int]: ...
    def getbkgd(self) -> tuple[int, int]:
        """Return the window's current background character/attribute pair."""

    @overload
    def getch(self) -> int:
        """getch([y, x])
        Get a character code from terminal keyboard.

          y
            Y-coordinate.
          x
            X-coordinate.

        The integer returned does not have to be in ASCII range: function keys,
        keypad keys and so on return numbers higher than 256.  In no-delay mode, -1
        is returned if there is no input, else getch() waits until a key is pressed.
        """

    @overload
    def getch(self, y: int, x: int) -> int: ...
    @overload
    def get_wch(self) -> int | str:
        """get_wch([y, x])
        Get a wide character from terminal keyboard.

          y
            Y-coordinate.
          x
            X-coordinate.

        Return a character for most keys, or an integer for function keys,
        keypad keys, and other special keys.
        """

    @overload
    def get_wch(self, y: int, x: int) -> int | str: ...
    @overload
    def getkey(self) -> str:
        """getkey([y, x])
        Get a character (string) from terminal keyboard.

          y
            Y-coordinate.
          x
            X-coordinate.

        Returning a string instead of an integer, as getch() does.  Function keys,
        keypad keys and other special keys return a multibyte string containing the
        key name.  In no-delay mode, an exception is raised if there is no input.
        """

    @overload
    def getkey(self, y: int, x: int) -> str: ...
    def getmaxyx(self) -> tuple[int, int]: ...
    def getparyx(self) -> tuple[int, int]: ...
    @overload
    def getstr(self) -> bytes:
        """getstr([[y, x,] n=2047])
        Read a string from the user, with primitive line editing capacity.

          y
            Y-coordinate.
          x
            X-coordinate.
          n
            Maximal number of characters.
        """

    @overload
    def getstr(self, n: int) -> bytes: ...
    @overload
    def getstr(self, y: int, x: int) -> bytes: ...
    @overload
    def getstr(self, y: int, x: int, n: int) -> bytes: ...
    def getyx(self) -> tuple[int, int]: ...
    @overload
    def hline(self, ch: _ChType, n: int) -> None:
        """hline([y, x,] ch, n, [attr=_curses.A_NORMAL])
        Display a horizontal line.

          y
            Starting Y-coordinate.
          x
            Starting X-coordinate.
          ch
            Character to draw.
          n
            Line length.
          attr
            Attributes for the characters.
        """

    @overload
    def hline(self, y: int, x: int, ch: _ChType, n: int) -> None: ...
    def idcok(self, flag: bool) -> None: ...
    def idlok(self, yes: bool) -> None: ...
    def immedok(self, flag: bool) -> None: ...
    @overload
    def inch(self) -> int:
        """inch([y, x])
        Return the character at the given position in the window.

          y
            Y-coordinate.
          x
            X-coordinate.

        The bottom 8 bits are the character proper, and upper bits are the attributes.
        """

    @overload
    def inch(self, y: int, x: int) -> int: ...
    @overload
    def insch(self, ch: _ChType, attr: int = ...) -> None:
        """insch([y, x,] ch, [attr=_curses.A_NORMAL])
        Insert a character before the current or specified position.

          y
            Y-coordinate.
          x
            X-coordinate.
          ch
            Character to insert.
          attr
            Attributes for the character.

        All characters to the right of the cursor are shifted one position right, with
        the rightmost characters on the line being lost.
        """

    @overload
    def insch(self, y: int, x: int, ch: _ChType, attr: int = ...) -> None: ...
    def insdelln(self, nlines: int) -> None: ...
    def insertln(self) -> None: ...
    @overload
    def insnstr(self, str: str, n: int, attr: int = ...) -> None:
        """insnstr([y, x,] str, n, [attr])
        Insert at most n characters of the string.

          y
            Y-coordinate.
          x
            X-coordinate.
          str
            String to insert.
          n
            Maximal number of characters.
          attr
            Attributes for characters.

        Insert a character string (as many characters as will fit on the line)
        before the character under the cursor, up to n characters.  If n is zero
        or negative, the entire string is inserted.  All characters to the right
        of the cursor are shifted right, with the rightmost characters on the line
        being lost.  The cursor position does not change (after moving to y, x, if
        specified).
        """

    @overload
    def insnstr(self, y: int, x: int, str: str, n: int, attr: int = ...) -> None: ...
    @overload
    def insstr(self, str: str, attr: int = ...) -> None:
        """insstr([y, x,] str, [attr])
        Insert the string before the current or specified position.

          y
            Y-coordinate.
          x
            X-coordinate.
          str
            String to insert.
          attr
            Attributes for characters.

        Insert a character string (as many characters as will fit on the line)
        before the character under the cursor.  All characters to the right of
        the cursor are shifted right, with the rightmost characters on the line
        being lost.  The cursor position does not change (after moving to y, x,
        if specified).
        """

    @overload
    def insstr(self, y: int, x: int, str: str, attr: int = ...) -> None: ...
    @overload
    def instr(self, n: int = 2047) -> bytes:
        """instr([y, x,] n=2047)
        Return a string of characters, extracted from the window.

          y
            Y-coordinate.
          x
            X-coordinate.
          n
            Maximal number of characters.

        Return a string of characters, extracted from the window starting at the
        current cursor position, or at y, x if specified.  Attributes are stripped
        from the characters.  If n is specified, instr() returns a string at most
        n characters long (exclusive of the trailing NUL).
        """

    @overload
    def instr(self, y: int, x: int, n: int = 2047) -> bytes: ...
    def is_linetouched(self, line: int, /) -> bool:
        """Return True if the specified line was modified, otherwise return False.

          line
            Line number.

        Raise a curses.error exception if line is not valid for the given window.
        """

    def is_wintouched(self) -> bool: ...
    def keypad(self, yes: bool, /) -> None: ...
    def leaveok(self, yes: bool) -> None: ...
    def move(self, new_y: int, new_x: int) -> None: ...
    def mvderwin(self, y: int, x: int) -> None: ...
    def mvwin(self, new_y: int, new_x: int) -> None: ...
    def nodelay(self, yes: bool) -> None: ...
    def notimeout(self, yes: bool) -> None: ...
    @overload
    def noutrefresh(self) -> None:
        """noutrefresh([pminrow, pmincol, sminrow, smincol, smaxrow, smaxcol])
        Mark for refresh but wait.

        This function updates the data structure representing the desired state of the
        window, but does not force an update of the physical screen.  To accomplish
        that, call doupdate().
        """

    @overload
    def noutrefresh(self, pminrow: int, pmincol: int, sminrow: int, smincol: int, smaxrow: int, smaxcol: int) -> None: ...
    @overload
    def overlay(self, destwin: window) -> None:
        """overlay(destwin, [sminrow, smincol, dminrow, dmincol, dmaxrow, dmaxcol])
        Overlay the window on top of destwin.

        The windows need not be the same size, only the overlapping region is copied.
        This copy is non-destructive, which means that the current background
        character does not overwrite the old contents of destwin.

        To get fine-grained control over the copied region, the second form of
        overlay() can be used.  sminrow and smincol are the upper-left coordinates
        of the source window, and the other variables mark a rectangle in the
        destination window.
        """

    @overload
    def overlay(
        self, destwin: window, sminrow: int, smincol: int, dminrow: int, dmincol: int, dmaxrow: int, dmaxcol: int
    ) -> None: ...
    @overload
    def overwrite(self, destwin: window) -> None:
        """overwrite(destwin, [sminrow, smincol, dminrow, dmincol, dmaxrow,
                  dmaxcol])
        Overwrite the window on top of destwin.

        The windows need not be the same size, in which case only the overlapping
        region is copied.  This copy is destructive, which means that the current
        background character overwrites the old contents of destwin.

        To get fine-grained control over the copied region, the second form of
        overwrite() can be used. sminrow and smincol are the upper-left coordinates
        of the source window, the other variables mark a rectangle in the destination
        window.
        """

    @overload
    def overwrite(
        self, destwin: window, sminrow: int, smincol: int, dminrow: int, dmincol: int, dmaxrow: int, dmaxcol: int
    ) -> None: ...
    def putwin(self, file: SupportsWrite[bytes], /) -> None:
        """Write all data associated with the window into the provided file object.

        This information can be later retrieved using the getwin() function.
        """

    def redrawln(self, beg: int, num: int, /) -> None:
        """Mark the specified lines corrupted.

          beg
            Starting line number.
          num
            The number of lines.

        They should be completely redrawn on the next refresh() call.
        """

    def redrawwin(self) -> None: ...
    @overload
    def refresh(self) -> None:
        """refresh([pminrow, pmincol, sminrow, smincol, smaxrow, smaxcol])
        Update the display immediately.

        Synchronize actual screen with previous drawing/deleting methods.
        The 6 optional arguments can only be specified when the window is a pad
        created with newpad().  The additional parameters are needed to indicate
        what part of the pad and screen are involved.  pminrow and pmincol specify
        the upper left-hand corner of the rectangle to be displayed in the pad.
        sminrow, smincol, smaxrow, and smaxcol specify the edges of the rectangle to
        be displayed on the screen.  The lower right-hand corner of the rectangle to
        be displayed in the pad is calculated from the screen coordinates, since the
        rectangles must be the same size.  Both rectangles must be entirely contained
        within their respective structures.  Negative values of pminrow, pmincol,
        sminrow, or smincol are treated as if they were zero.
        """

    @overload
    def refresh(self, pminrow: int, pmincol: int, sminrow: int, smincol: int, smaxrow: int, smaxcol: int) -> None: ...
    def resize(self, nlines: int, ncols: int) -> None: ...
    def scroll(self, lines: int = 1) -> None:
        """scroll([lines=1])
        Scroll the screen or scrolling region.

          lines
            Number of lines to scroll.

        Scroll upward if the argument is positive and downward if it is negative.
        """

    def scrollok(self, flag: bool) -> None: ...
    def setscrreg(self, top: int, bottom: int, /) -> None:
        """Define a software scrolling region.

          top
            First line number.
          bottom
            Last line number.

        All scrolling actions will take place in this region.
        """

    def standend(self) -> None: ...
    def standout(self) -> None: ...
    @overload
    def subpad(self, begin_y: int, begin_x: int) -> window:
        """subwin([nlines=0, ncols=0,] begin_y, begin_x)
        Create a sub-window (screen-relative coordinates).

          nlines
            Height.
          ncols
            Width.
          begin_y
            Top side y-coordinate.
          begin_x
            Left side x-coordinate.

        By default, the sub-window will extend from the specified position to the
        lower right corner of the window.
        """

    @overload
    def subpad(self, nlines: int, ncols: int, begin_y: int, begin_x: int) -> window: ...
    @overload
    def subwin(self, begin_y: int, begin_x: int) -> window:
        """subwin([nlines=0, ncols=0,] begin_y, begin_x)
        Create a sub-window (screen-relative coordinates).

          nlines
            Height.
          ncols
            Width.
          begin_y
            Top side y-coordinate.
          begin_x
            Left side x-coordinate.

        By default, the sub-window will extend from the specified position to the
        lower right corner of the window.
        """

    @overload
    def subwin(self, nlines: int, ncols: int, begin_y: int, begin_x: int) -> window: ...
    def syncdown(self) -> None: ...
    def syncok(self, flag: bool) -> None: ...
    def syncup(self) -> None: ...
    def timeout(self, delay: int) -> None: ...
    def touchline(self, start: int, count: int, changed: bool = True) -> None:
        """touchline(start, count, [changed=True])
        Pretend count lines have been changed, starting with line start.

        If changed is supplied, it specifies whether the affected lines are marked
        as having been changed (changed=True) or unchanged (changed=False).
        """

    def touchwin(self) -> None: ...
    def untouchwin(self) -> None: ...
    @overload
    def vline(self, ch: _ChType, n: int) -> None:
        """vline([y, x,] ch, n, [attr=_curses.A_NORMAL])
        Display a vertical line.

          y
            Starting Y-coordinate.
          x
            Starting X-coordinate.
          ch
            Character to draw.
          n
            Line length.
          attr
            Attributes for the character.
        """

    @overload
    def vline(self, y: int, x: int, ch: _ChType, n: int) -> None: ...

ncurses_version: _ncurses_version
