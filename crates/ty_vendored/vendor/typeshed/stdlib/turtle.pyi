"""
Imagine a robotic turtle starting at (0, 0) in the x-y plane.
After an `import turtle`, give it the command `turtle.forward(15)`, and it
moves (on-screen!) 15 pixels in the direction it is facing, drawing a line as
it moves. Give it the command `turtle.right(25)`, and it rotates in-place 25
degrees clockwise.

Turtle graphics is an implementation of the drawing tools introduced in Logo
in 1967. It was created as an educational tool, and its instant, visible
feedback makes it an effective way for learners to encounter programming
concepts. It is also a convenient way to produce simple graphical output
without bringing in external libraries.
"""

import sys
from _typeshed import StrPath
from collections.abc import Callable, Generator, Sequence
from contextlib import contextmanager
from tkinter import Canvas, Frame, Misc, PhotoImage, Scrollbar
from typing import Any, ClassVar, Literal, TypeAlias, TypedDict, overload, type_check_only
from typing_extensions import Self, deprecated, disjoint_base

__all__ = [
    "ScrolledCanvas",
    "TurtleScreen",
    "Screen",
    "RawTurtle",
    "Turtle",
    "RawPen",
    "Pen",
    "Shape",
    "Vec2D",
    "addshape",
    "bgcolor",
    "bgpic",
    "bye",
    "clearscreen",
    "colormode",
    "delay",
    "exitonclick",
    "getcanvas",
    "getshapes",
    "listen",
    "mainloop",
    "mode",
    "numinput",
    "onkey",
    "onkeypress",
    "onkeyrelease",
    "onscreenclick",
    "ontimer",
    "register_shape",
    "resetscreen",
    "screensize",
    "setup",
    "setworldcoordinates",
    "textinput",
    "title",
    "tracer",
    "turtles",
    "update",
    "window_height",
    "window_width",
    "back",
    "backward",
    "begin_fill",
    "begin_poly",
    "bk",
    "circle",
    "clear",
    "clearstamp",
    "clearstamps",
    "clone",
    "color",
    "degrees",
    "distance",
    "dot",
    "down",
    "end_fill",
    "end_poly",
    "fd",
    "fillcolor",
    "filling",
    "forward",
    "get_poly",
    "getpen",
    "getscreen",
    "get_shapepoly",
    "getturtle",
    "goto",
    "heading",
    "hideturtle",
    "home",
    "ht",
    "isdown",
    "isvisible",
    "left",
    "lt",
    "onclick",
    "ondrag",
    "onrelease",
    "pd",
    "pen",
    "pencolor",
    "pendown",
    "pensize",
    "penup",
    "pos",
    "position",
    "pu",
    "radians",
    "right",
    "reset",
    "resizemode",
    "rt",
    "seth",
    "setheading",
    "setpos",
    "setposition",
    "setundobuffer",
    "setx",
    "sety",
    "shape",
    "shapesize",
    "shapetransform",
    "shearfactor",
    "showturtle",
    "speed",
    "st",
    "stamp",
    "tilt",
    "tiltangle",
    "towards",
    "turtlesize",
    "undo",
    "undobufferentries",
    "up",
    "width",
    "write",
    "xcor",
    "ycor",
    "write_docstringdict",
    "done",
    "Terminator",
]

if sys.version_info >= (3, 14):
    __all__ += ["fill", "no_animation", "poly", "save"]

if sys.version_info >= (3, 12):
    __all__ += ["teleport"]

if sys.version_info < (3, 13):
    __all__ += ["settiltangle"]

# Note: '_Color' is the alias we use for arguments and _AnyColor is the
# alias we use for return types. Really, these two aliases should be the
# same, but as per the "no union returns" typeshed policy, we'll return
# Any instead.
_Color: TypeAlias = str | tuple[float, float, float]
_AnyColor: TypeAlias = Any

@type_check_only
class _PenState(TypedDict):
    shown: bool
    pendown: bool
    pencolor: _Color
    fillcolor: _Color
    pensize: int
    speed: int
    resizemode: Literal["auto", "user", "noresize"]
    stretchfactor: tuple[float, float]
    shearfactor: float
    outline: int
    tilt: float

_Speed: TypeAlias = str | float
_PolygonCoords: TypeAlias = Sequence[tuple[float, float]]

if sys.version_info >= (3, 12):
    class Vec2D(tuple[float, float]):
        """A two-dimensional vector class, used as a helper class for
        implementing turtle graphics. May be useful for turtle graphics
        programs too. Derived from tuple, so a vector is a tuple!

        Provides (for a, b vectors, k number):
           a + b: vector addition
           a - b: vector subtraction
           a * b: inner product
           k * a and a * k: multiplication with scalar
           abs(a): absolute value of a
           a.rotate(angle): rotation
        """

        def __new__(cls, x: float, y: float) -> Self: ...
        def __add__(self, other: tuple[float, float]) -> Vec2D: ...  # type: ignore[override]

        @overload  # type: ignore[override]
        def __mul__(self, other: Vec2D) -> float: ...
        @overload
        def __mul__(self, other: float) -> Vec2D: ...

        def __rmul__(self, other: float) -> Vec2D: ...  # type: ignore[override]
        def __sub__(self, other: tuple[float, float]) -> Vec2D: ...
        def __neg__(self) -> Vec2D: ...
        def __abs__(self) -> float: ...
        def rotate(self, angle: float) -> Vec2D:
            """Rotate self counterclockwise by angle."""

else:
    @disjoint_base
    class Vec2D(tuple[float, float]):
        """A 2 dimensional vector class, used as a helper class
        for implementing turtle graphics.
        May be useful for turtle graphics programs also.
        Derived from tuple, so a vector is a tuple!

        Provides (for a, b vectors, k number):
           a+b vector addition
           a-b vector subtraction
           a*b inner product
           k*a and a*k multiplication with scalar
           |a| absolute value of a
           a.rotate(angle) rotation
        """

        def __new__(cls, x: float, y: float) -> Self: ...
        def __add__(self, other: tuple[float, float]) -> Vec2D: ...  # type: ignore[override]

        @overload  # type: ignore[override]
        def __mul__(self, other: Vec2D) -> float: ...
        @overload
        def __mul__(self, other: float) -> Vec2D: ...

        def __rmul__(self, other: float) -> Vec2D: ...  # type: ignore[override]
        def __sub__(self, other: tuple[float, float]) -> Vec2D: ...
        def __neg__(self) -> Vec2D: ...
        def __abs__(self) -> float: ...
        def rotate(self, angle: float) -> Vec2D:
            """rotate self counterclockwise by angle"""

# Does not actually inherit from Canvas, but dynamically gets all methods of Canvas
class ScrolledCanvas(Canvas, Frame):  # type: ignore[misc]  # pyrefly: ignore [inconsistent-inheritance]
    """Modeled after the scrolled canvas class from Grayons's Tkinter book.

    Used as the default canvas, which pops up automatically when
    using turtle graphics functions or the Turtle class.
    """

    bg: str
    hscroll: Scrollbar
    vscroll: Scrollbar
    def __init__(
        self, master: Misc | None, width: int = 500, height: int = 350, canvwidth: int = 600, canvheight: int = 500
    ) -> None: ...
    canvwidth: int
    canvheight: int
    def reset(self, canvwidth: int | None = None, canvheight: int | None = None, bg: str | None = None) -> None:
        """Adjust canvas size, background color and scrollbars."""

class TurtleScreenBase:
    """Provide the basic graphics functionality.
    Interface between Tkinter and turtle.py.

    To port turtle.py to some different graphics toolkit
    a corresponding TurtleScreenBase class has to be implemented.
    """

    cv: Canvas
    canvwidth: int
    canvheight: int
    xscale: float
    yscale: float
    def __init__(self, cv: Canvas) -> None: ...
    def mainloop(self) -> None:
        """Start the event loop by calling Tkinter's mainloop function.

        Must be the last statement in a turtle graphics program. Must not
        be used if a script is run from within IDLE in -n mode (no
        subprocess).
        """

    def textinput(self, title: str, prompt: str) -> str | None:
        """Pop up a dialog window for input of a string.

        title is the title of the dialog window and prompt is a text
        describing what information to input. Return the string entered.
        If the dialog is canceled, return None.

        Arguments:
            title -- a string
            prompt -- a string

        Example (for a TurtleScreen instance named screen):
            >>> screen.textinput("NIM", "Name of first player:")
        """

    def numinput(
        self, title: str, prompt: str, default: float | None = None, minval: float | None = None, maxval: float | None = None
    ) -> float | None:
        """Pop up a dialog window for input of a number.

        title is the title of the dialog window and prompt is a text
        describing what numerical information to input. default is the
        default value, minval and maxval are the minimum and maximum
        values for input. If these are given, the number entered must be
        between minval and maxval. If not, a hint is issued and the dialog
        remains open for correction. Return the number entered.
        If the dialog is canceled, return None.

        Arguments:
            title -- a string
            prompt -- a string
            default -- a number or None
            minval -- a number or None
            maxval -- a number or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.numinput("Poker", "Your stakes:", 1000,
            ...                 minval=10, maxval=10000)
        """

class Terminator(Exception):
    """Will be raised in TurtleScreen.update, if _RUNNING becomes False.

    This stops execution of a turtle graphics script.
    Main purpose: use in the Demo-Viewer turtle.Demo.py.
    """

class TurtleGraphicsError(Exception):
    """Raised for invalid arguments or operations."""

class Shape:
    """Data structure modeling shapes.

    type_ is one of the strings "polygon", "image" or "compound". What
    data must be depends on type_: for "polygon" a tuple of pairs of
    coordinates, for "image" an image (only used internally), and for
    "compound" None, as a compound shape is built up using the
    addcomponent method.
    """

    def __init__(
        self, type_: Literal["polygon", "image", "compound"], data: _PolygonCoords | PhotoImage | None = None
    ) -> None: ...
    def addcomponent(self, poly: _PolygonCoords, fill: _Color, outline: _Color | None = None) -> None:
        """Add a component to a shape of type "compound".

        poly is a polygon, a tuple of pairs of numbers. fill is the color
        the polygon is filled with and outline is the color of its
        outline. If outline is not given, fill is used for it as well.

        Arguments:
            poly -- a tuple of pairs of numbers
            fill -- a color
            outline -- a color or None

        Example:
            >>> poly = ((0,0), (10,-5), (0,10), (-10,-5))
            >>> s = Shape("compound")
            >>> s.addcomponent(poly, "red", "blue")
            >>> # ... add more components and then use register_shape()
        """

class TurtleScreen(TurtleScreenBase):
    """Provides screen oriented methods like bgcolor etc.

    Only relies upon the methods of TurtleScreenBase and not upon
    components of the underlying graphics toolkit, which is Tkinter
    in this case.
    """

    def __init__(
        self, cv: Canvas, mode: Literal["standard", "logo", "world"] = "standard", colormode: float = 1.0, delay: int = 10
    ) -> None: ...
    def clear(self) -> None:
        """Delete all drawings and all turtles from the TurtleScreen.

        Aliases: clear | clearscreen

        Reset the now empty TurtleScreen to its initial state: white
        background, no background image, no event bindings and tracing on.
        As a function, this method is only available under the name
        clearscreen. The function clear is derived from the Turtle method
        clear.

        Example (for a TurtleScreen instance named screen):
            >>> screen.clearscreen()
        """

    @overload
    def mode(self, mode: None = None) -> str:
        """Set the turtle mode and perform a reset.

        mode is one of the strings "standard", "logo" or "world". If mode
        is not given, return the current mode.

        Mode "standard" is compatible with the old turtle module. Mode
        "logo" is compatible with most Logo turtle graphics. Mode "world"
        uses user-defined world coordinates, see setworldcoordinates(). In
        this mode, angles appear distorted if the x and y units are not of
        equal length.

             Mode      Initial turtle heading     positive angles
         ------------|-------------------------|-------------------
          "standard"    to the right (east)       counterclockwise
            "logo"        upward    (north)         clockwise

        Argument:
            mode -- a string or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.mode("logo")  # resets turtle heading to north
            >>> screen.mode()
            'logo'
        """
    @overload
    def mode(self, mode: Literal["standard", "logo", "world"]) -> None: ...

    def setworldcoordinates(self, llx: float, lly: float, urx: float, ury: float) -> None:
        """Set up a user-defined coordinate system.

        llx and lly are the x and y coordinates of the lower left corner of
        the canvas. urx and ury are the x and y coordinates of the upper
        right corner. Switch to mode "world" if necessary, which performs a
        reset. If mode "world" is already active, all drawings are redrawn
        according to the new coordinates. In user-defined coordinate
        systems, angles may appear distorted, see mode().

        Arguments:
            llx -- a number
            lly -- a number
            urx -- a number
            ury -- a number

        Example (for a TurtleScreen instance named screen):
            >>> screen.reset()
            >>> screen.setworldcoordinates(-50,-7.5,50,7.5)
            >>> for _ in range(72):
            ...     left(10)
            ...
            >>> for _ in range(8):
            ...     left(45); fd(2)   # a regular octagon
        """

    def register_shape(self, name: str, shape: _PolygonCoords | Shape | None = None) -> None:
        """Add a turtle shape to the TurtleScreen's shape list.

        Aliases: register_shape | addshape

        It can be called in four different ways:

        (1) name is the name of an image file (PNG, GIF, PGM, and PPM) and
            shape is None. Install the corresponding image shape.
        (2) name is an arbitrary string and shape is the name of an image
            file (PNG, GIF, PGM, and PPM). Install the corresponding image
            shape.
        (3) name is an arbitrary string and shape is a tuple of pairs of
            coordinates. Install the corresponding polygon shape.
        (4) name is an arbitrary string and shape is a (compound) Shape
            object. Install the corresponding compound shape.

        Image shapes do not rotate when turning the turtle, so they do not
        display the heading of the turtle. To use a registered shape, call
        shape() with its name.

        Arguments:
            name -- a string
            shape -- a string, a tuple of pairs of numbers, a Shape object
                     or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.register_shape("turtle.gif")
            >>> screen.register_shape("turtle", "turtle.gif")
            >>> screen.register_shape("triangle", ((5,-3), (0,5), (-5,-3)))
        """

    @overload
    def colormode(self, cmode: None = None) -> float:
        """Return the colormode or set it to 1.0 or 255.

        After setting it, the r, g and b values of color triples have to
        be between 0 and cmode.

        Argument:
            cmode -- a number or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.colormode(1)
            >>> turtle.pencolor(240, 160, 80)
            Traceback (most recent call last):
                 ...
            TurtleGraphicsError: bad color sequence: (240, 160, 80)
            >>> screen.colormode()
            1.0
            >>> screen.colormode(255)
            >>> screen.colormode()
            255
            >>> turtle.pencolor(240,160,80)
        """
    @overload
    def colormode(self, cmode: float) -> None: ...

    def reset(self) -> None:
        """Reset all turtles on the screen to their initial state.

        Aliases: reset | resetscreen

        As a function, this method is only available under the name
        resetscreen. The function reset is derived from the Turtle method
        reset().
        """

    def turtles(self) -> list[Turtle]:
        """Return the list of turtles on the screen.

        Example (for a TurtleScreen instance named screen):
            >>> for turtle in screen.turtles():
            ...     turtle.color("red")
        """

    @overload
    def bgcolor(self) -> _AnyColor:
        """Return or set the background color of the TurtleScreen.

        Four input formats are allowed:
          - bgcolor()
            Return the current background color as a color specification
            string or as a tuple (see example).  May be used as input
            to another color(), pencolor(), fillcolor() or bgcolor() call.
          - bgcolor(colorstring)
            Set the background color to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - bgcolor((r, g, b))
            Set the background color to the RGB color represented by
            the tuple of r, g, and b.  Each of r, g, and b must be
            between 0 and colormode, where colormode is either 1.0 or 255
            (see colormode()).
          - bgcolor(r, g, b)
            Set the background color to the RGB color represented by
            r, g, and b.  Each of r, g, and b must be between 0 and
            colormode.

        Example (for a TurtleScreen instance named screen):
            >>> screen.bgcolor("orange")
            >>> screen.bgcolor()
            'orange'
            >>> screen.colormode(255)
            >>> screen.bgcolor("#800080")
            >>> screen.bgcolor()
            (128.0, 0.0, 128.0)
        """
    @overload
    def bgcolor(self, color: _Color) -> None: ...
    @overload
    def bgcolor(self, r: float, g: float, b: float) -> None: ...

    @overload
    def tracer(self, n: None = None) -> int:
        """Turn turtle animation on or off and set the drawing delay.

        If n is given, only every n-th regular screen update is performed,
        which can be used to speed up the drawing of complex graphics. If
        n is 0, animation is turned off and the screen is only updated by
        calling update(). If delay is given, it sets the drawing delay, see
        delay(). When called without arguments, return the current value
        of n.

        Arguments:
            n -- a nonnegative integer or None
            delay -- a nonnegative integer or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.tracer(8, 25)
            >>> dist = 2
            >>> for i in range(200):
            ...     fd(dist)
            ...     rt(90)
            ...     dist += 2
        """
    @overload
    def tracer(self, n: int, delay: int | None = None) -> None: ...

    @overload
    def delay(self, delay: None = None) -> int:
        """Return or set the drawing delay in milliseconds.

        This is approximately the time interval between two consecutive
        canvas updates. The longer the drawing delay, the slower the
        animation.

        Argument:
            delay -- a nonnegative integer or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.delay()
            10
            >>> screen.delay(5)
            >>> screen.delay()
            5
        """
    @overload
    def delay(self, delay: int) -> None: ...

    if sys.version_info >= (3, 14):
        @contextmanager
        def no_animation(self) -> Generator[None]:
            """Temporarily turn off turtle animation.

            The code inside the with block is not animated. Once the block is
            exited, the drawing appears. This is useful for drawing complex
            shapes where even the fastest speed setting is too slow.

            Example (for a TurtleScreen instance named screen):
                >>> with screen.no_animation():
                ...     for dist in range(2, 400, 2):
                ...         fd(dist)
                ...         rt(90)
            """

    def update(self) -> None:
        """Perform a TurtleScreen update."""

    def window_width(self) -> int:
        """Return the width of the turtle window.

        Example (for a TurtleScreen instance named screen):
            >>> screen.window_width()
            640
        """

    def window_height(self) -> int:
        """Return the height of the turtle window.

        Example (for a TurtleScreen instance named screen):
            >>> screen.window_height()
            480
        """

    def getcanvas(self) -> Canvas:
        """Return the Canvas of this TurtleScreen.

        Example (for a Screen instance named screen):
            >>> cv = screen.getcanvas()
            >>> cv
            <turtle.ScrolledCanvas object ...>
        """

    def getshapes(self) -> list[str]:
        """Return a list of names of all currently available turtle shapes.

        Example (for a TurtleScreen instance named screen):
            >>> screen.getshapes()
            ['arrow', 'blank', 'circle', ..., 'turtle']
        """

    def onclick(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-click events on this screen.

        Aliases: onclick | onscreenclick

        fun is called with the coordinates of the clicked point on the
        canvas. If fun is None, existing bindings are removed. btn is the
        number of the mouse button and defaults to 1, the left mouse
        button. If add is True, a new binding is added, otherwise it
        replaces a former binding. As a function, this method is only
        available under the name onscreenclick. The function onclick is
        derived from the Turtle method onclick.

        Arguments:
            fun -- a function with two arguments or None
            btn -- an integer
            add -- a boolean or None

        Example (for a TurtleScreen instance named screen):
            >>> screen.onscreenclick(turtle.goto)
            >>> # Subsequently clicking into the TurtleScreen will
            >>> # make the turtle move to the clicked point.
            >>> screen.onscreenclick(None)  # remove event binding again
        """

    def onkey(self, fun: Callable[[], object], key: str) -> None:
        """Bind fun to key-release event of key.

        Arguments:
        fun -- a function with no arguments
        key -- a string: key (e.g. "a") or key-symbol (e.g. "space")

        In order to be able to register key-events, TurtleScreen
        must have focus. (See method listen.)

        Example (for a TurtleScreen instance named screen):

        >>> def f():
        ...     fd(50)
        ...     lt(60)
        ...
        >>> screen.onkey(f, "Up")
        >>> screen.listen()

        Subsequently the turtle can be moved by repeatedly pressing
        the up-arrow key, consequently drawing a hexagon

        """

    def listen(self, xdummy: float | None = None, ydummy: float | None = None) -> None:
        """Set focus on TurtleScreen (in order to collect key-events)

        No arguments.
        Dummy arguments are provided in order
        to be able to pass listen to the onclick method.

        Example (for a TurtleScreen instance named screen):
        >>> screen.listen()
        """

    def ontimer(self, fun: Callable[[], object], t: int = 0) -> None:
        """Install a timer, which calls fun after t milliseconds.

        Arguments:
        fun -- a function with no arguments.
        t -- a number >= 0

        Example (for a TurtleScreen instance named screen):

        >>> running = True
        >>> def f():
        ...     if running:
        ...             fd(50)
        ...             lt(60)
        ...             screen.ontimer(f, 250)
        ...
        >>> f()   # makes the turtle marching around
        >>> running = False
        """

    @overload
    def bgpic(self, picname: None = None) -> str:
        """Set background image or return name of current backgroundimage.

        Optional argument:
        picname -- a string, name of an image file (PNG, GIF, PGM, and PPM) or "nopic".

        If picname is a filename, set the corresponding image as background.
        If picname is "nopic", delete backgroundimage, if present.
        If picname is None, return the filename of the current backgroundimage.

        Example (for a TurtleScreen instance named screen):
        >>> screen.bgpic()
        'nopic'
        >>> screen.bgpic("landscape.gif")
        >>> screen.bgpic()
        'landscape.gif'
        """
    @overload
    def bgpic(self, picname: str) -> None: ...

    @overload
    def screensize(self, canvwidth: None = None, canvheight: None = None, bg: None = None) -> tuple[int, int]:
        """Resize the canvas the turtles are drawing on.

        Optional arguments:
        canvwidth -- positive integer, new width of canvas in pixels
        canvheight --  positive integer, new height of canvas in pixels
        bg -- colorstring or color-tuple, new backgroundcolor
        If no arguments are given, return current (canvaswidth, canvasheight)

        Do not alter the drawing window. To observe hidden parts of
        the canvas use the scrollbars. (Can make visible those parts
        of a drawing, which were outside the canvas before!)

        Example (for a Turtle instance named turtle):
        >>> turtle.screensize(2000,1500)
        >>> # e.g. to search for an erroneously escaped turtle ;-)
        """
    # Looks like if self.cv is not a ScrolledCanvas, this could return a tuple as well
    @overload
    def screensize(self, canvwidth: int, canvheight: int, bg: _Color | None = None) -> None: ...

    if sys.version_info >= (3, 14):
        def save(self, filename: StrPath, *, overwrite: bool = False) -> None:
            """Save the drawing as a PostScript file

            Arguments:
            filename -- a string, the path of the created file.
                        Must end with '.ps' or '.eps'.

            Optional arguments:
            overwrite -- boolean, if true, then existing files will be overwritten

            Example (for a TurtleScreen instance named screen):
            >>> screen.save('my_drawing.eps')
            """

    onscreenclick = onclick
    resetscreen = reset
    clearscreen = clear
    addshape = register_shape
    def onkeypress(self, fun: Callable[[], object], key: str | None = None) -> None:
        """Bind fun to key-press event of key if key is given,
        or to any key-press-event if no key is given.

        Arguments:
        fun -- a function with no arguments
        key -- a string: key (e.g. "a") or key-symbol (e.g. "space")

        In order to be able to register key-events, TurtleScreen
        must have focus. (See method listen.)

        Example (for a TurtleScreen instance named screen
        and a Turtle instance named turtle):

        >>> def f():
        ...     fd(50)
        ...     lt(60)
        ...
        >>> screen.onkeypress(f, "Up")
        >>> screen.listen()

        Subsequently the turtle can be moved by repeatedly pressing
        the up-arrow key, or by keeping pressed the up-arrow key.
        consequently drawing a hexagon.
        """

    onkeyrelease = onkey

class TNavigator:
    """The navigation part of RawTurtle.

    Implements methods for turtle movement.
    """

    START_ORIENTATION: dict[str, Vec2D]
    DEFAULT_MODE: str
    DEFAULT_ANGLEOFFSET: int
    DEFAULT_ANGLEORIENT: int
    def __init__(self, mode: Literal["standard", "logo", "world"] = "standard") -> None: ...
    def reset(self) -> None:
        """Reset the turtle to its initial values.

        Will be overwritten by the child class.
        """

    def degrees(self, fullcircle: float = 360.0) -> None:
        """Set the angle measurement units to degrees.

        The number of degrees in a full circle is set to fullcircle, which
        defaults to 360.

        Argument:
            fullcircle -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.left(90)
            >>> turtle.heading()
            90.0

            >>> # Change angle measurement unit to grad (also known as gon,
            >>> # grade, or gradian and equals 1/100-th of the right angle.)
            >>> turtle.degrees(400.0)
            >>> turtle.heading()
            100.0
            >>> turtle.degrees(360)
            >>> turtle.heading()
            90.0
        """

    def radians(self) -> None:
        """Set the angle measurement units to radians.

        Equivalent to degrees(2 * math.pi).

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.left(90)
            >>> turtle.heading()
            90.0
            >>> turtle.radians()
            >>> turtle.heading()
            1.5707963267948966
        """

    if sys.version_info >= (3, 12):
        def teleport(self, x: float | None = None, y: float | None = None, *, fill_gap: bool = False) -> None:
            """To be overwritten by the child class RawTurtle.

            Includes no TPen references.
            """

    def forward(self, distance: float) -> None:
        """Move the turtle forward by the specified distance.

        Aliases: forward | fd

        The turtle moves in the direction it is headed.

        Argument:
            distance -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.position()
            (0.00,0.00)
            >>> turtle.forward(25)
            >>> turtle.position()
            (25.00,0.00)
            >>> turtle.forward(-75)
            >>> turtle.position()
            (-50.00,0.00)
        """

    def back(self, distance: float) -> None:
        """Move the turtle backward by distance.

        Aliases: back | backward | bk

        The turtle moves opposite to the direction it is headed.
        The turtle's heading does not change.

        Argument:
            distance -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.position()
            (0.00,0.00)
            >>> turtle.backward(30)
            >>> turtle.position()
            (-30.00,0.00)
        """

    def right(self, angle: float) -> None:
        """Turn the turtle right by the specified angle.

        Aliases: right | rt

        The angle is measured in degrees by default; the unit can be
        changed with degrees() or radians(). How the heading is measured
        depends on the turtle mode, see mode().

        Argument:
            angle -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.heading()
            22.0
            >>> turtle.right(45)
            >>> turtle.heading()
            337.0
        """

    def left(self, angle: float) -> None:
        """Turn the turtle left by the specified angle.

        Aliases: left | lt

        The angle is measured in degrees by default; the unit can be
        changed with degrees() or radians(). How the heading is measured
        depends on the turtle mode, see mode().

        Argument:
            angle -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.heading()
            22.0
            >>> turtle.left(45)
            >>> turtle.heading()
            67.0
        """

    def pos(self) -> Vec2D:
        """Return the turtle's current location (x,y) as a Vec2D vector.

        Aliases: pos | position

        Example (for a Turtle instance named turtle):
            >>> turtle.pos()
            (0.00,240.00)
        """

    def xcor(self) -> float:
        """Return the turtle's x coordinate.

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.left(50)
            >>> turtle.forward(100)
            >>> print(round(turtle.xcor(), 5))
            64.27876
        """

    def ycor(self) -> float:
        """Return the turtle's y coordinate

        Example (for a Turtle instance named turtle):
            >>> reset()
            >>> turtle.left(60)
            >>> turtle.forward(100)
            >>> print(round(turtle.ycor(), 5))
            86.60254
        """

    @overload
    def goto(self, x: tuple[float, float], y: None = None) -> None:
        """Move the turtle to an absolute position.

        Aliases: goto | setpos | setposition

        If y is None, x must be a pair of coordinates or a Vec2D, for
        example as returned by pos(). If the pen is down, a line is drawn.
        The turtle's heading does not change.

        Arguments:
            x -- a number or a pair/vector of numbers
            y -- a number or None

        Example (for a Turtle instance named turtle):
            >>> tp = turtle.pos()
            >>> tp
            (0.00,0.00)
            >>> turtle.goto(60,30)
            >>> turtle.pos()
            (60.00,30.00)
            >>> turtle.goto((20,80))
            >>> turtle.pos()
            (20.00,80.00)
            >>> turtle.goto(tp)
            >>> turtle.pos()
            (0.00,0.00)
        """
    @overload
    def goto(self, x: float, y: float) -> None: ...

    def home(self) -> None:
        """Move the turtle to the origin, coordinates (0,0).

        The turtle's heading is set to its start orientation, which
        depends on the turtle mode, see mode().

        Example (for a Turtle instance named turtle):
            >>> turtle.heading()
            90.0
            >>> turtle.position()
            (0.00,-10.00)
            >>> turtle.home()
            >>> turtle.position()
            (0.00,0.00)
            >>> turtle.heading()
            0.0
        """

    def setx(self, x: float) -> None:
        """Set the turtle's x coordinate to x.

        The y coordinate is unchanged.

        Argument:
            x -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.position()
            (0.00,240.00)
            >>> turtle.setx(10)
            >>> turtle.position()
            (10.00,240.00)
        """

    def sety(self, y: float) -> None:
        """Set the turtle's y coordinate to y.

        The x coordinate is unchanged.

        Argument:
            y -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.position()
            (0.00,40.00)
            >>> turtle.sety(-10)
            >>> turtle.position()
            (0.00,-10.00)
        """

    @overload
    def distance(self, x: TNavigator | tuple[float, float], y: None = None) -> float:
        """Return the distance from the turtle to (x,y) in turtle step units.

        If y is None, x must be a pair of coordinates, a Vec2D, for example
        as returned by pos(), or another turtle.

        Arguments:
            x -- a number or a pair/vector of numbers or a turtle instance
            y -- a number if x is a number, else None

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.distance(30,40)
            50.0
            >>> turtle.distance((30,40))
            50.0
            >>> joe = Turtle()
            >>> joe.forward(77)
            >>> turtle.distance(joe)
            77.0
        """
    @overload
    def distance(self, x: float, y: float) -> float: ...

    @overload
    def towards(self, x: TNavigator | tuple[float, float], y: None = None) -> float:
        """Return the angle of the line from the turtle's position to (x,y).

        If y is None, x must be a pair of coordinates, a Vec2D, for example
        as returned by pos(), or another turtle. The angle is measured from
        the turtle's start orientation, which depends on the turtle mode,
        see mode().

        Arguments:
            x -- a number or a pair/vector of numbers or a turtle instance
            y -- a number if x is a number, else None

        Example (for a Turtle instance named turtle):
            >>> turtle.goto(10, 10)
            >>> turtle.towards(0,0)
            225.0
        """
    @overload
    def towards(self, x: float, y: float) -> float: ...

    def heading(self) -> float:
        """Return the turtle's current heading.

        The value depends on the turtle mode, see mode().

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.left(67)
            >>> turtle.heading()
            67.0
        """

    def setheading(self, to_angle: float) -> None:
        """Set the turtle's heading to to_angle.

        Aliases: setheading | seth

        Here are some common directions in degrees:

             standard mode       logo mode
          -------------------|---------------
               0 - east           0 - north
              90 - north         90 - east
             180 - west         180 - south
             270 - south        270 - west

        Argument:
            to_angle -- a number

        Example (for a Turtle instance named turtle):
            >>> turtle.setheading(90)
            >>> turtle.heading()
            90.0
        """

    def circle(self, radius: float, extent: float | None = None, steps: int | None = None) -> None:
        """Draw a circle with the given radius.

        The center is radius units left of the turtle; extent, an angle,
        determines which part of the circle is drawn. If extent is not
        given, draw the entire circle. If extent is not a full circle, one
        endpoint of the arc is the current pen position. Draw the arc in
        the counterclockwise direction if radius is positive, otherwise in
        the clockwise direction. Finally, the turtle's heading is changed by
        extent.

        As the circle is approximated by an inscribed regular polygon,
        steps determines the number of steps to use. If not given, it
        will be calculated automatically. May be used to draw regular
        polygons.

        Arguments:
            radius -- a number
            extent -- a number or None
            steps -- an integer or None

        Example (for a Turtle instance named turtle):
            >>> turtle.circle(50)
            >>> turtle.circle(120, 180)  # draw a semicircle
        """

    def speed(self, s: int | None = 0) -> int | None:
        """Dummy method - to be overwritten by the child class."""

    fd = forward
    bk = back
    backward = back
    rt = right
    lt = left
    position = pos
    setpos = goto
    setposition = goto
    seth = setheading

class TPen:
    """Drawing part of the RawTurtle.

    Implements drawing properties.
    """

    def __init__(self, resizemode: Literal["auto", "user", "noresize"] = "noresize") -> None: ...

    @overload
    def resizemode(self, rmode: None = None) -> str:
        """Set the resizemode to "auto", "user" or "noresize".

        If rmode is not given, return the current resizemode. The
        different resizemodes have the following effects:

          - "auto": the turtle is resized according to the value of
            pensize().
          - "user": the turtle is resized according to the stretch factors
            and outline width, which are set by shapesize().
          - "noresize": the turtle is not resized.

        resizemode("user") is called by shapesize() when used with
        arguments.

        Argument:
            rmode -- a string or None

        Example (for a Turtle instance named turtle):
            >>> turtle.resizemode()
            'noresize'
            >>> turtle.resizemode("auto")
            >>> turtle.resizemode()
            'auto'
        """
    @overload
    def resizemode(self, rmode: Literal["auto", "user", "noresize"]) -> None: ...

    @overload
    def pensize(self, width: None = None) -> int:
        """Return or set the line thickness.

        Aliases: pensize | width

        If width is not given, return the current pensize. If resizemode
        is set to "auto" and the turtle shape is a polygon, that polygon is
        drawn with the same line thickness.

        Argument:
            width -- a positive number or None

        Example (for a Turtle instance named turtle):
            >>> turtle.pensize()
            1
            >>> turtle.pensize(10)   # from here on lines of width 10 are drawn
        """
    @overload
    def pensize(self, width: int) -> None: ...

    def penup(self) -> None:
        """Lift the pen.

        Aliases: penup | pu | up

        The turtle does not draw when it moves.
        """

    def pendown(self) -> None:
        """Lower the pen.

        Aliases: pendown | pd | down

        The turtle draws a line when it moves.
        """

    def isdown(self) -> bool:
        """Return True if the pen is down, False if it's up.

        Example (for a Turtle instance named turtle):
            >>> turtle.penup()
            >>> turtle.isdown()
            False
            >>> turtle.pendown()
            >>> turtle.isdown()
            True
        """

    @overload
    def speed(self, speed: None = None) -> int:
        """Return or set the turtle's speed.

        If speed is not given, return the current speed. Otherwise set the
        turtle's speed to an integer from 0 to 10. If speed is a number
        greater than 10 or smaller than 0.5, the speed is set to 0. You
        can also use these names instead of numbers:

            "fastest": 0
            "fast": 10
            "normal": 6
            "slow": 3
            "slowest": 1

        Speeds from 1 to 10 give increasingly faster animation of line
        drawing and turtle turning. A speed of 0 means that no animation
        takes place: forward() and back() make the turtle jump, and
        likewise left() and right() make the turtle turn instantly.

        Argument:
            speed -- an integer, a string or None

        Example (for a Turtle instance named turtle):
            >>> turtle.speed()
            3
            >>> turtle.speed('normal')
            >>> turtle.speed()
            6
            >>> turtle.speed(9)
            >>> turtle.speed()
            9
        """
    @overload
    def speed(self, speed: _Speed) -> None: ...

    @overload
    def pencolor(self) -> _AnyColor:
        """Return or set the pencolor.

        Four input formats are allowed:
          - pencolor()
            Return the current pencolor as color specification string or
            as a tuple (see example).  May be used as input to another
            color(), pencolor(), fillcolor() or bgcolor() call.
          - pencolor(colorstring)
            Set the pencolor to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - pencolor((r, g, b))
            Set the pencolor to the RGB color represented by the tuple of
            r, g, and b.  Each of r, g, and b must be between 0 and
            colormode, where colormode is either 1.0 or 255 (see
            colormode()).
          - pencolor(r, g, b)
            Set the pencolor to the RGB color represented by r, g, and b.
            Each of r, g, and b must be between 0 and colormode.

        If the turtle shape is a polygon, the outline of that polygon is
        drawn with the newly set pencolor.

        Example (for a Turtle instance named turtle):
            >>> turtle.pencolor("brown")
            >>> turtle.pencolor()
            'brown'
            >>> colormode(255)
            >>> turtle.pencolor("#32c18f")
            >>> turtle.pencolor()
            (50.0, 193.0, 143.0)
        """
    @overload
    def pencolor(self, color: _Color) -> None: ...
    @overload
    def pencolor(self, r: float, g: float, b: float) -> None: ...

    @overload
    def fillcolor(self) -> _AnyColor:
        """Return or set the fillcolor.

        Four input formats are allowed:
          - fillcolor()
            Return the current fillcolor as color specification string,
            possibly in tuple format (see example).  May be used as
            input to another color(), pencolor(), fillcolor() or bgcolor() call.
          - fillcolor(colorstring)
            Set the fillcolor to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - fillcolor((r, g, b))
            Set the fillcolor to the RGB color represented by the tuple of
            r, g, and b.  Each of r, g, and b must be between 0 and
            colormode, where colormode is either 1.0 or 255 (see
            colormode()).
          - fillcolor(r, g, b)
            Set the fillcolor to the RGB color represented by r, g, and b.
            Each of r, g, and b must be between 0 and colormode.

        If the turtle shape is a polygon, the interior of that polygon is
        drawn with the newly set fillcolor.

        Example (for a Turtle instance named turtle):
            >>> turtle.fillcolor("violet")
            >>> turtle.fillcolor()
            'violet'
            >>> colormode(255)
            >>> turtle.fillcolor("#ffffff")
            >>> turtle.fillcolor()
            (255.0, 255.0, 255.0)
        """
    @overload
    def fillcolor(self, color: _Color) -> None: ...
    @overload
    def fillcolor(self, r: float, g: float, b: float) -> None: ...

    @overload
    def color(self) -> tuple[_AnyColor, _AnyColor]:
        """Return or set the pencolor and fillcolor.

        Several input formats are allowed. They use 0 to 3 arguments as follows:
          - color()
            Return the current pencolor and the current fillcolor as
            a pair of color specification strings or tuples as returned
            by pencolor() and fillcolor().
          - color(colorstring), color((r,g,b)), color(r,g,b)
            Inputs as in pencolor(). Set both the pencolor and the
            fillcolor to the given value.
          - color(colorstring1, colorstring2), color((r1,g1,b1), (r2,g2,b2))
            Equivalent to pencolor(colorstring1) and fillcolor(colorstring2)
            and analogously if the other input format is used.

        If the turtle shape is a polygon, the outline and interior of that
        polygon are drawn with the newly set colors. For more information,
        see pencolor() and fillcolor().

        Example (for a Turtle instance named turtle):
            >>> turtle.color("red", "green")
            >>> turtle.color()
            ('red', 'green')
            >>> colormode(255)
            >>> turtle.color("#285078", "#a0c8f0")
            >>> turtle.color()
            ((40.0, 80.0, 120.0), (160.0, 200.0, 240.0))
        """
    @overload
    def color(self, color: _Color) -> None: ...
    @overload
    def color(self, r: float, g: float, b: float) -> None: ...
    @overload
    def color(self, color1: _Color, color2: _Color) -> None: ...

    if sys.version_info >= (3, 12):
        def teleport(self, x: float | None = None, y: float | None = None, *, fill_gap: bool = False) -> None:
            """To be overwritten by the child class RawTurtle.

            Includes no TNavigator references.
            """

    def showturtle(self) -> None:
        """Make the turtle visible.

        Aliases: showturtle | st
        """

    def hideturtle(self) -> None:
        """Make the turtle invisible.

        Aliases: hideturtle | ht

        If the pen is down, movement still draws.
        """

    def isvisible(self) -> bool:
        """Return True if the turtle is visible, False if it's hidden.

        Example (for a Turtle instance named turtle):
            >>> turtle.hideturtle()
            >>> turtle.isvisible()
            False
            >>> turtle.showturtle()
            >>> turtle.isvisible()
            True
        """

    # Note: signatures 1 and 2 overlap unsafely when no arguments are provided
    @overload
    def pen(self) -> _PenState:
        """Return or set the pen's attributes.

        Arguments:
            pen -- a dictionary with some or all of the below listed keys.
            **pendict -- one or more keyword-arguments with the below
                         listed keys as keywords.

        Return or set the pen's attributes in a 'pen-dictionary'
        with the following key/value pairs:
           "shown"      :   True/False
           "pendown"    :   True/False
           "pencolor"   :   color-string or color-tuple
           "fillcolor"  :   color-string or color-tuple
           "pensize"    :   positive number
           "speed"      :   number in range 0..10
           "resizemode" :   "auto" or "user" or "noresize"
           "stretchfactor": (positive number, positive number)
           "shearfactor":   number
           "outline"    :   positive number
           "tilt"       :   number

        This dictionary can be used as argument for a subsequent
        pen()-call to restore the former pen-state. Moreover one
        or more of these attributes can be provided as keyword-arguments.
        This can be used to set several pen attributes in one statement.


        Examples (for a Turtle instance named turtle):
        >>> turtle.pen(fillcolor="black", pencolor="red", pensize=10)
        >>> turtle.pen()
        {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
        'pencolor': 'red', 'pendown': True, 'fillcolor': 'black',
        'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
        >>> penstate=turtle.pen()
        >>> turtle.color("yellow","")
        >>> turtle.penup()
        >>> turtle.pen()
        {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
        'pencolor': 'yellow', 'pendown': False, 'fillcolor': '',
        'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
        >>> p.pen(penstate, fillcolor="green")
        >>> p.pen()
        {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
        'pencolor': 'red', 'pendown': True, 'fillcolor': 'green',
        'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
        """
    @overload
    def pen(
        self,
        pen: _PenState | None = None,
        *,
        shown: bool = ...,
        pendown: bool = ...,
        pencolor: _Color = ...,
        fillcolor: _Color = ...,
        pensize: int = ...,
        speed: int = ...,
        resizemode: Literal["auto", "user", "noresize"] = ...,
        stretchfactor: tuple[float, float] = ...,
        outline: int = ...,
        tilt: float = ...,
    ) -> None: ...

    width = pensize
    up = penup
    pu = penup
    pd = pendown
    down = pendown
    st = showturtle
    ht = hideturtle

class RawTurtle(TPen, TNavigator):  # type: ignore[misc]  # Conflicting methods in base classes  # pyrefly: ignore [inconsistent-inheritance]
    """A turtle that draws on the given canvas or TurtleScreen.

    Combines the navigation part (TNavigator) and the drawing part (TPen)
    and adds the animation of the turtle on the screen.
    """

    screen: TurtleScreen
    screens: ClassVar[list[TurtleScreen]]
    def __init__(
        self,
        canvas: Canvas | TurtleScreen | None = None,
        shape: str = "classic",
        undobuffersize: int = 1000,
        visible: bool = True,
    ) -> None: ...
    def reset(self) -> None:
        """Delete the turtle's drawings and re-center the turtle.

        The turtle's settings are restored to their default values.

        Example (for a Turtle instance named turtle):
            >>> turtle.goto(0,-22)
            >>> turtle.left(100)
            >>> turtle.position()
            (0.00,-22.00)
            >>> turtle.heading()
            100.0
            >>> turtle.reset()
            >>> turtle.position()
            (0.00,0.00)
            >>> turtle.heading()
            0.0
        """

    def setundobuffer(self, size: int | None) -> None:
        """Set or disable the undo buffer.

        If size is an integer, an empty undo buffer of the given size is
        installed. size is the maximum number of turtle actions that can
        be undone by undo(). If size is None, the undo buffer is disabled.

        Argument:
            size -- an integer or None

        Example (for a Turtle instance named turtle):
            >>> turtle.setundobuffer(42)
        """

    def undobufferentries(self) -> int:
        """Return the number of entries in the undo buffer.

        Example (for a Turtle instance named turtle):
            >>> while undobufferentries():
            ...     undo()
        """

    def clear(self) -> None:
        """Delete the turtle's drawings from the screen.

        The state and position of the turtle as well as the drawings of
        other turtles are not affected.

        Example (for a Turtle instance named turtle):
            >>> turtle.clear()
        """

    def clone(self) -> Self:
        """Create and return a clone of the turtle.

        The clone has the same position, heading and turtle properties.

        Example:
            >>> leonel = Turtle()
            >>> marco = leonel.clone()
        """

    @overload
    def shape(self, name: None = None) -> str:
        """Set the turtle shape to the named shape.

        If name is not given, return the name of the current shape. The
        shape must exist in the TurtleScreen's shape dictionary. Initially
        there are the following shapes: "arrow", "turtle", "circle",
        "square", "triangle" and "classic". To add shapes, see the Screen
        method register_shape().

        Argument:
            name -- a string or None

        Example (for a Turtle instance named turtle):
            >>> turtle.shape()
            'classic'
            >>> turtle.shape("turtle")
            >>> turtle.shape()
            'turtle'
        """
    @overload
    def shape(self, name: str) -> None: ...

    # Unsafely overlaps when no arguments are provided
    @overload
    def shapesize(self) -> tuple[float, float, float]:
        """Return or set the turtle's stretch factors and outline width.

        Aliases: shapesize | turtlesize

        If no arguments are given, return the current values. Otherwise
        set resizemode to "user", so that the turtle is displayed
        stretched according to its stretch factors:

          - stretch_wid: the stretch factor perpendicular to the turtle's
            heading
          - stretch_len: the stretch factor in the direction of the
            turtle's heading
          - outline: the width of the shape's outline

        For example, shapesize(2, 1) makes the turtle twice as wide
        without changing its length. If only stretch_wid is given,
        stretch_len is set to the same value.

        Arguments:
            stretch_wid -- a positive number or None
            stretch_len -- a positive number or None
            outline -- a positive number or None

        Example (for a Turtle instance named turtle):
            >>> turtle.shapesize()
            (1.0, 1.0, 1)
            >>> turtle.resizemode("user")
            >>> turtle.shapesize(5, 5, 12)
            >>> turtle.shapesize()
            (5, 5, 12)
            >>> turtle.shapesize(outline=8)
            >>> turtle.shapesize()
            (5, 5, 8)
        """
    @overload
    def shapesize(
        self, stretch_wid: float | None = None, stretch_len: float | None = None, outline: float | None = None
    ) -> None: ...

    @overload
    def shearfactor(self, shear: None = None) -> float:
        """Set or return the current shearfactor.

        Optional argument: shear -- number, tangent of the shear angle

        Shear the turtleshape according to the given shearfactor shear,
        which is the tangent of the shear angle. DO NOT change the
        turtle's heading (direction of movement).
        If shear is not given: return the current shearfactor, i. e. the
        tangent of the shear angle, by which lines parallel to the
        heading of the turtle are sheared.

        Examples (for a Turtle instance named turtle):
        >>> turtle.shape("circle")
        >>> turtle.shapesize(5,2)
        >>> turtle.shearfactor(0.5)
        >>> turtle.shearfactor()
        >>> 0.5
        """
    @overload
    def shearfactor(self, shear: float) -> None: ...

    # Unsafely overlaps when no arguments are provided
    @overload
    def shapetransform(self) -> tuple[float, float, float, float]:
        """Set or return the current transformation matrix of the turtle shape.

        Optional arguments: t11, t12, t21, t22 -- numbers.

        If none of the matrix elements are given, return the transformation
        matrix.
        Otherwise set the given elements and transform the turtleshape
        according to the matrix consisting of first row t11, t12 and
        second row t21, 22.
        Modify stretchfactor, shearfactor and tiltangle according to the
        given matrix.

        Examples (for a Turtle instance named turtle):
        >>> turtle.shape("square")
        >>> turtle.shapesize(4,2)
        >>> turtle.shearfactor(-0.5)
        >>> turtle.shapetransform()
        (4.0, -1.0, -0.0, 2.0)
        """
    @overload
    def shapetransform(
        self, t11: float | None = None, t12: float | None = None, t21: float | None = None, t22: float | None = None
    ) -> None: ...

    def get_shapepoly(self) -> _PolygonCoords | None:
        """Return the current shape polygon as tuple of coordinate pairs.

        No argument.

        Examples (for a Turtle instance named turtle):
        >>> turtle.shape("square")
        >>> turtle.shapetransform(4, -1, 0, 2)
        >>> turtle.get_shapepoly()
        ((50, -20), (30, 20), (-50, 20), (-30, -20))

        """

    if sys.version_info < (3, 13):
        @deprecated("Deprecated since Python 3.1; removed in Python 3.13. Use `tiltangle()` instead.")
        def settiltangle(self, angle: float) -> None:
            """Rotate the turtleshape to point in the specified direction

            Argument: angle -- number

            Rotate the turtleshape to point in the direction specified by angle,
            regardless of its current tilt-angle. DO NOT change the turtle's
            heading (direction of movement).

            Deprecated since Python 3.1

            Examples (for a Turtle instance named turtle):
            >>> turtle.shape("circle")
            >>> turtle.shapesize(5,2)
            >>> turtle.settiltangle(45)
            >>> turtle.stamp()
            >>> turtle.fd(50)
            >>> turtle.settiltangle(-45)
            >>> turtle.stamp()
            >>> turtle.fd(50)
            """

    @overload
    def tiltangle(self, angle: None = None) -> float:
        """Set or return the current tilt-angle.

        Optional argument: angle -- number

        Rotate the turtleshape to point in the direction specified by angle,
        regardless of its current tilt-angle. DO NOT change the turtle's
        heading (direction of movement).
        If angle is not given: return the current tilt-angle, i. e. the angle
        between the orientation of the turtleshape and the heading of the
        turtle (its direction of movement).

        Examples (for a Turtle instance named turtle):
        >>> turtle.shape("circle")
        >>> turtle.shapesize(5, 2)
        >>> turtle.tiltangle()
        0.0
        >>> turtle.tiltangle(45)
        >>> turtle.tiltangle()
        45.0
        >>> turtle.stamp()
        >>> turtle.fd(50)
        >>> turtle.tiltangle(-45)
        >>> turtle.tiltangle()
        315.0
        >>> turtle.stamp()
        >>> turtle.fd(50)
        """
    @overload
    def tiltangle(self, angle: float) -> None: ...

    def tilt(self, angle: float) -> None:
        """Rotate the turtleshape by angle.

        Argument:
        angle - a number

        Rotate the turtleshape by angle from its current tilt-angle,
        but do NOT change the turtle's heading (direction of movement).

        Examples (for a Turtle instance named turtle):
        >>> turtle.shape("circle")
        >>> turtle.shapesize(5,2)
        >>> turtle.tilt(30)
        >>> turtle.fd(50)
        >>> turtle.tilt(30)
        >>> turtle.fd(50)
        """

    # Can return either 'int' or Tuple[int, ...] based on if the stamp is
    # a compound stamp or not. So, as per the "no Union return" policy,
    # we return Any.
    def stamp(self) -> Any:
        """Stamp a copy of the turtleshape onto the canvas and return its id.

        No argument.

        Stamp a copy of the turtle shape onto the canvas at the current
        turtle position. Return a stamp_id for that stamp, which can be
        used to delete it by calling clearstamp(stamp_id).

        Example (for a Turtle instance named turtle):
        >>> turtle.color("blue")
        >>> turtle.stamp()
        13
        >>> turtle.fd(50)
        """

    def clearstamp(self, stampid: int | tuple[int, ...]) -> None:
        """Delete stamp with given stampid

        Argument:
        stampid - an integer, must be return value of previous stamp() call.

        Example (for a Turtle instance named turtle):
        >>> turtle.color("blue")
        >>> astamp = turtle.stamp()
        >>> turtle.fd(50)
        >>> turtle.clearstamp(astamp)
        """

    def clearstamps(self, n: int | None = None) -> None:
        """Delete all or first/last n of turtle's stamps.

        Optional argument:
        n -- an integer

        If n is None, delete all of pen's stamps,
        else if n > 0 delete first n stamps
        else if n < 0 delete last n stamps.

        Example (for a Turtle instance named turtle):
        >>> for i in range(8):
        ...     turtle.stamp(); turtle.fd(30)
        ...
        >>> turtle.clearstamps(2)
        >>> turtle.clearstamps(-2)
        >>> turtle.clearstamps()
        """

    def filling(self) -> bool:
        """Return True if the turtle is filling, False otherwise.

        Example (for a Turtle instance named turtle):
            >>> turtle.begin_fill()
            >>> if turtle.filling():
            ...     turtle.pensize(5)
            ... else:
            ...     turtle.pensize(3)
        """

    if sys.version_info >= (3, 14):
        @contextmanager
        def fill(self) -> Generator[None]:
            """A context manager for filling a shape.

            Implicitly ensures the code block is wrapped with
            begin_fill() and end_fill().

            Example (for a Turtle instance named turtle):
            >>> turtle.color("black", "red")
            >>> with turtle.fill():
            ...     turtle.circle(60)
            """

    def begin_fill(self) -> None:
        """Called just before drawing a shape to be filled.

        No argument.

        Example (for a Turtle instance named turtle):
        >>> turtle.color("black", "red")
        >>> turtle.begin_fill()
        >>> turtle.circle(60)
        >>> turtle.end_fill()
        """

    def end_fill(self) -> None:
        """Fill the shape drawn after the call begin_fill().

        No argument.

        Example (for a Turtle instance named turtle):
        >>> turtle.color("black", "red")
        >>> turtle.begin_fill()
        >>> turtle.circle(60)
        >>> turtle.end_fill()
        """

    @overload
    def dot(self, size: int | _Color | None = None) -> None:
        """Draw a circular dot with diameter size, using color.

        If size is not given, the larger of pensize + 4 and 2 * pensize is
        used. If color is not given, the current pencolor is used.

        Arguments:
            size -- a positive integer or None
            color -- a colorstring or a numeric color tuple

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.dot()
            >>> turtle.fd(50); turtle.dot(20, "blue"); turtle.fd(50)
        """
    @overload
    def dot(self, size: int | None, color: _Color, /) -> None: ...
    @overload
    def dot(self, size: int | None, r: float, g: float, b: float, /) -> None: ...

    def write(
        self, arg: object, move: bool = False, align: str = "left", font: tuple[str, int, str] = ("Arial", 8, "normal")
    ) -> None:
        """Write text at the current turtle position.

        Arguments:
        arg -- info, which is to be written to the TurtleScreen
        move (optional) -- True/False
        align (optional) -- one of the strings "left", "center" or right"
        font (optional) -- a triple (fontname, fontsize, fonttype)

        Write text - the string representation of arg - at the current
        turtle position according to align ("left", "center" or right")
        and with the given font.
        If move is True, the pen is moved to the bottom-right corner
        of the text. By default, move is False.

        Example (for a Turtle instance named turtle):
        >>> turtle.write('Home = ', True, align="center")
        >>> turtle.write((0,0), True)
        """

    if sys.version_info >= (3, 14):
        @contextmanager
        def poly(self) -> Generator[None]:
            """A context manager for recording the vertices of a polygon.

            Implicitly ensures that the code block is wrapped with
            begin_poly() and end_poly()

            Example (for a Turtle instance named turtle) where we create a
            triangle as the polygon and move the turtle 100 steps forward:
            >>> with turtle.poly():
            ...     for side in range(3)
            ...         turtle.forward(50)
            ...         turtle.right(60)
            >>> turtle.forward(100)
            """

    def begin_poly(self) -> None:
        """Start recording the vertices of a polygon.

        The current turtle position is the first vertex of the polygon.
        """

    def end_poly(self) -> None:
        """Stop recording the vertices of a polygon.

        The current turtle position is the last vertex of the polygon.
        This will be connected with the first vertex.
        """

    def get_poly(self) -> _PolygonCoords | None:
        """Return the last recorded polygon.

        Example (for a Turtle instance named turtle):
            >>> turtle.home()
            >>> turtle.begin_poly()
            >>> turtle.fd(100)
            >>> turtle.left(20)
            >>> turtle.fd(30)
            >>> turtle.left(60)
            >>> turtle.fd(50)
            >>> turtle.end_poly()
            >>> p = turtle.get_poly()
            >>> register_shape("myFavouriteShape", p)
        """

    def getscreen(self) -> TurtleScreen:
        """Return the TurtleScreen object the turtle is drawing on.

        So TurtleScreen-methods can be called for that object.

        Example (for a Turtle instance named turtle):
            >>> ts = turtle.getscreen()
            >>> ts
            <turtle.TurtleScreen object at ...>
            >>> ts.bgcolor("pink")
        """

    def getturtle(self) -> Self:
        """Return the Turtle object itself.

        Aliases: getturtle | getpen

        Call getturtle() to get the turtle used by functions such as
        forward() and left().

        Example:
            >>> pet = getturtle()
            >>> pet.fd(50)
            >>> pet
            <turtle.Turtle object at 0x...>
        """

    getpen = getturtle
    def onclick(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-click events on this turtle.

        fun is called with the coordinates of the clicked point on the
        canvas. If fun is None, existing bindings are removed. btn is the
        number of the mouse button and defaults to 1, the left mouse
        button. If add is True, a new binding is added, otherwise it
        replaces a former binding.

        Arguments:
            fun -- a function with two arguments or None
            btn -- an integer
            add -- a boolean or None

        Example for the anonymous turtle, that is, the procedural way:
            >>> def turn(x, y):
            ...     left(180)
            ...
            >>> onclick(turn)  # Now clicking into the turtle will turn it.
            >>> onclick(None)  # event-binding will be removed
        """

    def onrelease(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-button-release event on this turtle on canvas.

        Arguments:
        fun -- a function with two arguments, to which will be assigned
                the coordinates of the clicked point on the canvas.
        btn --  number of the mouse-button defaults to 1 (left mouse button).

        Example (for a MyTurtle instance named joe):
        >>> class MyTurtle(Turtle):
        ...     def glow(self,x,y):
        ...             self.fillcolor("red")
        ...     def unglow(self,x,y):
        ...             self.fillcolor("")
        ...
        >>> joe = MyTurtle()
        >>> joe.onclick(joe.glow)
        >>> joe.onrelease(joe.unglow)

        Clicking on joe turns fillcolor red, unclicking turns it to
        transparent.
        """

    def ondrag(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-move event on this turtle on canvas.

        Arguments:
        fun -- a function with two arguments, to which will be assigned
               the coordinates of the clicked point on the canvas.
        btn -- number of the mouse-button defaults to 1 (left mouse button).

        Every sequence of mouse-move-events on a turtle is preceded by a
        mouse-click event on that turtle.

        Example (for a Turtle instance named turtle):
        >>> turtle.ondrag(turtle.goto)

        Subsequently clicking and dragging a Turtle will move it
        across the screen thereby producing handdrawings (if pen is
        down).
        """

    def undo(self) -> None:
        """undo (repeatedly) the last turtle action.

        No argument.

        undo (repeatedly) the last turtle action.
        Number of available undo actions is determined by the size of
        the undobuffer.

        Example (for a Turtle instance named turtle):
        >>> for i in range(4):
        ...     turtle.fd(50); turtle.lt(80)
        ...
        >>> for i in range(8):
        ...     turtle.undo()
        ...
        """

    turtlesize = shapesize

class _Screen(TurtleScreen):
    def __init__(self) -> None: ...
    # Note int and float are interpreted differently, hence the Union instead of just float
    def setup(
        self,
        width: int | float = 0.5,  # noqa: Y041
        height: int | float = 0.75,  # noqa: Y041
        startx: int | None = None,
        starty: int | None = None,
    ) -> None:
        """Set the size and position of the main window.

        Arguments:
        width: as integer a size in pixels, as float a fraction of the screen.
          Default is 50% of screen.
        height: as integer the height in pixels, as float a fraction of the
          screen. Default is 75% of screen.
        startx: if positive, starting position in pixels from the left
          edge of the screen, if negative from the right edge
          Default, startx=None is to center window horizontally.
        starty: if positive, starting position in pixels from the top
          edge of the screen, if negative from the bottom edge
          Default, starty=None is to center window vertically.

        Examples (for a Screen instance named screen):
        >>> screen.setup (width=200, height=200, startx=0, starty=0)

        sets window to 200x200 pixels, in upper left of screen

        >>> screen.setup(width=.75, height=0.5, startx=None, starty=None)

        sets window to 75% of screen by 50% of screen and centers
        """

    def title(self, titlestring: str) -> None:
        """Set the title of the turtle window to titlestring.

        The title is shown in the title bar of the turtle graphics window.
        This is a method of the Screen class. It is not available for
        TurtleScreen objects.

        Argument:
            titlestring -- a string

        Example (for a Screen instance named screen):
            >>> screen.title("Welcome to the turtle zoo!")
        """

    def bye(self) -> None:
        """Close the turtle graphics window."""

    def exitonclick(self) -> None:
        """Go into mainloop until the mouse is clicked.

        No arguments.

        Bind bye() method to mouseclick on TurtleScreen.
        If "using_IDLE" - value in configuration dictionary is False
        (default value), enter mainloop.
        If IDLE with -n switch (no subprocess) is used, this value should be
        set to True in turtle.cfg. In this case IDLE's mainloop
        is active also for the client script.

        This is a method of the Screen-class and not available for
        TurtleScreen instances.

        Example (for a Screen instance named screen):
        >>> screen.exitonclick()

        """

class Turtle(RawTurtle):
    """RawTurtle auto-creating (scrolled) canvas.

    When a Turtle object is created or a function derived from some
    Turtle method is called a TurtleScreen object is automatically created.
    """

    def __init__(self, shape: str = "classic", undobuffersize: int = 1000, visible: bool = True) -> None: ...

RawPen = RawTurtle
Pen = Turtle

def write_docstringdict(filename: str = "turtle_docstringdict") -> None:
    """Create and write docstring-dictionary to file.

    Optional argument:
    filename -- a string, used as filename
                default value is turtle_docstringdict

    Has to be called explicitly, (not used by the turtle-graphics classes)
    The docstring dictionary will be written to the Python script <filename>.py
    It is intended to serve as a template for translation of the docstrings
    into different languages.
    """

# Functions copied from TurtleScreenBase:

def mainloop() -> None:
    """Start the event loop by calling Tkinter's mainloop function.

    Must be the last statement in a turtle graphics program. Must not
    be used if a script is run from within IDLE in -n mode (no
    subprocess).
    """

def textinput(title: str, prompt: str) -> str | None:
    """Pop up a dialog window for input of a string.

    title is the title of the dialog window and prompt is a text
    describing what information to input. Return the string entered.
    If the dialog is canceled, return None.

    Arguments:
        title -- a string
        prompt -- a string

    Example:
        >>> textinput("NIM", "Name of first player:")
    """

def numinput(
    title: str, prompt: str, default: float | None = None, minval: float | None = None, maxval: float | None = None
) -> float | None:
    """Pop up a dialog window for input of a number.

    title is the title of the dialog window and prompt is a text
    describing what numerical information to input. default is the
    default value, minval and maxval are the minimum and maximum
    values for input. If these are given, the number entered must be
    between minval and maxval. If not, a hint is issued and the dialog
    remains open for correction. Return the number entered.
    If the dialog is canceled, return None.

    Arguments:
        title -- a string
        prompt -- a string
        default -- a number or None
        minval -- a number or None
        maxval -- a number or None

    Example:
        >>> numinput("Poker", "Your stakes:", 1000,
        ...                 minval=10, maxval=10000)
    """

# Functions copied from TurtleScreen:

def clear() -> None:
    """Delete the turtle's drawings from the screen.

    The state and position of the turtle as well as the drawings of
    other turtles are not affected.

    Example:
        >>> clear()
    """

@overload
def mode(mode: None = None) -> str:
    """Set the turtle mode and perform a reset.

    mode is one of the strings "standard", "logo" or "world". If mode
    is not given, return the current mode.

    Mode "standard" is compatible with the old turtle module. Mode
    "logo" is compatible with most Logo turtle graphics. Mode "world"
    uses user-defined world coordinates, see setworldcoordinates(). In
    this mode, angles appear distorted if the x and y units are not of
    equal length.

         Mode      Initial turtle heading     positive angles
     ------------|-------------------------|-------------------
      "standard"    to the right (east)       counterclockwise
        "logo"        upward    (north)         clockwise

    Argument:
        mode -- a string or None

    Example:
        >>> mode("logo")  # resets turtle heading to north
        >>> mode()
        'logo'
    """
@overload
def mode(mode: Literal["standard", "logo", "world"]) -> None: ...

def setworldcoordinates(llx: float, lly: float, urx: float, ury: float) -> None:
    """Set up a user-defined coordinate system.

    llx and lly are the x and y coordinates of the lower left corner of
    the canvas. urx and ury are the x and y coordinates of the upper
    right corner. Switch to mode "world" if necessary, which performs a
    reset. If mode "world" is already active, all drawings are redrawn
    according to the new coordinates. In user-defined coordinate
    systems, angles may appear distorted, see mode().

    Arguments:
        llx -- a number
        lly -- a number
        urx -- a number
        ury -- a number

    Example:
        >>> reset()
        >>> setworldcoordinates(-50,-7.5,50,7.5)
        >>> for _ in range(72):
        ...     left(10)
        ...
        >>> for _ in range(8):
        ...     left(45); fd(2)   # a regular octagon
    """

def register_shape(name: str, shape: _PolygonCoords | Shape | None = None) -> None:
    """Add a turtle shape to the TurtleScreen's shape list.

    Aliases: register_shape | addshape

    It can be called in four different ways:

    (1) name is the name of an image file (PNG, GIF, PGM, and PPM) and
        shape is None. Install the corresponding image shape.
    (2) name is an arbitrary string and shape is the name of an image
        file (PNG, GIF, PGM, and PPM). Install the corresponding image
        shape.
    (3) name is an arbitrary string and shape is a tuple of pairs of
        coordinates. Install the corresponding polygon shape.
    (4) name is an arbitrary string and shape is a (compound) Shape
        object. Install the corresponding compound shape.

    Image shapes do not rotate when turning the turtle, so they do not
    display the heading of the turtle. To use a registered shape, call
    shape() with its name.

    Arguments:
        name -- a string
        shape -- a string, a tuple of pairs of numbers, a Shape object
                 or None

    Example:
        >>> register_shape("turtle.gif")
        >>> register_shape("turtle", "turtle.gif")
        >>> register_shape("triangle", ((5,-3), (0,5), (-5,-3)))
    """

@overload
def colormode(cmode: None = None) -> float:
    """Return the colormode or set it to 1.0 or 255.

    After setting it, the r, g and b values of color triples have to
    be between 0 and cmode.

    Argument:
        cmode -- a number or None

    Example:
        >>> colormode(1)
        >>> turtle.pencolor(240, 160, 80)
        Traceback (most recent call last):
             ...
        TurtleGraphicsError: bad color sequence: (240, 160, 80)
        >>> colormode()
        1.0
        >>> colormode(255)
        >>> colormode()
        255
        >>> turtle.pencolor(240,160,80)
    """
@overload
def colormode(cmode: float) -> None: ...

def reset() -> None:
    """Delete the turtle's drawings and re-center the

    The turtle's settings are restored to their default values.

    Example:
        >>> goto(0,-22)
        >>> left(100)
        >>> position()
        (0.00,-22.00)
        >>> heading()
        100.0
        >>> reset()
        >>> position()
        (0.00,0.00)
        >>> heading()
        0.0
    """

def turtles() -> list[Turtle]:
    """Return the list of turtles on the

    Example:
        >>> for turtle in turtles():
        ...     turtle.color("red")
    """

@overload
def bgcolor() -> _AnyColor:
    """Return or set the background color of the TurtleScreen.

    Four input formats are allowed:
      - bgcolor()
        Return the current background color as a color specification
        string or as a tuple (see example).  May be used as input
        to another color(), pencolor(), fillcolor() or bgcolor() call.
      - bgcolor(colorstring)
        Set the background color to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - bgcolor((r, g, b))
        Set the background color to the RGB color represented by
        the tuple of r, g, and b.  Each of r, g, and b must be
        between 0 and colormode, where colormode is either 1.0 or 255
        (see colormode()).
      - bgcolor(r, g, b)
        Set the background color to the RGB color represented by
        r, g, and b.  Each of r, g, and b must be between 0 and
        colormode.

    Example:
        >>> bgcolor("orange")
        >>> bgcolor()
        'orange'
        >>> colormode(255)
        >>> bgcolor("#800080")
        >>> bgcolor()
        (128.0, 0.0, 128.0)
    """
@overload
def bgcolor(color: _Color) -> None: ...
@overload
def bgcolor(r: float, g: float, b: float) -> None: ...

@overload
def tracer(n: None = None) -> int:
    """Turn turtle animation on or off and set the drawing delay.

    If n is given, only every n-th regular screen update is performed,
    which can be used to speed up the drawing of complex graphics. If
    n is 0, animation is turned off and the screen is only updated by
    calling update(). If delay is given, it sets the drawing delay, see
    delay(). When called without arguments, return the current value
    of n.

    Arguments:
        n -- a nonnegative integer or None
        delay -- a nonnegative integer or None

    Example:
        >>> tracer(8, 25)
        >>> dist = 2
        >>> for i in range(200):
        ...     fd(dist)
        ...     rt(90)
        ...     dist += 2
    """
@overload
def tracer(n: int, delay: int | None = None) -> None: ...

@overload
def delay(delay: None = None) -> int:
    """Return or set the drawing delay in milliseconds.

    This is approximately the time interval between two consecutive
    canvas updates. The longer the drawing delay, the slower the
    animation.

    Argument:
        delay -- a nonnegative integer or None

    Example:
        >>> delay()
        10
        >>> delay(5)
        >>> delay()
        5
    """
@overload
def delay(delay: int) -> None: ...

if sys.version_info >= (3, 14):
    @contextmanager
    def no_animation() -> Generator[None]:
        """Temporarily turn off turtle animation.

        The code inside the with block is not animated. Once the block is
        exited, the drawing appears. This is useful for drawing complex
        shapes where even the fastest speed setting is too slow.

        Example:
            >>> with no_animation():
            ...     for dist in range(2, 400, 2):
            ...         fd(dist)
            ...         rt(90)
        """

def update() -> None:
    """Perform a TurtleScreen update."""

def window_width() -> int:
    """Return the width of the turtle window.

    Example:
        >>> window_width()
        640
    """

def window_height() -> int:
    """Return the height of the turtle window.

    Example:
        >>> window_height()
        480
    """

def getcanvas() -> Canvas:
    """Return the Canvas of this TurtleScreen.

    Example:
        >>> cv = getcanvas()
        >>> cv
        <turtle.ScrolledCanvas object ...>
    """

def getshapes() -> list[str]:
    """Return a list of names of all currently available turtle shapes.

    Example:
        >>> getshapes()
        ['arrow', 'blank', 'circle', ..., 'turtle']
    """

def onclick(fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
    """Bind fun to mouse-click events on this

    fun is called with the coordinates of the clicked point on the
    canvas. If fun is None, existing bindings are removed. btn is the
    number of the mouse button and defaults to 1, the left mouse
    button. If add is True, a new binding is added, otherwise it
    replaces a former binding.

    Arguments:
        fun -- a function with two arguments or None
        btn -- an integer
        add -- a boolean or None

    Example for the anonymous turtle, that is, the procedural way:
        >>> def turn(x, y):
        ...     left(180)
        ...
        >>> onclick(turn)  # Now clicking into the turtle will turn it.
        >>> onclick(None)  # event-binding will be removed
    """

def onkey(fun: Callable[[], object], key: str) -> None:
    """Bind fun to key-release event of key.

    Arguments:
    fun -- a function with no arguments
    key -- a string: key (e.g. "a") or key-symbol (e.g. "space")

    In order to be able to register key-events, TurtleScreen
    must have focus. (See method listen.)

    Example:

    >>> def f():
    ...     fd(50)
    ...     lt(60)
    ...
    >>> onkey(f, "Up")
    >>> listen()

    Subsequently the turtle can be moved by repeatedly pressing
    the up-arrow key, consequently drawing a hexagon

    """

def listen(xdummy: float | None = None, ydummy: float | None = None) -> None:
    """Set focus on TurtleScreen (in order to collect key-events)

    No arguments.
    Dummy arguments are provided in order
    to be able to pass listen to the onclick method.

    Example:
    >>> listen()
    """

def ontimer(fun: Callable[[], object], t: int = 0) -> None:
    """Install a timer, which calls fun after t milliseconds.

    Arguments:
    fun -- a function with no arguments.
    t -- a number >= 0

    Example:

    >>> running = True
    >>> def f():
    ...     if running:
    ...             fd(50)
    ...             lt(60)
    ...             ontimer(f, 250)
    ...
    >>> f()   # makes the turtle marching around
    >>> running = False
    """

@overload
def bgpic(picname: None = None) -> str:
    """Set background image or return name of current backgroundimage.

    Optional argument:
    picname -- a string, name of an image file (PNG, GIF, PGM, and PPM) or "nopic".

    If picname is a filename, set the corresponding image as background.
    If picname is "nopic", delete backgroundimage, if present.
    If picname is None, return the filename of the current backgroundimage.

    Example:
    >>> bgpic()
    'nopic'
    >>> bgpic("landscape.gif")
    >>> bgpic()
    'landscape.gif'
    """
@overload
def bgpic(picname: str) -> None: ...

@overload
def screensize(canvwidth: None = None, canvheight: None = None, bg: None = None) -> tuple[int, int]:
    """Resize the canvas the turtles are drawing on.

    Optional arguments:
    canvwidth -- positive integer, new width of canvas in pixels
    canvheight --  positive integer, new height of canvas in pixels
    bg -- colorstring or color-tuple, new backgroundcolor
    If no arguments are given, return current (canvaswidth, canvasheight)

    Do not alter the drawing window. To observe hidden parts of
    the canvas use the scrollbars. (Can make visible those parts
    of a drawing, which were outside the canvas before!)

    Example (for a Turtle instance named turtle):
    >>> turtle.screensize(2000,1500)
    >>> # e.g. to search for an erroneously escaped turtle ;-)
    """
@overload
def screensize(canvwidth: int, canvheight: int, bg: _Color | None = None) -> None: ...

if sys.version_info >= (3, 14):
    def save(filename: StrPath, *, overwrite: bool = False) -> None:
        """Save the drawing as a PostScript file

        Arguments:
        filename -- a string, the path of the created file.
                    Must end with '.ps' or '.eps'.

        Optional arguments:
        overwrite -- boolean, if true, then existing files will be overwritten

        Example:
        >>> save('my_drawing.eps')
        """

onscreenclick = onclick
resetscreen = reset
clearscreen = clear
addshape = register_shape

def onkeypress(fun: Callable[[], object], key: str | None = None) -> None:
    """Bind fun to key-press event of key if key is given,
    or to any key-press-event if no key is given.

    Arguments:
    fun -- a function with no arguments
    key -- a string: key (e.g. "a") or key-symbol (e.g. "space")

    In order to be able to register key-events, TurtleScreen
    must have focus. (See method listen.)

    Example (for a TurtleScreen instance named screen
    and a Turtle instance named turtle):

    >>> def f():
    ...     fd(50)
    ...     lt(60)
    ...
    >>> onkeypress(f, "Up")
    >>> listen()

    Subsequently the turtle can be moved by repeatedly pressing
    the up-arrow key, or by keeping pressed the up-arrow key.
    consequently drawing a hexagon.
    """

onkeyrelease = onkey

# Functions copied from _Screen:

def setup(width: float = 0.5, height: float = 0.75, startx: int | None = None, starty: int | None = None) -> None:
    """Set the size and position of the main window.

    Arguments:
    width: as integer a size in pixels, as float a fraction of the
      Default is 50% of
    height: as integer the height in pixels, as float a fraction of the
       Default is 75% of
    startx: if positive, starting position in pixels from the left
      edge of the screen, if negative from the right edge
      Default, startx=None is to center window horizontally.
    starty: if positive, starting position in pixels from the top
      edge of the screen, if negative from the bottom edge
      Default, starty=None is to center window vertically.

    Examples:
    >>> setup (width=200, height=200, startx=0, starty=0)

    sets window to 200x200 pixels, in upper left of screen

    >>> setup(width=.75, height=0.5, startx=None, starty=None)

    sets window to 75% of screen by 50% of screen and centers
    """

def title(titlestring: str) -> None:
    """Set the title of the turtle window to titlestring.

    The title is shown in the title bar of the turtle graphics window.
    This is a method of the Screen class. It is not available for
    TurtleScreen objects.

    Argument:
        titlestring -- a string

    Example:
        >>> title("Welcome to the turtle zoo!")
    """

def bye() -> None:
    """Close the turtle graphics window."""

def exitonclick() -> None:
    """Go into mainloop until the mouse is clicked.

    No arguments.

    Bind bye() method to mouseclick on TurtleScreen.
    If "using_IDLE" - value in configuration dictionary is False
    (default value), enter mainloop.
    If IDLE with -n switch (no subprocess) is used, this value should be
    set to True in turtle.cfg. In this case IDLE's mainloop
    is active also for the client script.

    This is a method of the Screen-class and not available for
    TurtleScreen instances.

    Example:
    >>> exitonclick()

    """

def Screen() -> _Screen:
    """Return the single screen object.

    If it does not exist yet, create it. Otherwise return the existing one.
    """

# Functions copied from TNavigator:

def degrees(fullcircle: float = 360.0) -> None:
    """Set the angle measurement units to degrees.

    The number of degrees in a full circle is set to fullcircle, which
    defaults to 360.

    Argument:
        fullcircle -- a number

    Example:
        >>> home()
        >>> left(90)
        >>> heading()
        90.0

        >>> # Change angle measurement unit to grad (also known as gon,
        >>> # grade, or gradian and equals 1/100-th of the right angle.)
        >>> degrees(400.0)
        >>> heading()
        100.0
        >>> degrees(360)
        >>> heading()
        90.0
    """

def radians() -> None:
    """Set the angle measurement units to radians.

    Equivalent to degrees(2 * math.pi).

    Example:
        >>> home()
        >>> left(90)
        >>> heading()
        90.0
        >>> radians()
        >>> heading()
        1.5707963267948966
    """

def forward(distance: float) -> None:
    """Move the turtle forward by the specified distance.

    Aliases: forward | fd

    The turtle moves in the direction it is headed.

    Argument:
        distance -- a number

    Example:
        >>> position()
        (0.00,0.00)
        >>> forward(25)
        >>> position()
        (25.00,0.00)
        >>> forward(-75)
        >>> position()
        (-50.00,0.00)
    """

def back(distance: float) -> None:
    """Move the turtle backward by distance.

    Aliases: back | backward | bk

    The turtle moves opposite to the direction it is headed.
    The turtle's heading does not change.

    Argument:
        distance -- a number

    Example:
        >>> position()
        (0.00,0.00)
        >>> backward(30)
        >>> position()
        (-30.00,0.00)
    """

def right(angle: float) -> None:
    """Turn the turtle right by the specified angle.

    Aliases: right | rt

    The angle is measured in degrees by default; the unit can be
    changed with degrees() or radians(). How the heading is measured
    depends on the turtle mode, see mode().

    Argument:
        angle -- a number

    Example:
        >>> heading()
        22.0
        >>> right(45)
        >>> heading()
        337.0
    """

def left(angle: float) -> None:
    """Turn the turtle left by the specified angle.

    Aliases: left | lt

    The angle is measured in degrees by default; the unit can be
    changed with degrees() or radians(). How the heading is measured
    depends on the turtle mode, see mode().

    Argument:
        angle -- a number

    Example:
        >>> heading()
        22.0
        >>> left(45)
        >>> heading()
        67.0
    """

def pos() -> Vec2D:
    """Return the turtle's current location (x,y) as a Vec2D vector.

    Aliases: pos | position

    Example:
        >>> pos()
        (0.00,240.00)
    """

def xcor() -> float:
    """Return the turtle's x coordinate.

    Example:
        >>> home()
        >>> left(50)
        >>> forward(100)
        >>> print(round(xcor(), 5))
        64.27876
    """

def ycor() -> float:
    """Return the turtle's y coordinate

    Example:
        >>> reset()
        >>> left(60)
        >>> forward(100)
        >>> print(round(ycor(), 5))
        86.60254
    """

@overload
def goto(x: tuple[float, float], y: None = None) -> None:
    """Move the turtle to an absolute position.

    Aliases: goto | setpos | setposition

    If y is None, x must be a pair of coordinates or a Vec2D, for
    example as returned by pos(). If the pen is down, a line is drawn.
    The turtle's heading does not change.

    Arguments:
        x -- a number or a pair/vector of numbers
        y -- a number or None

    Example:
        >>> tp = pos()
        >>> tp
        (0.00,0.00)
        >>> goto(60,30)
        >>> pos()
        (60.00,30.00)
        >>> goto((20,80))
        >>> pos()
        (20.00,80.00)
        >>> goto(tp)
        >>> pos()
        (0.00,0.00)
    """
@overload
def goto(x: float, y: float) -> None: ...

def home() -> None:
    """Move the turtle to the origin, coordinates (0,0).

    The turtle's heading is set to its start orientation, which
    depends on the turtle mode, see mode().

    Example:
        >>> heading()
        90.0
        >>> position()
        (0.00,-10.00)
        >>> home()
        >>> position()
        (0.00,0.00)
        >>> heading()
        0.0
    """

def setx(x: float) -> None:
    """Set the turtle's x coordinate to x.

    The y coordinate is unchanged.

    Argument:
        x -- a number

    Example:
        >>> position()
        (0.00,240.00)
        >>> setx(10)
        >>> position()
        (10.00,240.00)
    """

def sety(y: float) -> None:
    """Set the turtle's y coordinate to y.

    The x coordinate is unchanged.

    Argument:
        y -- a number

    Example:
        >>> position()
        (0.00,40.00)
        >>> sety(-10)
        >>> position()
        (0.00,-10.00)
    """

@overload
def distance(x: TNavigator | tuple[float, float], y: None = None) -> float:
    """Return the distance from the turtle to (x,y) in turtle step units.

    If y is None, x must be a pair of coordinates, a Vec2D, for example
    as returned by pos(), or another

    Arguments:
        x -- a number or a pair/vector of numbers or a turtle instance
        y -- a number if x is a number, else None

    Example:
        >>> home()
        >>> distance(30,40)
        50.0
        >>> distance((30,40))
        50.0
        >>> joe = Turtle()
        >>> joe.forward(77)
        >>> distance(joe)
        77.0
    """
@overload
def distance(x: float, y: float) -> float: ...

@overload
def towards(x: TNavigator | tuple[float, float], y: None = None) -> float:
    """Return the angle of the line from the turtle's position to (x,y).

    If y is None, x must be a pair of coordinates, a Vec2D, for example
    as returned by pos(), or another  The angle is measured from
    the turtle's start orientation, which depends on the turtle mode,
    see mode().

    Arguments:
        x -- a number or a pair/vector of numbers or a turtle instance
        y -- a number if x is a number, else None

    Example:
        >>> goto(10, 10)
        >>> towards(0,0)
        225.0
    """
@overload
def towards(x: float, y: float) -> float: ...

def heading() -> float:
    """Return the turtle's current heading.

    The value depends on the turtle mode, see mode().

    Example:
        >>> home()
        >>> left(67)
        >>> heading()
        67.0
    """

def setheading(to_angle: float) -> None:
    """Set the turtle's heading to to_angle.

    Aliases: setheading | seth

    Here are some common directions in degrees:

         standard mode       logo mode
      -------------------|---------------
           0 - east           0 - north
          90 - north         90 - east
         180 - west         180 - south
         270 - south        270 - west

    Argument:
        to_angle -- a number

    Example:
        >>> setheading(90)
        >>> heading()
        90.0
    """

def circle(radius: float, extent: float | None = None, steps: int | None = None) -> None:
    """Draw a circle with the given radius.

    The center is radius units left of the turtle; extent, an angle,
    determines which part of the circle is drawn. If extent is not
    given, draw the entire circle. If extent is not a full circle, one
    endpoint of the arc is the current pen position. Draw the arc in
    the counterclockwise direction if radius is positive, otherwise in
    the clockwise direction. Finally, the turtle's heading is changed by
    extent.

    As the circle is approximated by an inscribed regular polygon,
    steps determines the number of steps to use. If not given, it
    will be calculated automatically. May be used to draw regular
    polygons.

    Arguments:
        radius -- a number
        extent -- a number or None
        steps -- an integer or None

    Example:
        >>> circle(50)
        >>> circle(120, 180)  # draw a semicircle
    """

fd = forward
bk = back
backward = back
rt = right
lt = left
position = pos
setpos = goto
setposition = goto
seth = setheading

# Functions copied from TPen:
@overload
def resizemode(rmode: None = None) -> str:
    """Set the resizemode to "auto", "user" or "noresize".

    If rmode is not given, return the current resizemode. The
    different resizemodes have the following effects:

      - "auto": the turtle is resized according to the value of
        pensize().
      - "user": the turtle is resized according to the stretch factors
        and outline width, which are set by shapesize().
      - "noresize": the turtle is not resized.

    resizemode("user") is called by shapesize() when used with
    arguments.

    Argument:
        rmode -- a string or None

    Example:
        >>> resizemode()
        'noresize'
        >>> resizemode("auto")
        >>> resizemode()
        'auto'
    """
@overload
def resizemode(rmode: Literal["auto", "user", "noresize"]) -> None: ...

@overload
def pensize(width: None = None) -> int:
    """Return or set the line thickness.

    Aliases: pensize | width

    If width is not given, return the current pensize. If resizemode
    is set to "auto" and the turtle shape is a polygon, that polygon is
    drawn with the same line thickness.

    Argument:
        width -- a positive number or None

    Example:
        >>> pensize()
        1
        >>> pensize(10)   # from here on lines of width 10 are drawn
    """
@overload
def pensize(width: int) -> None: ...

def penup() -> None:
    """Lift the pen.

    Aliases: penup | pu | up

    The turtle does not draw when it moves.
    """

def pendown() -> None:
    """Lower the pen.

    Aliases: pendown | pd | down

    The turtle draws a line when it moves.
    """

def isdown() -> bool:
    """Return True if the pen is down, False if it's up.

    Example:
        >>> penup()
        >>> isdown()
        False
        >>> pendown()
        >>> isdown()
        True
    """

@overload
def speed(speed: None = None) -> int:
    """Return or set the turtle's speed.

    If speed is not given, return the current speed. Otherwise set the
    turtle's speed to an integer from 0 to 10. If speed is a number
    greater than 10 or smaller than 0.5, the speed is set to 0. You
    can also use these names instead of numbers:

        "fastest": 0
        "fast": 10
        "normal": 6
        "slow": 3
        "slowest": 1

    Speeds from 1 to 10 give increasingly faster animation of line
    drawing and turtle turning. A speed of 0 means that no animation
    takes place: forward() and back() make the turtle jump, and
    likewise left() and right() make the turtle turn instantly.

    Argument:
        speed -- an integer, a string or None

    Example:
        >>> speed()
        3
        >>> speed('normal')
        >>> speed()
        6
        >>> speed(9)
        >>> speed()
        9
    """
@overload
def speed(speed: _Speed) -> None: ...

@overload
def pencolor() -> _AnyColor:
    """Return or set the pencolor.

    Four input formats are allowed:
      - pencolor()
        Return the current pencolor as color specification string or
        as a tuple (see example).  May be used as input to another
        color(), pencolor(), fillcolor() or bgcolor() call.
      - pencolor(colorstring)
        Set the pencolor to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - pencolor((r, g, b))
        Set the pencolor to the RGB color represented by the tuple of
        r, g, and b.  Each of r, g, and b must be between 0 and
        colormode, where colormode is either 1.0 or 255 (see
        colormode()).
      - pencolor(r, g, b)
        Set the pencolor to the RGB color represented by r, g, and b.
        Each of r, g, and b must be between 0 and colormode.

    If the turtle shape is a polygon, the outline of that polygon is
    drawn with the newly set pencolor.

    Example:
        >>> pencolor("brown")
        >>> pencolor()
        'brown'
        >>> colormode(255)
        >>> pencolor("#32c18f")
        >>> pencolor()
        (50.0, 193.0, 143.0)
    """
@overload
def pencolor(color: _Color) -> None: ...
@overload
def pencolor(r: float, g: float, b: float) -> None: ...

@overload
def fillcolor() -> _AnyColor:
    """Return or set the fillcolor.

    Four input formats are allowed:
      - fillcolor()
        Return the current fillcolor as color specification string,
        possibly in tuple format (see example).  May be used as
        input to another color(), pencolor(), fillcolor() or bgcolor() call.
      - fillcolor(colorstring)
        Set the fillcolor to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - fillcolor((r, g, b))
        Set the fillcolor to the RGB color represented by the tuple of
        r, g, and b.  Each of r, g, and b must be between 0 and
        colormode, where colormode is either 1.0 or 255 (see
        colormode()).
      - fillcolor(r, g, b)
        Set the fillcolor to the RGB color represented by r, g, and b.
        Each of r, g, and b must be between 0 and colormode.

    If the turtle shape is a polygon, the interior of that polygon is
    drawn with the newly set fillcolor.

    Example:
        >>> fillcolor("violet")
        >>> fillcolor()
        'violet'
        >>> colormode(255)
        >>> fillcolor("#ffffff")
        >>> fillcolor()
        (255.0, 255.0, 255.0)
    """
@overload
def fillcolor(color: _Color) -> None: ...
@overload
def fillcolor(r: float, g: float, b: float) -> None: ...

@overload
def color() -> tuple[_AnyColor, _AnyColor]:
    """Return or set the pencolor and fillcolor.

    Several input formats are allowed. They use 0 to 3 arguments as follows:
      - color()
        Return the current pencolor and the current fillcolor as
        a pair of color specification strings or tuples as returned
        by pencolor() and fillcolor().
      - color(colorstring), color((r,g,b)), color(r,g,b)
        Inputs as in pencolor(). Set both the pencolor and the
        fillcolor to the given value.
      - color(colorstring1, colorstring2), color((r1,g1,b1), (r2,g2,b2))
        Equivalent to pencolor(colorstring1) and fillcolor(colorstring2)
        and analogously if the other input format is used.

    If the turtle shape is a polygon, the outline and interior of that
    polygon are drawn with the newly set colors. For more information,
    see pencolor() and fillcolor().

    Example:
        >>> color("red", "green")
        >>> color()
        ('red', 'green')
        >>> colormode(255)
        >>> color("#285078", "#a0c8f0")
        >>> color()
        ((40.0, 80.0, 120.0), (160.0, 200.0, 240.0))
    """
@overload
def color(color: _Color) -> None: ...
@overload
def color(r: float, g: float, b: float) -> None: ...
@overload
def color(color1: _Color, color2: _Color) -> None: ...

def showturtle() -> None:
    """Make the turtle visible.

    Aliases: showturtle | st
    """

def hideturtle() -> None:
    """Make the turtle invisible.

    Aliases: hideturtle | ht

    If the pen is down, movement still draws.
    """

def isvisible() -> bool:
    """Return True if the turtle is visible, False if it's hidden.

    Example:
        >>> hideturtle()
        >>> isvisible()
        False
        >>> showturtle()
        >>> isvisible()
        True
    """

# Note: signatures 1 and 2 overlap unsafely when no arguments are provided
@overload
def pen() -> _PenState:
    """Return or set the pen's attributes.

    Arguments:
        pen -- a dictionary with some or all of the below listed keys.
        **pendict -- one or more keyword-arguments with the below
                     listed keys as keywords.

    Return or set the pen's attributes in a 'pen-dictionary'
    with the following key/value pairs:
       "shown"      :   True/False
       "pendown"    :   True/False
       "pencolor"   :   color-string or color-tuple
       "fillcolor"  :   color-string or color-tuple
       "pensize"    :   positive number
       "speed"      :   number in range 0..10
       "resizemode" :   "auto" or "user" or "noresize"
       "stretchfactor": (positive number, positive number)
       "shearfactor":   number
       "outline"    :   positive number
       "tilt"       :   number

    This dictionary can be used as argument for a subsequent
    pen()-call to restore the former pen-state. Moreover one
    or more of these attributes can be provided as keyword-arguments.
    This can be used to set several pen attributes in one statement.


    Examples:
    >>> pen(fillcolor="black", pencolor="red", pensize=10)
    >>> pen()
    {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
    'pencolor': 'red', 'pendown': True, 'fillcolor': 'black',
    'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
    >>> penstate=pen()
    >>> color("yellow","")
    >>> penup()
    >>> pen()
    {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
    'pencolor': 'yellow', 'pendown': False, 'fillcolor': '',
    'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
    >>> p.pen(penstate, fillcolor="green")
    >>> p.pen()
    {'pensize': 10, 'shown': True, 'resizemode': 'auto', 'outline': 1,
    'pencolor': 'red', 'pendown': True, 'fillcolor': 'green',
    'stretchfactor': (1,1), 'speed': 3, 'shearfactor': 0.0}
    """
@overload
def pen(
    pen: _PenState | None = None,
    *,
    shown: bool = ...,
    pendown: bool = ...,
    pencolor: _Color = ...,
    fillcolor: _Color = ...,
    pensize: int = ...,
    speed: int = ...,
    resizemode: Literal["auto", "user", "noresize"] = ...,
    stretchfactor: tuple[float, float] = ...,
    outline: int = ...,
    tilt: float = ...,
) -> None: ...

width = pensize
up = penup
pu = penup
pd = pendown
down = pendown
st = showturtle
ht = hideturtle

# Functions copied from RawTurtle:

def setundobuffer(size: int | None) -> None:
    """Set or disable the undo buffer.

    If size is an integer, an empty undo buffer of the given size is
    installed. size is the maximum number of turtle actions that can
    be undone by undo(). If size is None, the undo buffer is disabled.

    Argument:
        size -- an integer or None

    Example:
        >>> setundobuffer(42)
    """

def undobufferentries() -> int:
    """Return the number of entries in the undo buffer.

    Example:
        >>> while undobufferentries():
        ...     undo()
    """

@overload
def shape(name: None = None) -> str:
    """Set the turtle shape to the named shape.

    If name is not given, return the name of the current shape. The
    shape must exist in the TurtleScreen's shape dictionary. Initially
    there are the following shapes: "arrow", "turtle", "circle",
    "square", "triangle" and "classic". To add shapes, see the Screen
    method register_shape().

    Argument:
        name -- a string or None

    Example:
        >>> shape()
        'classic'
        >>> shape("turtle")
        >>> shape()
        'turtle'
    """
@overload
def shape(name: str) -> None: ...

if sys.version_info >= (3, 12):
    def teleport(x: float | None = None, y: float | None = None, *, fill_gap: bool = False) -> None:
        """Teleport, or instantly move, the turtle to an absolute position.

        Unlike goto(), no line is drawn. If x or y is not given, that
        coordinate is kept as it is. The turtle's heading does not change.

        If the turtle is filling a shape, the shape drawn so far is
        finished and filled before teleporting, and filling starts again
        at the new position. Pass fill_gap=True to keep filling one shape
        instead. The teleport then counts as an edge of the shape, as it
        would with goto().

        Arguments:
            x -- a number or None
            y -- a number or None
            fill_gap -- a boolean

        Example:
            >>> tp = pos()
            >>> tp
            (0.00,0.00)
            >>> teleport(60)
            >>> pos()
            (60.00,0.00)
            >>> teleport(y=10)
            >>> pos()
            (60.00,10.00)
            >>> teleport(20, 30)
            >>> pos()
            (20.00,30.00)
        """

# Unsafely overlaps when no arguments are provided
@overload
def shapesize() -> tuple[float, float, float]:
    """Return or set the turtle's stretch factors and outline width.

    Aliases: shapesize | turtlesize

    If no arguments are given, return the current values. Otherwise
    set resizemode to "user", so that the turtle is displayed
    stretched according to its stretch factors:

      - stretch_wid: the stretch factor perpendicular to the turtle's
        heading
      - stretch_len: the stretch factor in the direction of the
        turtle's heading
      - outline: the width of the shape's outline

    For example, shapesize(2, 1) makes the turtle twice as wide
    without changing its length. If only stretch_wid is given,
    stretch_len is set to the same value.

    Arguments:
        stretch_wid -- a positive number or None
        stretch_len -- a positive number or None
        outline -- a positive number or None

    Example:
        >>> shapesize()
        (1.0, 1.0, 1)
        >>> resizemode("user")
        >>> shapesize(5, 5, 12)
        >>> shapesize()
        (5, 5, 12)
        >>> shapesize(outline=8)
        >>> shapesize()
        (5, 5, 8)
    """
@overload
def shapesize(stretch_wid: float | None = None, stretch_len: float | None = None, outline: float | None = None) -> None: ...

@overload
def shearfactor(shear: None = None) -> float:
    """Set or return the current shearfactor.

    Optional argument: shear -- number, tangent of the shear angle

    Shear the turtleshape according to the given shearfactor shear,
    which is the tangent of the shear angle. DO NOT change the
    turtle's heading (direction of movement).
    If shear is not given: return the current shearfactor, i. e. the
    tangent of the shear angle, by which lines parallel to the
    heading of the turtle are sheared.

    Examples:
    >>> shape("circle")
    >>> shapesize(5,2)
    >>> shearfactor(0.5)
    >>> shearfactor()
    >>> 0.5
    """
@overload
def shearfactor(shear: float) -> None: ...

# Unsafely overlaps when no arguments are provided
@overload
def shapetransform() -> tuple[float, float, float, float]:
    """Set or return the current transformation matrix of the turtle shape.

    Optional arguments: t11, t12, t21, t22 -- numbers.

    If none of the matrix elements are given, return the transformation
    matrix.
    Otherwise set the given elements and transform the turtleshape
    according to the matrix consisting of first row t11, t12 and
    second row t21, 22.
    Modify stretchfactor, shearfactor and tiltangle according to the
    given matrix.

    Examples:
    >>> shape("square")
    >>> shapesize(4,2)
    >>> shearfactor(-0.5)
    >>> shapetransform()
    (4.0, -1.0, -0.0, 2.0)
    """
@overload
def shapetransform(
    t11: float | None = None, t12: float | None = None, t21: float | None = None, t22: float | None = None
) -> None: ...

def get_shapepoly() -> _PolygonCoords | None:
    """Return the current shape polygon as tuple of coordinate pairs.

    No argument.

    Examples:
    >>> shape("square")
    >>> shapetransform(4, -1, 0, 2)
    >>> get_shapepoly()
    ((50, -20), (30, 20), (-50, 20), (-30, -20))

    """

if sys.version_info < (3, 13):
    @deprecated("Deprecated since Python 3.1; removed in Python 3.13. Use `tiltangle()` instead.")
    def settiltangle(angle: float) -> None:
        """Rotate the turtleshape to point in the specified direction

        Argument: angle -- number

        Rotate the turtleshape to point in the direction specified by angle,
        regardless of its current tilt-angle. DO NOT change the turtle's
        heading (direction of movement).

        Deprecated since Python 3.1

        Examples:
        >>> shape("circle")
        >>> shapesize(5,2)
        >>> settiltangle(45)
        >>> stamp()
        >>> fd(50)
        >>> settiltangle(-45)
        >>> stamp()
        >>> fd(50)
        """

@overload
def tiltangle(angle: None = None) -> float:
    """Set or return the current tilt-angle.

    Optional argument: angle -- number

    Rotate the turtleshape to point in the direction specified by angle,
    regardless of its current tilt-angle. DO NOT change the turtle's
    heading (direction of movement).
    If angle is not given: return the current tilt-angle, i. e. the angle
    between the orientation of the turtleshape and the heading of the
    turtle (its direction of movement).

    Examples:
    >>> shape("circle")
    >>> shapesize(5, 2)
    >>> tiltangle()
    0.0
    >>> tiltangle(45)
    >>> tiltangle()
    45.0
    >>> stamp()
    >>> fd(50)
    >>> tiltangle(-45)
    >>> tiltangle()
    315.0
    >>> stamp()
    >>> fd(50)
    """
@overload
def tiltangle(angle: float) -> None: ...

def tilt(angle: float) -> None:
    """Rotate the turtleshape by angle.

    Argument:
    angle - a number

    Rotate the turtleshape by angle from its current tilt-angle,
    but do NOT change the turtle's heading (direction of movement).

    Examples:
    >>> shape("circle")
    >>> shapesize(5,2)
    >>> tilt(30)
    >>> fd(50)
    >>> tilt(30)
    >>> fd(50)
    """

# Can return either 'int' or Tuple[int, ...] based on if the stamp is
# a compound stamp or not. So, as per the "no Union return" policy,
# we return Any.
def stamp() -> Any:
    """Stamp a copy of the turtleshape onto the canvas and return its id.

    No argument.

    Stamp a copy of the turtle shape onto the canvas at the current
    turtle position. Return a stamp_id for that stamp, which can be
    used to delete it by calling clearstamp(stamp_id).

    Example:
    >>> color("blue")
    >>> stamp()
    13
    >>> fd(50)
    """

def clearstamp(stampid: int | tuple[int, ...]) -> None:
    """Delete stamp with given stampid

    Argument:
    stampid - an integer, must be return value of previous stamp() call.

    Example:
    >>> color("blue")
    >>> astamp = stamp()
    >>> fd(50)
    >>> clearstamp(astamp)
    """

def clearstamps(n: int | None = None) -> None:
    """Delete all or first/last n of turtle's stamps.

    Optional argument:
    n -- an integer

    If n is None, delete all of pen's stamps,
    else if n > 0 delete first n stamps
    else if n < 0 delete last n stamps.

    Example:
    >>> for i in range(8):
    ...     stamp(); fd(30)
    ...
    >>> clearstamps(2)
    >>> clearstamps(-2)
    >>> clearstamps()
    """

def filling() -> bool:
    """Return True if the turtle is filling, False otherwise.

    Example:
        >>> begin_fill()
        >>> if filling():
        ...     pensize(5)
        ... else:
        ...     pensize(3)
    """

if sys.version_info >= (3, 14):
    @contextmanager
    def fill() -> Generator[None]:
        """A context manager for filling a shape.

        Implicitly ensures the code block is wrapped with
        begin_fill() and end_fill().

        Example:
        >>> color("black", "red")
        >>> with fill():
        ...     circle(60)
        """

def begin_fill() -> None:
    """Called just before drawing a shape to be filled.

    No argument.

    Example:
    >>> color("black", "red")
    >>> begin_fill()
    >>> circle(60)
    >>> end_fill()
    """

def end_fill() -> None:
    """Fill the shape drawn after the call begin_fill().

    No argument.

    Example:
    >>> color("black", "red")
    >>> begin_fill()
    >>> circle(60)
    >>> end_fill()
    """

@overload
def dot(size: int | _Color | None = None) -> None:
    """Draw a circular dot with diameter size, using color.

    If size is not given, the larger of pensize + 4 and 2 * pensize is
    used. If color is not given, the current pencolor is used.

    Arguments:
        size -- a positive integer or None
        color -- a colorstring or a numeric color tuple

    Example:
        >>> home()
        >>> dot()
        >>> fd(50); dot(20, "blue"); fd(50)
    """
@overload
def dot(size: int | None, color: _Color, /) -> None: ...
@overload
def dot(size: int | None, r: float, g: float, b: float, /) -> None: ...

def write(arg: object, move: bool = False, align: str = "left", font: tuple[str, int, str] = ("Arial", 8, "normal")) -> None:
    """Write text at the current turtle position.

    Arguments:
    arg -- info, which is to be written to the TurtleScreen
    move (optional) -- True/False
    align (optional) -- one of the strings "left", "center" or right"
    font (optional) -- a triple (fontname, fontsize, fonttype)

    Write text - the string representation of arg - at the current
    turtle position according to align ("left", "center" or right")
    and with the given font.
    If move is True, the pen is moved to the bottom-right corner
    of the text. By default, move is False.

    Example:
    >>> write('Home = ', True, align="center")
    >>> write((0,0), True)
    """

if sys.version_info >= (3, 14):
    @contextmanager
    def poly() -> Generator[None]:
        """A context manager for recording the vertices of a polygon.

        Implicitly ensures that the code block is wrapped with
        begin_poly() and end_poly()

        Example (for a Turtle instance named turtle) where we create a
        triangle as the polygon and move the turtle 100 steps forward:
        >>> with poly():
        ...     for side in range(3)
        ...         forward(50)
        ...         right(60)
        >>> forward(100)
        """

def begin_poly() -> None:
    """Start recording the vertices of a polygon.

    The current turtle position is the first vertex of the polygon.
    """

def end_poly() -> None:
    """Stop recording the vertices of a polygon.

    The current turtle position is the last vertex of the polygon.
    This will be connected with the first vertex.
    """

def get_poly() -> _PolygonCoords | None:
    """Return the last recorded polygon.

    Example:
        >>> home()
        >>> begin_poly()
        >>> fd(100)
        >>> left(20)
        >>> fd(30)
        >>> left(60)
        >>> fd(50)
        >>> end_poly()
        >>> p = get_poly()
        >>> register_shape("myFavouriteShape", p)
    """

def getscreen() -> TurtleScreen:
    """Return the TurtleScreen object the turtle is drawing on.

    So TurtleScreen-methods can be called for that object.

    Example:
        >>> ts = getscreen()
        >>> ts
        <TurtleScreen object at ...>
        >>> ts.bgcolor("pink")
    """

def getturtle() -> Turtle:
    """Return the Turtle object itself.

    Aliases: getturtle | getpen

    Call getturtle() to get the turtle used by functions such as
    forward() and left().

    Example:
        >>> pet = getturtle()
        >>> pet.fd(50)
        >>> pet
        <Turtle object at 0x...>
    """

getpen = getturtle

def onrelease(fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
    """Bind fun to mouse-button-release event on this turtle on canvas.

    Arguments:
    fun -- a function with two arguments, to which will be assigned
            the coordinates of the clicked point on the canvas.
    btn --  number of the mouse-button defaults to 1 (left mouse button).

    Example (for a MyTurtle instance named joe):
    >>> class MyTurtle(Turtle):
    ...     def glow(self,x,y):
    ...             self.fillcolor("red")
    ...     def unglow(self,x,y):
    ...             self.fillcolor("")
    ...
    >>> joe = MyTurtle()
    >>> joe.onclick(joe.glow)
    >>> joe.onrelease(joe.unglow)

    Clicking on joe turns fillcolor red, unclicking turns it to
    transparent.
    """

def ondrag(fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
    """Bind fun to mouse-move event on this turtle on canvas.

    Arguments:
    fun -- a function with two arguments, to which will be assigned
           the coordinates of the clicked point on the canvas.
    btn -- number of the mouse-button defaults to 1 (left mouse button).

    Every sequence of mouse-move-events on a turtle is preceded by a
    mouse-click event on that

    Example:
    >>> ondrag(goto)

    Subsequently clicking and dragging a Turtle will move it
    across the screen thereby producing handdrawings (if pen is
    down).
    """

def undo() -> None:
    """undo (repeatedly) the last turtle action.

    No argument.

    undo (repeatedly) the last turtle action.
    Number of available undo actions is determined by the size of
    the undobuffer.

    Example:
    >>> for i in range(4):
    ...     fd(50); lt(80)
    ...
    >>> for i in range(8):
    ...     undo()
    ...
    """

turtlesize = shapesize

# Functions copied from RawTurtle with a few tweaks:

def clone() -> Turtle:
    """Create and return a clone of the

    The clone has the same position, heading and turtle properties.

    Example:
        >>> leonel = Turtle()
        >>> marco = leonel.clone()
    """

# Extra functions present only in the global scope:

done = mainloop
