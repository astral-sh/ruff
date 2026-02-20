"""
Turtle graphics is a popular way for introducing programming to
kids. It was part of the original Logo programming language developed
by Wally Feurzig and Seymour Papert in 1966.

Imagine a robotic turtle starting at (0, 0) in the x-y plane. After an ``import turtle``, give it
the command turtle.forward(15), and it moves (on-screen!) 15 pixels in
the direction it is facing, drawing a line as it moves. Give it the
command turtle.right(25), and it rotates in-place 25 degrees clockwise.

By combining together these and similar commands, intricate shapes and
pictures can easily be drawn.

----- turtle.py

This module is an extended reimplementation of turtle.py from the
Python standard distribution up to Python 2.5. (See: https://www.python.org)

It tries to keep the merits of turtle.py and to be (nearly) 100%
compatible with it. This means in the first place to enable the
learning programmer to use all the commands, classes and methods
interactively when using the module from within IDLE run with
the -n switch.

Roughly it has the following features added:

- Better animation of the turtle movements, especially of turning the
  turtle. So the turtles can more easily be used as a visual feedback
  instrument by the (beginning) programmer.

- Different turtle shapes, image files as turtle shapes, user defined
  and user controllable turtle shapes, among them compound
  (multicolored) shapes. Turtle shapes can be stretched and tilted, which
  makes turtles very versatile geometrical objects.

- Fine control over turtle movement and screen updates via delay(),
  and enhanced tracer() and speed() methods.

- Aliases for the most commonly used commands, like fd for forward etc.,
  following the early Logo traditions. This reduces the boring work of
  typing long sequences of commands, which often occur in a natural way
  when kids try to program fancy pictures on their first encounter with
  turtle graphics.

- Turtles now have an undo()-method with configurable undo-buffer.

- Some simple commands/methods for creating event driven programs
  (mouse-, key-, timer-events). Especially useful for programming games.

- A scrollable Canvas class. The default scrollable Canvas can be
  extended interactively as needed while playing around with the turtle(s).

- A TurtleScreen class with methods controlling background color or
  background image, window and canvas size and other properties of the
  TurtleScreen.

- There is a method, setworldcoordinates(), to install a user defined
  coordinate-system for the TurtleScreen.

- The implementation uses a 2-vector class named Vec2D, derived from tuple.
  This class is public, so it can be imported by the application programmer,
  which makes certain types of computations very natural and compact.

- Appearance of the TurtleScreen and the Turtles at startup/import can be
  configured by means of a turtle.cfg configuration file.
  The default configuration mimics the appearance of the old turtle module.

- If configured appropriately the module reads in docstrings from a docstring
  dictionary in some different language, supplied separately  and replaces
  the English ones by those read in. There is a utility function
  write_docstringdict() to write a dictionary with the original (English)
  docstrings to disc, so it can serve as a template for translations.

Behind the scenes there are some features included with possible
extensions in mind. These will be commented and documented elsewhere.
"""

import sys
from _typeshed import StrPath
from collections.abc import Callable, Generator, Sequence
from contextlib import contextmanager
from tkinter import Canvas, Frame, Misc, PhotoImage, Scrollbar
from typing import Any, ClassVar, Literal, TypedDict, overload, type_check_only
from typing_extensions import Self, TypeAlias, deprecated, disjoint_base

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
class ScrolledCanvas(Canvas, Frame):  # type: ignore[misc]
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
        """Adjust canvas and scrollbars according to given canvas size."""

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
        """Starts event loop - calling Tkinter's mainloop function.

        No argument.

        Must be last statement in a turtle graphics program.
        Must NOT be used if a script is run from within IDLE in -n mode
        (No subprocess) - for interactive use of turtle graphics.

        Example (for a TurtleScreen instance named screen):
        >>> screen.mainloop()

        """

    def textinput(self, title: str, prompt: str) -> str | None:
        """Pop up a dialog window for input of a string.

        Arguments: title is the title of the dialog window,
        prompt is a text mostly describing what information to input.

        Return the string input
        If the dialog is canceled, return None.

        Example (for a TurtleScreen instance named screen):
        >>> screen.textinput("NIM", "Name of first player:")

        """

    def numinput(
        self, title: str, prompt: str, default: float | None = None, minval: float | None = None, maxval: float | None = None
    ) -> float | None:
        """Pop up a dialog window for input of a number.

        Arguments: title is the title of the dialog window,
        prompt is a text mostly describing what numerical information to input.
        default: default value
        minval: minimum value for input
        maxval: maximum value for input

        The number input must be in the range minval .. maxval if these are
        given. If not, a hint is issued and the dialog remains open for
        correction. Return the number input.
        If the dialog is canceled,  return None.

        Example (for a TurtleScreen instance named screen):
        >>> screen.numinput("Poker", "Your stakes:", 1000, minval=10, maxval=10000)

        """

class Terminator(Exception):
    """Will be raised in TurtleScreen.update, if _RUNNING becomes False.

    This stops execution of a turtle graphics script.
    Main purpose: use in the Demo-Viewer turtle.Demo.py.
    """

class TurtleGraphicsError(Exception):
    """Some TurtleGraphics Error"""

class Shape:
    """Data structure modeling shapes.

    attribute _type is one of "polygon", "image", "compound"
    attribute _data is - depending on _type a poygon-tuple,
    an image or a list constructed using the addcomponent method.
    """

    def __init__(
        self, type_: Literal["polygon", "image", "compound"], data: _PolygonCoords | PhotoImage | None = None
    ) -> None: ...
    def addcomponent(self, poly: _PolygonCoords, fill: _Color, outline: _Color | None = None) -> None:
        """Add component to a shape of type compound.

        Arguments: poly is a polygon, i. e. a tuple of number pairs.
        fill is the fillcolor of the component,
        outline is the outline color of the component.

        call (for a Shapeobject namend s):
        --   s.addcomponent(((0,0), (10,10), (-10,10)), "red", "blue")

        Example:
        >>> poly = ((0,0),(10,-5),(0,10),(-10,-5))
        >>> s = Shape("compound")
        >>> s.addcomponent(poly, "red", "blue")
        >>> # .. add more components and then use register_shape()
        """

class TurtleScreen(TurtleScreenBase):
    """Provides screen oriented methods like bgcolor etc.

    Only relies upon the methods of TurtleScreenBase and NOT
    upon components of the underlying graphics toolkit -
    which is Tkinter in this case.
    """

    def __init__(
        self, cv: Canvas, mode: Literal["standard", "logo", "world"] = "standard", colormode: float = 1.0, delay: int = 10
    ) -> None: ...
    def clear(self) -> None:
        """Delete all drawings and all turtles from the TurtleScreen.

        No argument.

        Reset empty TurtleScreen to its initial state: white background,
        no backgroundimage, no eventbindings and tracing on.

        Example (for a TurtleScreen instance named screen):
        >>> screen.clear()

        Note: this method is not available as function.
        """

    @overload
    def mode(self, mode: None = None) -> str:
        """Set turtle-mode ('standard', 'logo' or 'world') and perform reset.

        Optional argument:
        mode -- one of the strings 'standard', 'logo' or 'world'

        Mode 'standard' is compatible with turtle.py.
        Mode 'logo' is compatible with most Logo-Turtle-Graphics.
        Mode 'world' uses userdefined 'worldcoordinates'. *Attention*: in
        this mode angles appear distorted if x/y unit-ratio doesn't equal 1.
        If mode is not given, return the current mode.

             Mode      Initial turtle heading     positive angles
         ------------|-------------------------|-------------------
          'standard'    to the right (east)       counterclockwise
            'logo'        upward    (north)         clockwise

        Examples:
        >>> mode('logo')   # resets turtle heading to north
        >>> mode()
        'logo'
        """

    @overload
    def mode(self, mode: Literal["standard", "logo", "world"]) -> None: ...
    def setworldcoordinates(self, llx: float, lly: float, urx: float, ury: float) -> None:
        """Set up a user defined coordinate-system.

        Arguments:
        llx -- a number, x-coordinate of lower left corner of canvas
        lly -- a number, y-coordinate of lower left corner of canvas
        urx -- a number, x-coordinate of upper right corner of canvas
        ury -- a number, y-coordinate of upper right corner of canvas

        Set up user coodinat-system and switch to mode 'world' if necessary.
        This performs a screen.reset. If mode 'world' is already active,
        all drawings are redrawn according to the new coordinates.

        But ATTENTION: in user-defined coordinatesystems angles may appear
        distorted. (see Screen.mode())

        Example (for a TurtleScreen instance named screen):
        >>> screen.setworldcoordinates(-10,-0.5,50,1.5)
        >>> for _ in range(36):
        ...     left(10)
        ...     forward(0.5)
        """

    def register_shape(self, name: str, shape: _PolygonCoords | Shape | None = None) -> None:
        """Adds a turtle shape to TurtleScreen's shapelist.

        Arguments:
        (1) name is the name of an image file (PNG, GIF, PGM, and PPM) and shape is None.
            Installs the corresponding image shape.
            !! Image-shapes DO NOT rotate when turning the turtle,
            !! so they do not display the heading of the turtle!
        (2) name is an arbitrary string and shape is the name of an image file (PNG, GIF, PGM, and PPM).
            Installs the corresponding image shape.
            !! Image-shapes DO NOT rotate when turning the turtle,
            !! so they do not display the heading of the turtle!
        (3) name is an arbitrary string and shape is a tuple
            of pairs of coordinates. Installs the corresponding
            polygon shape
        (4) name is an arbitrary string and shape is a
            (compound) Shape object. Installs the corresponding
            compound shape.
        To use a shape, you have to issue the command shape(shapename).

        call: register_shape("turtle.gif")
        --or: register_shape("tri", ((0,0), (10,10), (-10,10)))

        Example (for a TurtleScreen instance named screen):
        >>> screen.register_shape("triangle", ((5,-3),(0,5),(-5,-3)))

        """

    @overload
    def colormode(self, cmode: None = None) -> float:
        """Return the colormode or set it to 1.0 or 255.

        Optional argument:
        cmode -- one of the values 1.0 or 255

        r, g, b values of colortriples have to be in range 0..cmode.

        Example (for a TurtleScreen instance named screen):
        >>> screen.colormode()
        1.0
        >>> screen.colormode(255)
        >>> pencolor(240,160,80)
        """

    @overload
    def colormode(self, cmode: float) -> None: ...
    def reset(self) -> None:
        """Reset all Turtles on the Screen to their initial state.

        No argument.

        Example (for a TurtleScreen instance named screen):
        >>> screen.reset()
        """

    def turtles(self) -> list[Turtle]:
        """Return the list of turtles on the screen.

        Example (for a TurtleScreen instance named screen):
        >>> screen.turtles()
        [<turtle.Turtle object at 0x00E11FB0>]
        """

    @overload
    def bgcolor(self) -> _AnyColor:
        """Set or return backgroundcolor of the TurtleScreen.

        Four input formats are allowed:
          - bgcolor()
            Return the current background color as color specification
            string or as a tuple (see example).  May be used as input
            to another color/pencolor/fillcolor/bgcolor call.
          - bgcolor(colorstring)
            Set the background color to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - bgcolor((r, g, b))
            Set the background color to the RGB color represented by
            the tuple of r, g, and b.  Each of r, g, and b must be in
            the range 0..colormode, where colormode is either 1.0 or 255
            (see colormode()).
          - bgcolor(r, g, b)
            Set the background color to the RGB color represented by
            r, g, and b.  Each of r, g, and b must be in the range
            0..colormode.

        Example (for a TurtleScreen instance named screen):
        >>> screen.bgcolor("orange")
        >>> screen.bgcolor()
        'orange'
        >>> colormode(255)
        >>> screen.bgcolor('#800080')
        >>> screen.bgcolor()
        (128.0, 0.0, 128.0)
        """

    @overload
    def bgcolor(self, color: _Color) -> None: ...
    @overload
    def bgcolor(self, r: float, g: float, b: float) -> None: ...
    @overload
    def tracer(self, n: None = None) -> int:
        """Turns turtle animation on/off and set delay for update drawings.

        Optional arguments:
        n -- nonnegative  integer
        delay -- nonnegative  integer

        If n is given, only each n-th regular screen update is really performed.
        (Can be used to accelerate the drawing of complex graphics.)
        Second arguments sets delay value (see RawTurtle.delay())

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

        Optional argument:
        delay -- positive integer

        Example (for a TurtleScreen instance named screen):
        >>> screen.delay(15)
        >>> screen.delay()
        15
        """

    @overload
    def delay(self, delay: int) -> None: ...
    if sys.version_info >= (3, 14):
        @contextmanager
        def no_animation(self) -> Generator[None]:
            """Temporarily turn off auto-updating the screen.

            This is useful for drawing complex shapes where even the fastest setting
            is too slow. Once this context manager is exited, the drawing will
            be displayed.

            Example (for a TurtleScreen instance named screen
            and a Turtle instance named turtle):
            >>> with screen.no_animation():
            ...    turtle.circle(50)
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

        No argument.

        Example (for a Screen instance named screen):
        >>> cv = screen.getcanvas()
        >>> cv
        <turtle.ScrolledCanvas instance at 0x010742D8>
        """

    def getshapes(self) -> list[str]:
        """Return a list of names of all currently available turtle shapes.

        No argument.

        Example (for a TurtleScreen instance named screen):
        >>> screen.getshapes()
        ['arrow', 'blank', 'circle', ... , 'turtle']
        """

    def onclick(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-click event on canvas.

        Arguments:
        fun -- a function with two arguments, the coordinates of the
               clicked point on the canvas.
        btn -- the number of the mouse-button, defaults to 1

        Example (for a TurtleScreen instance named screen)

        >>> screen.onclick(goto)
        >>> # Subsequently clicking into the TurtleScreen will
        >>> # make the turtle move to the clicked point.
        >>> screen.onclick(None)
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
    """Navigation part of the RawTurtle.
    Implements methods for turtle movement.
    """

    START_ORIENTATION: dict[str, Vec2D]
    DEFAULT_MODE: str
    DEFAULT_ANGLEOFFSET: int
    DEFAULT_ANGLEORIENT: int
    def __init__(self, mode: Literal["standard", "logo", "world"] = "standard") -> None: ...
    def reset(self) -> None:
        """reset turtle to its initial values

        Will be overwritten by parent class
        """

    def degrees(self, fullcircle: float = 360.0) -> None:
        """Set angle measurement units to degrees.

        Optional argument:
        fullcircle -  a number

        Set angle measurement units, i. e. set number
        of 'degrees' for a full circle. Default value is
        360 degrees.

        Example (for a Turtle instance named turtle):
        >>> turtle.left(90)
        >>> turtle.heading()
        90

        Change angle measurement unit to grad (also known as gon,
        grade, or gradian and equals 1/100-th of the right angle.)
        >>> turtle.degrees(400.0)
        >>> turtle.heading()
        100

        """

    def radians(self) -> None:
        """Set the angle measurement units to radians.

        No arguments.

        Example (for a Turtle instance named turtle):
        >>> turtle.heading()
        90
        >>> turtle.radians()
        >>> turtle.heading()
        1.5707963267948966
        """
    if sys.version_info >= (3, 12):
        def teleport(self, x: float | None = None, y: float | None = None, *, fill_gap: bool = False) -> None:
            """To be overwritten by child class RawTurtle.
            Includes no TPen references.
            """

    def forward(self, distance: float) -> None:
        """Move the turtle forward by the specified distance.

        Aliases: forward | fd

        Argument:
        distance -- a number (integer or float)

        Move the turtle forward by the specified distance, in the direction
        the turtle is headed.

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

        Argument:
        distance -- a number

        Move the turtle backward by distance, opposite to the direction the
        turtle is headed. Do not change the turtle's heading.

        Example (for a Turtle instance named turtle):
        >>> turtle.position()
        (0.00,0.00)
        >>> turtle.backward(30)
        >>> turtle.position()
        (-30.00,0.00)
        """

    def right(self, angle: float) -> None:
        """Turn turtle right by angle units.

        Aliases: right | rt

        Argument:
        angle -- a number (integer or float)

        Turn turtle right by angle units. (Units are by default degrees,
        but can be set via the degrees() and radians() functions.)
        Angle orientation depends on mode. (See this.)

        Example (for a Turtle instance named turtle):
        >>> turtle.heading()
        22.0
        >>> turtle.right(45)
        >>> turtle.heading()
        337.0
        """

    def left(self, angle: float) -> None:
        """Turn turtle left by angle units.

        Aliases: left | lt

        Argument:
        angle -- a number (integer or float)

        Turn turtle left by angle units. (Units are by default degrees,
        but can be set via the degrees() and radians() functions.)
        Angle orientation depends on mode. (See this.)

        Example (for a Turtle instance named turtle):
        >>> turtle.heading()
        22.0
        >>> turtle.left(45)
        >>> turtle.heading()
        67.0
        """

    def pos(self) -> Vec2D:
        """Return the turtle's current location (x,y), as a Vec2D-vector.

        Aliases: pos | position

        No arguments.

        Example (for a Turtle instance named turtle):
        >>> turtle.pos()
        (0.00, 240.00)
        """

    def xcor(self) -> float:
        """Return the turtle's x coordinate.

        No arguments.

        Example (for a Turtle instance named turtle):
        >>> reset()
        >>> turtle.left(60)
        >>> turtle.forward(100)
        >>> print(turtle.xcor())
        50.0
        """

    def ycor(self) -> float:
        """Return the turtle's y coordinate
        ---
        No arguments.

        Example (for a Turtle instance named turtle):
        >>> reset()
        >>> turtle.left(60)
        >>> turtle.forward(100)
        >>> print(turtle.ycor())
        86.6025403784
        """

    @overload
    def goto(self, x: tuple[float, float], y: None = None) -> None:
        """Move turtle to an absolute position.

        Aliases: setpos | setposition | goto:

        Arguments:
        x -- a number      or     a pair/vector of numbers
        y -- a number             None

        call: goto(x, y)         # two coordinates
        --or: goto((x, y))       # a pair (tuple) of coordinates
        --or: goto(vec)          # e.g. as returned by pos()

        Move turtle to an absolute position. If the pen is down,
        a line will be drawn. The turtle's orientation does not change.

        Example (for a Turtle instance named turtle):
        >>> tp = turtle.pos()
        >>> tp
        (0.00,0.00)
        >>> turtle.setpos(60,30)
        >>> turtle.pos()
        (60.00,30.00)
        >>> turtle.setpos((20,80))
        >>> turtle.pos()
        (20.00,80.00)
        >>> turtle.setpos(tp)
        >>> turtle.pos()
        (0.00,0.00)
        """

    @overload
    def goto(self, x: float, y: float) -> None: ...
    def home(self) -> None:
        """Move turtle to the origin - coordinates (0,0).

        No arguments.

        Move turtle to the origin - coordinates (0,0) and set its
        heading to its start-orientation (which depends on mode).

        Example (for a Turtle instance named turtle):
        >>> turtle.home()
        """

    def setx(self, x: float) -> None:
        """Set the turtle's first coordinate to x

        Argument:
        x -- a number (integer or float)

        Set the turtle's first coordinate to x, leave second coordinate
        unchanged.

        Example (for a Turtle instance named turtle):
        >>> turtle.position()
        (0.00, 240.00)
        >>> turtle.setx(10)
        >>> turtle.position()
        (10.00, 240.00)
        """

    def sety(self, y: float) -> None:
        """Set the turtle's second coordinate to y

        Argument:
        y -- a number (integer or float)

        Set the turtle's first coordinate to x, second coordinate remains
        unchanged.

        Example (for a Turtle instance named turtle):
        >>> turtle.position()
        (0.00, 40.00)
        >>> turtle.sety(-10)
        >>> turtle.position()
        (0.00, -10.00)
        """

    @overload
    def distance(self, x: TNavigator | tuple[float, float], y: None = None) -> float:
        """Return the distance from the turtle to (x,y) in turtle step units.

        Arguments:
        x -- a number   or  a pair/vector of numbers   or   a turtle instance
        y -- a number       None                            None

        call: distance(x, y)         # two coordinates
        --or: distance((x, y))       # a pair (tuple) of coordinates
        --or: distance(vec)          # e.g. as returned by pos()
        --or: distance(mypen)        # where mypen is another turtle

        Example (for a Turtle instance named turtle):
        >>> turtle.pos()
        (0.00,0.00)
        >>> turtle.distance(30,40)
        50.0
        >>> pen = Turtle()
        >>> pen.forward(77)
        >>> turtle.distance(pen)
        77.0
        """

    @overload
    def distance(self, x: float, y: float) -> float: ...
    @overload
    def towards(self, x: TNavigator | tuple[float, float], y: None = None) -> float:
        """Return the angle of the line from the turtle's position to (x, y).

        Arguments:
        x -- a number   or  a pair/vector of numbers   or   a turtle instance
        y -- a number       None                            None

        call: distance(x, y)         # two coordinates
        --or: distance((x, y))       # a pair (tuple) of coordinates
        --or: distance(vec)          # e.g. as returned by pos()
        --or: distance(mypen)        # where mypen is another turtle

        Return the angle, between the line from turtle-position to position
        specified by x, y and the turtle's start orientation. (Depends on
        modes - "standard" or "logo")

        Example (for a Turtle instance named turtle):
        >>> turtle.pos()
        (10.00, 10.00)
        >>> turtle.towards(0,0)
        225.0
        """

    @overload
    def towards(self, x: float, y: float) -> float: ...
    def heading(self) -> float:
        """Return the turtle's current heading.

        No arguments.

        Example (for a Turtle instance named turtle):
        >>> turtle.left(67)
        >>> turtle.heading()
        67.0
        """

    def setheading(self, to_angle: float) -> None:
        """Set the orientation of the turtle to to_angle.

        Aliases:  setheading | seth

        Argument:
        to_angle -- a number (integer or float)

        Set the orientation of the turtle to to_angle.
        Here are some common directions in degrees:

         standard - mode:          logo-mode:
        -------------------|--------------------
           0 - east                0 - north
          90 - north              90 - east
         180 - west              180 - south
         270 - south             270 - west

        Example (for a Turtle instance named turtle):
        >>> turtle.setheading(90)
        >>> turtle.heading()
        90
        """

    def circle(self, radius: float, extent: float | None = None, steps: int | None = None) -> None:
        """Draw a circle with given radius.

        Arguments:
        radius -- a number
        extent (optional) -- a number
        steps (optional) -- an integer

        Draw a circle with given radius. The center is radius units left
        of the turtle; extent - an angle - determines which part of the
        circle is drawn. If extent is not given, draw the entire circle.
        If extent is not a full circle, one endpoint of the arc is the
        current pen position. Draw the arc in counterclockwise direction
        if radius is positive, otherwise in clockwise direction. Finally
        the direction of the turtle is changed by the amount of extent.

        As the circle is approximated by an inscribed regular polygon,
        steps determines the number of steps to use. If not given,
        it will be calculated automatically. Maybe used to draw regular
        polygons.

        call: circle(radius)                  # full circle
        --or: circle(radius, extent)          # arc
        --or: circle(radius, extent, steps)
        --or: circle(radius, steps=6)         # 6-sided polygon

        Example (for a Turtle instance named turtle):
        >>> turtle.circle(50)
        >>> turtle.circle(120, 180)  # semicircle
        """

    def speed(self, s: int | None = 0) -> int | None:
        """dummy method - to be overwritten by child class"""
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
        """Set resizemode to one of the values: "auto", "user", "noresize".

        (Optional) Argument:
        rmode -- one of the strings "auto", "user", "noresize"

        Different resizemodes have the following effects:
          - "auto" adapts the appearance of the turtle
                   corresponding to the value of pensize.
          - "user" adapts the appearance of the turtle according to the
                   values of stretchfactor and outlinewidth (outline),
                   which are set by shapesize()
          - "noresize" no adaption of the turtle's appearance takes place.
        If no argument is given, return current resizemode.
        resizemode("user") is called by a call of shapesize with arguments.


        Examples (for a Turtle instance named turtle):
        >>> turtle.resizemode("noresize")
        >>> turtle.resizemode()
        'noresize'
        """

    @overload
    def resizemode(self, rmode: Literal["auto", "user", "noresize"]) -> None: ...
    @overload
    def pensize(self, width: None = None) -> int:
        """Set or return the line thickness.

        Aliases:  pensize | width

        Argument:
        width -- positive number

        Set the line thickness to width or return it. If resizemode is set
        to "auto" and turtleshape is a polygon, that polygon is drawn with
        the same line thickness. If no argument is given, current pensize
        is returned.

        Example (for a Turtle instance named turtle):
        >>> turtle.pensize()
        1
        >>> turtle.pensize(10)   # from here on lines of width 10 are drawn
        """

    @overload
    def pensize(self, width: int) -> None: ...
    def penup(self) -> None:
        """Pull the pen up -- no drawing when moving.

        Aliases: penup | pu | up

        No argument

        Example (for a Turtle instance named turtle):
        >>> turtle.penup()
        """

    def pendown(self) -> None:
        """Pull the pen down -- drawing when moving.

        Aliases: pendown | pd | down

        No argument.

        Example (for a Turtle instance named turtle):
        >>> turtle.pendown()
        """

    def isdown(self) -> bool:
        """Return True if pen is down, False if it's up.

        No argument.

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

        Optional argument:
        speed -- an integer in the range 0..10 or a speedstring (see below)

        Set the turtle's speed to an integer value in the range 0 .. 10.
        If no argument is given: return current speed.

        If input is a number greater than 10 or smaller than 0.5,
        speed is set to 0.
        Speedstrings  are mapped to speedvalues in the following way:
            'fastest' :  0
            'fast'    :  10
            'normal'  :  6
            'slow'    :  3
            'slowest' :  1
        speeds from 1 to 10 enforce increasingly faster animation of
        line drawing and turtle turning.

        Attention:
        speed = 0 : *no* animation takes place. forward/back makes turtle jump
        and likewise left/right make the turtle turn instantly.

        Example (for a Turtle instance named turtle):
        >>> turtle.speed(3)
        """

    @overload
    def speed(self, speed: _Speed) -> None: ...
    @overload
    def pencolor(self) -> _AnyColor:
        """Return or set the pencolor.

        Arguments:
        Four input formats are allowed:
          - pencolor()
            Return the current pencolor as color specification string or
            as a tuple (see example).  May be used as input to another
            color/pencolor/fillcolor/bgcolor call.
          - pencolor(colorstring)
            Set pencolor to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - pencolor((r, g, b))
            Set pencolor to the RGB color represented by the tuple of
            r, g, and b.  Each of r, g, and b must be in the range
            0..colormode, where colormode is either 1.0 or 255 (see
            colormode()).
          - pencolor(r, g, b)
            Set pencolor to the RGB color represented by r, g, and b.
            Each of r, g, and b must be in the range 0..colormode.

        If turtleshape is a polygon, the outline of that polygon is drawn
        with the newly set pencolor.

        Example (for a Turtle instance named turtle):
        >>> turtle.pencolor('brown')
        >>> turtle.pencolor()
        'brown'
        >>> colormode(255)
        >>> turtle.pencolor('#32c18f')
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

        Arguments:
        Four input formats are allowed:
          - fillcolor()
            Return the current fillcolor as color specification string,
            possibly in tuple format (see example).  May be used as
            input to another color/pencolor/fillcolor/bgcolor call.
          - fillcolor(colorstring)
            Set fillcolor to colorstring, which is a Tk color
            specification string, such as "red", "yellow", or "#33cc8c".
          - fillcolor((r, g, b))
            Set fillcolor to the RGB color represented by the tuple of
            r, g, and b.  Each of r, g, and b must be in the range
            0..colormode, where colormode is either 1.0 or 255 (see
            colormode()).
          - fillcolor(r, g, b)
            Set fillcolor to the RGB color represented by r, g, and b.
            Each of r, g, and b must be in the range 0..colormode.

        If turtleshape is a polygon, the interior of that polygon is drawn
        with the newly set fillcolor.

        Example (for a Turtle instance named turtle):
        >>> turtle.fillcolor('violet')
        >>> turtle.fillcolor()
        'violet'
        >>> colormode(255)
        >>> turtle.fillcolor('#ffffff')
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

        Arguments:
        Several input formats are allowed.
        They use 0 to 3 arguments as follows:
          - color()
            Return the current pencolor and the current fillcolor as
            a pair of color specification strings or tuples as returned
            by pencolor() and fillcolor().
          - color(colorstring), color((r,g,b)), color(r,g,b)
            Inputs as in pencolor(), set both, fillcolor and pencolor,
            to the given value.
          - color(colorstring1, colorstring2), color((r1,g1,b1), (r2,g2,b2))
            Equivalent to pencolor(colorstring1) and fillcolor(colorstring2)
            and analogously if the other input format is used.

        If turtleshape is a polygon, outline and interior of that polygon
        is drawn with the newly set colors.
        For more info see: pencolor, fillcolor

        Example (for a Turtle instance named turtle):
        >>> turtle.color('red', 'green')
        >>> turtle.color()
        ('red', 'green')
        >>> colormode(255)
        >>> color(('#285078', '#a0c8f0'))
        >>> color()
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
            """To be overwritten by child class RawTurtle.
            Includes no TNavigator references.
            """

    def showturtle(self) -> None:
        """Makes the turtle visible.

        Aliases: showturtle | st

        No argument.

        Example (for a Turtle instance named turtle):
        >>> turtle.hideturtle()
        >>> turtle.showturtle()
        """

    def hideturtle(self) -> None:
        """Makes the turtle invisible.

        Aliases: hideturtle | ht

        No argument.

        It's a good idea to do this while you're in the
        middle of a complicated drawing, because hiding
        the turtle speeds up the drawing observably.

        Example (for a Turtle instance named turtle):
        >>> turtle.hideturtle()
        """

    def isvisible(self) -> bool:
        """Return True if the Turtle is shown, False if it's hidden.

        No argument.

        Example (for a Turtle instance named turtle):
        >>> turtle.hideturtle()
        >>> print(turtle.isvisible())
        False
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

class RawTurtle(TPen, TNavigator):  # type: ignore[misc]  # Conflicting methods in base classes
    """Animation part of the RawTurtle.
    Puts RawTurtle upon a TurtleScreen and provides tools for
    its animation.
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
        """Delete the turtle's drawings and restore its default values.

        No argument.

        Delete the turtle's drawings from the screen, re-center the turtle
        and set variables to the default values.

        Example (for a Turtle instance named turtle):
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
        """Set or disable undobuffer.

        Argument:
        size -- an integer or None

        If size is an integer an empty undobuffer of given size is installed.
        Size gives the maximum number of turtle-actions that can be undone
        by the undo() function.
        If size is None, no undobuffer is present.

        Example (for a Turtle instance named turtle):
        >>> turtle.setundobuffer(42)
        """

    def undobufferentries(self) -> int:
        """Return count of entries in the undobuffer.

        No argument.

        Example (for a Turtle instance named turtle):
        >>> while undobufferentries():
        ...     undo()
        """

    def clear(self) -> None:
        """Delete the turtle's drawings from the screen. Do not move turtle.

        No arguments.

        Delete the turtle's drawings from the screen. Do not move turtle.
        State and position of the turtle as well as drawings of other
        turtles are not affected.

        Examples (for a Turtle instance named turtle):
        >>> turtle.clear()
        """

    def clone(self) -> Self:
        """Create and return a clone of the turtle.

        No argument.

        Create and return a clone of the turtle with same position, heading
        and turtle properties.

        Example (for a Turtle instance named mick):
        mick = Turtle()
        joe = mick.clone()
        """

    @overload
    def shape(self, name: None = None) -> str:
        """Set turtle shape to shape with given name / return current shapename.

        Optional argument:
        name -- a string, which is a valid shapename

        Set turtle shape to shape with given name or, if name is not given,
        return name of current shape.
        Shape with name must exist in the TurtleScreen's shape dictionary.
        Initially there are the following polygon shapes:
        'arrow', 'turtle', 'circle', 'square', 'triangle', 'classic'.
        To learn about how to deal with shapes see Screen-method register_shape.

        Example (for a Turtle instance named turtle):
        >>> turtle.shape()
        'arrow'
        >>> turtle.shape("turtle")
        >>> turtle.shape()
        'turtle'
        """

    @overload
    def shape(self, name: str) -> None: ...
    # Unsafely overlaps when no arguments are provided
    @overload
    def shapesize(self) -> tuple[float, float, float]:
        """Set/return turtle's stretchfactors/outline. Set resizemode to "user".

        Optional arguments:
           stretch_wid : positive number
           stretch_len : positive number
           outline  : positive number

        Return or set the pen's attributes x/y-stretchfactors and/or outline.
        Set resizemode to "user".
        If and only if resizemode is set to "user", the turtle will be displayed
        stretched according to its stretchfactors:
        stretch_wid is stretchfactor perpendicular to orientation
        stretch_len is stretchfactor in direction of turtles orientation.
        outline determines the width of the shapes's outline.

        Examples (for a Turtle instance named turtle):
        >>> turtle.resizemode("user")
        >>> turtle.shapesize(5, 5, 12)
        >>> turtle.shapesize(outline=8)
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
        """Return fillstate (True if filling, False else).

        No argument.

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
        """Draw a dot with diameter size, using color.

        Optional arguments:
        size -- an integer >= 1 (if given)
        color -- a colorstring or a numeric color tuple

        Draw a circular dot with diameter size, using color.
        If size is not given, the maximum of pensize+4 and 2*pensize is used.

        Example (for a Turtle instance named turtle):
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

        No argument.

        Start recording the vertices of a polygon. Current turtle position
        is first point of polygon.

        Example (for a Turtle instance named turtle):
        >>> turtle.begin_poly()
        """

    def end_poly(self) -> None:
        """Stop recording the vertices of a polygon.

        No argument.

        Stop recording the vertices of a polygon. Current turtle position is
        last point of polygon. This will be connected with the first point.

        Example (for a Turtle instance named turtle):
        >>> turtle.end_poly()
        """

    def get_poly(self) -> _PolygonCoords | None:
        """Return the lastly recorded polygon.

        No argument.

        Example (for a Turtle instance named turtle):
        >>> p = turtle.get_poly()
        >>> turtle.register_shape("myFavouriteShape", p)
        """

    def getscreen(self) -> TurtleScreen:
        """Return the TurtleScreen object, the turtle is drawing  on.

        No argument.

        Return the TurtleScreen object, the turtle is drawing  on.
        So TurtleScreen-methods can be called for that object.

        Example (for a Turtle instance named turtle):
        >>> ts = turtle.getscreen()
        >>> ts
        <turtle.TurtleScreen object at 0x0106B770>
        >>> ts.bgcolor("pink")
        """

    def getturtle(self) -> Self:
        """Return the Turtleobject itself.

        No argument.

        Only reasonable use: as a function to return the 'anonymous turtle':

        Example:
        >>> pet = getturtle()
        >>> pet.fd(50)
        >>> pet
        <turtle.Turtle object at 0x0187D810>
        >>> turtles()
        [<turtle.Turtle object at 0x0187D810>]
        """
    getpen = getturtle
    def onclick(self, fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
        """Bind fun to mouse-click event on this turtle on canvas.

        Arguments:
        fun --  a function with two arguments, to which will be assigned
                the coordinates of the clicked point on the canvas.
        btn --  number of the mouse-button defaults to 1 (left mouse button).
        add --  True or False. If True, new binding will be added, otherwise
                it will replace a former binding.

        Example for the anonymous turtle, i. e. the procedural way:

        >>> def turn(x, y):
        ...     left(360)
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
        """Set title of turtle-window

        Argument:
        titlestring -- a string, to appear in the titlebar of the
                       turtle graphics window.

        This is a method of Screen-class. Not available for TurtleScreen-
        objects.

        Example (for a Screen instance named screen):
        >>> screen.title("Welcome to the turtle-zoo!")
        """

    def bye(self) -> None:
        """Shut the turtlegraphics window.

        Example (for a TurtleScreen instance named screen):
        >>> screen.bye()
        """

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
    """Starts event loop - calling Tkinter's mainloop function.

    No argument.

    Must be last statement in a turtle graphics program.
    Must NOT be used if a script is run from within IDLE in -n mode
    (No subprocess) - for interactive use of turtle graphics.

    Example:
    >>> mainloop()

    """

def textinput(title: str, prompt: str) -> str | None:
    """Pop up a dialog window for input of a string.

    Arguments: title is the title of the dialog window,
    prompt is a text mostly describing what information to input.

    Return the string input
    If the dialog is canceled, return None.

    Example:
    >>> textinput("NIM", "Name of first player:")

    """

def numinput(
    title: str, prompt: str, default: float | None = None, minval: float | None = None, maxval: float | None = None
) -> float | None:
    """Pop up a dialog window for input of a number.

    Arguments: title is the title of the dialog window,
    prompt is a text mostly describing what numerical information to input.
    default: default value
    minval: minimum value for input
    maxval: maximum value for input

    The number input must be in the range minval .. maxval if these are
    given. If not, a hint is issued and the dialog remains open for
    correction. Return the number input.
    If the dialog is canceled,  return None.

    Example:
    >>> numinput("Poker", "Your stakes:", 1000, minval=10, maxval=10000)

    """

# Functions copied from TurtleScreen:

def clear() -> None:
    """Delete the turtle's drawings from the screen. Do not move

    No arguments.

    Delete the turtle's drawings from the screen. Do not move
    State and position of the turtle as well as drawings of other
    turtles are not affected.

    Examples:
    >>> clear()
    """

@overload
def mode(mode: None = None) -> str:
    """Set turtle-mode ('standard', 'logo' or 'world') and perform reset.

    Optional argument:
    mode -- one of the strings 'standard', 'logo' or 'world'

    Mode 'standard' is compatible with turtle.py.
    Mode 'logo' is compatible with most Logo-Turtle-Graphics.
    Mode 'world' uses userdefined 'worldcoordinates'. *Attention*: in
    this mode angles appear distorted if x/y unit-ratio doesn't equal 1.
    If mode is not given, return the current mode.

         Mode      Initial turtle heading     positive angles
     ------------|-------------------------|-------------------
      'standard'    to the right (east)       counterclockwise
        'logo'        upward    (north)         clockwise

    Examples:
    >>> mode('logo')   # resets turtle heading to north
    >>> mode()
    'logo'
    """

@overload
def mode(mode: Literal["standard", "logo", "world"]) -> None: ...
def setworldcoordinates(llx: float, lly: float, urx: float, ury: float) -> None:
    """Set up a user defined coordinate-system.

    Arguments:
    llx -- a number, x-coordinate of lower left corner of canvas
    lly -- a number, y-coordinate of lower left corner of canvas
    urx -- a number, x-coordinate of upper right corner of canvas
    ury -- a number, y-coordinate of upper right corner of canvas

    Set up user coodinat-system and switch to mode 'world' if necessary.
    This performs a reset. If mode 'world' is already active,
    all drawings are redrawn according to the new coordinates.

    But ATTENTION: in user-defined coordinatesystems angles may appear
    distorted. (see Screen.mode())

    Example:
    >>> setworldcoordinates(-10,-0.5,50,1.5)
    >>> for _ in range(36):
    ...     left(10)
    ...     forward(0.5)
    """

def register_shape(name: str, shape: _PolygonCoords | Shape | None = None) -> None:
    """Adds a turtle shape to TurtleScreen's shapelist.

    Arguments:
    (1) name is the name of an image file (PNG, GIF, PGM, and PPM) and shape is None.
        Installs the corresponding image shape.
        !! Image-shapes DO NOT rotate when turning the turtle,
        !! so they do not display the heading of the turtle!
    (2) name is an arbitrary string and shape is the name of an image file (PNG, GIF, PGM, and PPM).
        Installs the corresponding image shape.
        !! Image-shapes DO NOT rotate when turning the turtle,
        !! so they do not display the heading of the turtle!
    (3) name is an arbitrary string and shape is a tuple
        of pairs of coordinates. Installs the corresponding
        polygon shape
    (4) name is an arbitrary string and shape is a
        (compound) Shape object. Installs the corresponding
        compound shape.
    To use a shape, you have to issue the command shape(shapename).

    call: register_shape("turtle.gif")
    --or: register_shape("tri", ((0,0), (10,10), (-10,10)))

    Example:
    >>> register_shape("triangle", ((5,-3),(0,5),(-5,-3)))

    """

@overload
def colormode(cmode: None = None) -> float:
    """Return the colormode or set it to 1.0 or 255.

    Optional argument:
    cmode -- one of the values 1.0 or 255

    r, g, b values of colortriples have to be in range 0..cmode.

    Example:
    >>> colormode()
    1.0
    >>> colormode(255)
    >>> pencolor(240,160,80)
    """

@overload
def colormode(cmode: float) -> None: ...
def reset() -> None:
    """Delete the turtle's drawings and restore its default values.

    No argument.

    Delete the turtle's drawings from the screen, re-center the turtle
    and set variables to the default values.

    Example:
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
    >>> turtles()
    [<turtle.Turtle object at 0x00E11FB0>]
    """

@overload
def bgcolor() -> _AnyColor:
    """Set or return backgroundcolor of the TurtleScreen.

    Four input formats are allowed:
      - bgcolor()
        Return the current background color as color specification
        string or as a tuple (see example).  May be used as input
        to another color/pencolor/fillcolor/bgcolor call.
      - bgcolor(colorstring)
        Set the background color to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - bgcolor((r, g, b))
        Set the background color to the RGB color represented by
        the tuple of r, g, and b.  Each of r, g, and b must be in
        the range 0..colormode, where colormode is either 1.0 or 255
        (see colormode()).
      - bgcolor(r, g, b)
        Set the background color to the RGB color represented by
        r, g, and b.  Each of r, g, and b must be in the range
        0..colormode.

    Example:
    >>> bgcolor("orange")
    >>> bgcolor()
    'orange'
    >>> colormode(255)
    >>> bgcolor('#800080')
    >>> bgcolor()
    (128.0, 0.0, 128.0)
    """

@overload
def bgcolor(color: _Color) -> None: ...
@overload
def bgcolor(r: float, g: float, b: float) -> None: ...
@overload
def tracer(n: None = None) -> int:
    """Turns turtle animation on/off and set delay for update drawings.

    Optional arguments:
    n -- nonnegative  integer
    delay -- nonnegative  integer

    If n is given, only each n-th regular screen update is really performed.
    (Can be used to accelerate the drawing of complex graphics.)
    Second arguments sets delay value (see RawTurtle.delay())

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

    Optional argument:
    delay -- positive integer

    Example:
    >>> delay(15)
    >>> delay()
    15
    """

@overload
def delay(delay: int) -> None: ...

if sys.version_info >= (3, 14):
    @contextmanager
    def no_animation() -> Generator[None]:
        """Temporarily turn off auto-updating the

        This is useful for drawing complex shapes where even the fastest setting
        is too slow. Once this context manager is exited, the drawing will
        be displayed.

        Example (for a TurtleScreen instance named screen
        and a Turtle instance named turtle):
        >>> with no_animation():
        ...    turtle.circle(50)
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

    No argument.

    Example:
    >>> cv = getcanvas()
    >>> cv
    <turtle.ScrolledCanvas instance at 0x010742D8>
    """

def getshapes() -> list[str]:
    """Return a list of names of all currently available turtle shapes.

    No argument.

    Example:
    >>> getshapes()
    ['arrow', 'blank', 'circle', ... , 'turtle']
    """

def onclick(fun: Callable[[float, float], object], btn: int = 1, add: bool | None = None) -> None:
    """Bind fun to mouse-click event on this turtle on canvas.

    Arguments:
    fun --  a function with two arguments, to which will be assigned
            the coordinates of the clicked point on the canvas.
    btn --  number of the mouse-button defaults to 1 (left mouse button).
    add --  True or False. If True, new binding will be added, otherwise
            it will replace a former binding.

    Example for the anonymous turtle, i. e. the procedural way:

    >>> def turn(x, y):
    ...     left(360)
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
    """Set title of turtle-window

    Argument:
    titlestring -- a string, to appear in the titlebar of the
                   turtle graphics window.

    This is a method of Screen-class. Not available for TurtleScreen-
    objects.

    Example:
    >>> title("Welcome to the turtle-zoo!")
    """

def bye() -> None:
    """Shut the turtlegraphics window.

    Example:
    >>> bye()
    """

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
    """Return the singleton screen object.
    If none exists at the moment, create a new one and return it,
    else return the existing one.
    """

# Functions copied from TNavigator:

def degrees(fullcircle: float = 360.0) -> None:
    """Set angle measurement units to degrees.

    Optional argument:
    fullcircle -  a number

    Set angle measurement units, i. e. set number
    of 'degrees' for a full circle. Default value is
    360 degrees.

    Example:
    >>> left(90)
    >>> heading()
    90

    Change angle measurement unit to grad (also known as gon,
    grade, or gradian and equals 1/100-th of the right angle.)
    >>> degrees(400.0)
    >>> heading()
    100

    """

def radians() -> None:
    """Set the angle measurement units to radians.

    No arguments.

    Example:
    >>> heading()
    90
    >>> radians()
    >>> heading()
    1.5707963267948966
    """

def forward(distance: float) -> None:
    """Move the turtle forward by the specified distance.

    Aliases: forward | fd

    Argument:
    distance -- a number (integer or float)

    Move the turtle forward by the specified distance, in the direction
    the turtle is headed.

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

    Argument:
    distance -- a number

    Move the turtle backward by distance, opposite to the direction the
    turtle is headed. Do not change the turtle's heading.

    Example:
    >>> position()
    (0.00,0.00)
    >>> backward(30)
    >>> position()
    (-30.00,0.00)
    """

def right(angle: float) -> None:
    """Turn turtle right by angle units.

    Aliases: right | rt

    Argument:
    angle -- a number (integer or float)

    Turn turtle right by angle units. (Units are by default degrees,
    but can be set via the degrees() and radians() functions.)
    Angle orientation depends on mode. (See this.)

    Example:
    >>> heading()
    22.0
    >>> right(45)
    >>> heading()
    337.0
    """

def left(angle: float) -> None:
    """Turn turtle left by angle units.

    Aliases: left | lt

    Argument:
    angle -- a number (integer or float)

    Turn turtle left by angle units. (Units are by default degrees,
    but can be set via the degrees() and radians() functions.)
    Angle orientation depends on mode. (See this.)

    Example:
    >>> heading()
    22.0
    >>> left(45)
    >>> heading()
    67.0
    """

def pos() -> Vec2D:
    """Return the turtle's current location (x,y), as a Vec2D-vector.

    Aliases: pos | position

    No arguments.

    Example:
    >>> pos()
    (0.00, 240.00)
    """

def xcor() -> float:
    """Return the turtle's x coordinate.

    No arguments.

    Example:
    >>> reset()
    >>> left(60)
    >>> forward(100)
    >>> print(xcor())
    50.0
    """

def ycor() -> float:
    """Return the turtle's y coordinate
    ---
    No arguments.

    Example:
    >>> reset()
    >>> left(60)
    >>> forward(100)
    >>> print(ycor())
    86.6025403784
    """

@overload
def goto(x: tuple[float, float], y: None = None) -> None:
    """Move turtle to an absolute position.

    Aliases: setpos | setposition | goto:

    Arguments:
    x -- a number      or     a pair/vector of numbers
    y -- a number             None

    call: goto(x, y)         # two coordinates
    --or: goto((x, y))       # a pair (tuple) of coordinates
    --or: goto(vec)          # e.g. as returned by pos()

    Move turtle to an absolute position. If the pen is down,
    a line will be drawn. The turtle's orientation does not change.

    Example:
    >>> tp = pos()
    >>> tp
    (0.00,0.00)
    >>> setpos(60,30)
    >>> pos()
    (60.00,30.00)
    >>> setpos((20,80))
    >>> pos()
    (20.00,80.00)
    >>> setpos(tp)
    >>> pos()
    (0.00,0.00)
    """

@overload
def goto(x: float, y: float) -> None: ...
def home() -> None:
    """Move turtle to the origin - coordinates (0,0).

    No arguments.

    Move turtle to the origin - coordinates (0,0) and set its
    heading to its start-orientation (which depends on mode).

    Example:
    >>> home()
    """

def setx(x: float) -> None:
    """Set the turtle's first coordinate to x

    Argument:
    x -- a number (integer or float)

    Set the turtle's first coordinate to x, leave second coordinate
    unchanged.

    Example:
    >>> position()
    (0.00, 240.00)
    >>> setx(10)
    >>> position()
    (10.00, 240.00)
    """

def sety(y: float) -> None:
    """Set the turtle's second coordinate to y

    Argument:
    y -- a number (integer or float)

    Set the turtle's first coordinate to x, second coordinate remains
    unchanged.

    Example:
    >>> position()
    (0.00, 40.00)
    >>> sety(-10)
    >>> position()
    (0.00, -10.00)
    """

@overload
def distance(x: TNavigator | tuple[float, float], y: None = None) -> float:
    """Return the distance from the turtle to (x,y) in turtle step units.

    Arguments:
    x -- a number   or  a pair/vector of numbers   or   a turtle instance
    y -- a number       None                            None

    call: distance(x, y)         # two coordinates
    --or: distance((x, y))       # a pair (tuple) of coordinates
    --or: distance(vec)          # e.g. as returned by pos()
    --or: distance(mypen)        # where mypen is another turtle

    Example:
    >>> pos()
    (0.00,0.00)
    >>> distance(30,40)
    50.0
    >>> pen = Turtle()
    >>> pen.forward(77)
    >>> distance(pen)
    77.0
    """

@overload
def distance(x: float, y: float) -> float: ...
@overload
def towards(x: TNavigator | tuple[float, float], y: None = None) -> float:
    """Return the angle of the line from the turtle's position to (x, y).

    Arguments:
    x -- a number   or  a pair/vector of numbers   or   a turtle instance
    y -- a number       None                            None

    call: distance(x, y)         # two coordinates
    --or: distance((x, y))       # a pair (tuple) of coordinates
    --or: distance(vec)          # e.g. as returned by pos()
    --or: distance(mypen)        # where mypen is another turtle

    Return the angle, between the line from turtle-position to position
    specified by x, y and the turtle's start orientation. (Depends on
    modes - "standard" or "logo")

    Example:
    >>> pos()
    (10.00, 10.00)
    >>> towards(0,0)
    225.0
    """

@overload
def towards(x: float, y: float) -> float: ...
def heading() -> float:
    """Return the turtle's current heading.

    No arguments.

    Example:
    >>> left(67)
    >>> heading()
    67.0
    """

def setheading(to_angle: float) -> None:
    """Set the orientation of the turtle to to_angle.

    Aliases:  setheading | seth

    Argument:
    to_angle -- a number (integer or float)

    Set the orientation of the turtle to to_angle.
    Here are some common directions in degrees:

     standard - mode:          logo-mode:
    -------------------|--------------------
       0 - east                0 - north
      90 - north              90 - east
     180 - west              180 - south
     270 - south             270 - west

    Example:
    >>> setheading(90)
    >>> heading()
    90
    """

def circle(radius: float, extent: float | None = None, steps: int | None = None) -> None:
    """Draw a circle with given radius.

    Arguments:
    radius -- a number
    extent (optional) -- a number
    steps (optional) -- an integer

    Draw a circle with given radius. The center is radius units left
    of the turtle; extent - an angle - determines which part of the
    circle is drawn. If extent is not given, draw the entire circle.
    If extent is not a full circle, one endpoint of the arc is the
    current pen position. Draw the arc in counterclockwise direction
    if radius is positive, otherwise in clockwise direction. Finally
    the direction of the turtle is changed by the amount of extent.

    As the circle is approximated by an inscribed regular polygon,
    steps determines the number of steps to use. If not given,
    it will be calculated automatically. Maybe used to draw regular
    polygons.

    call: circle(radius)                  # full circle
    --or: circle(radius, extent)          # arc
    --or: circle(radius, extent, steps)
    --or: circle(radius, steps=6)         # 6-sided polygon

    Example:
    >>> circle(50)
    >>> circle(120, 180)  # semicircle
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
    """Set resizemode to one of the values: "auto", "user", "noresize".

    (Optional) Argument:
    rmode -- one of the strings "auto", "user", "noresize"

    Different resizemodes have the following effects:
      - "auto" adapts the appearance of the turtle
               corresponding to the value of pensize.
      - "user" adapts the appearance of the turtle according to the
               values of stretchfactor and outlinewidth (outline),
               which are set by shapesize()
      - "noresize" no adaption of the turtle's appearance takes place.
    If no argument is given, return current resizemode.
    resizemode("user") is called by a call of shapesize with arguments.


    Examples:
    >>> resizemode("noresize")
    >>> resizemode()
    'noresize'
    """

@overload
def resizemode(rmode: Literal["auto", "user", "noresize"]) -> None: ...
@overload
def pensize(width: None = None) -> int:
    """Set or return the line thickness.

    Aliases:  pensize | width

    Argument:
    width -- positive number

    Set the line thickness to width or return it. If resizemode is set
    to "auto" and turtleshape is a polygon, that polygon is drawn with
    the same line thickness. If no argument is given, current pensize
    is returned.

    Example:
    >>> pensize()
    1
    >>> pensize(10)   # from here on lines of width 10 are drawn
    """

@overload
def pensize(width: int) -> None: ...
def penup() -> None:
    """Pull the pen up -- no drawing when moving.

    Aliases: penup | pu | up

    No argument

    Example:
    >>> penup()
    """

def pendown() -> None:
    """Pull the pen down -- drawing when moving.

    Aliases: pendown | pd | down

    No argument.

    Example:
    >>> pendown()
    """

def isdown() -> bool:
    """Return True if pen is down, False if it's up.

    No argument.

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

    Optional argument:
    speed -- an integer in the range 0..10 or a speedstring (see below)

    Set the turtle's speed to an integer value in the range 0 .. 10.
    If no argument is given: return current speed.

    If input is a number greater than 10 or smaller than 0.5,
    speed is set to 0.
    Speedstrings  are mapped to speedvalues in the following way:
        'fastest' :  0
        'fast'    :  10
        'normal'  :  6
        'slow'    :  3
        'slowest' :  1
    speeds from 1 to 10 enforce increasingly faster animation of
    line drawing and turtle turning.

    Attention:
    speed = 0 : *no* animation takes place. forward/back makes turtle jump
    and likewise left/right make the turtle turn instantly.

    Example:
    >>> speed(3)
    """

@overload
def speed(speed: _Speed) -> None: ...
@overload
def pencolor() -> _AnyColor:
    """Return or set the pencolor.

    Arguments:
    Four input formats are allowed:
      - pencolor()
        Return the current pencolor as color specification string or
        as a tuple (see example).  May be used as input to another
        color/pencolor/fillcolor/bgcolor call.
      - pencolor(colorstring)
        Set pencolor to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - pencolor((r, g, b))
        Set pencolor to the RGB color represented by the tuple of
        r, g, and b.  Each of r, g, and b must be in the range
        0..colormode, where colormode is either 1.0 or 255 (see
        colormode()).
      - pencolor(r, g, b)
        Set pencolor to the RGB color represented by r, g, and b.
        Each of r, g, and b must be in the range 0..colormode.

    If turtleshape is a polygon, the outline of that polygon is drawn
    with the newly set pencolor.

    Example:
    >>> pencolor('brown')
    >>> pencolor()
    'brown'
    >>> colormode(255)
    >>> pencolor('#32c18f')
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

    Arguments:
    Four input formats are allowed:
      - fillcolor()
        Return the current fillcolor as color specification string,
        possibly in tuple format (see example).  May be used as
        input to another color/pencolor/fillcolor/bgcolor call.
      - fillcolor(colorstring)
        Set fillcolor to colorstring, which is a Tk color
        specification string, such as "red", "yellow", or "#33cc8c".
      - fillcolor((r, g, b))
        Set fillcolor to the RGB color represented by the tuple of
        r, g, and b.  Each of r, g, and b must be in the range
        0..colormode, where colormode is either 1.0 or 255 (see
        colormode()).
      - fillcolor(r, g, b)
        Set fillcolor to the RGB color represented by r, g, and b.
        Each of r, g, and b must be in the range 0..colormode.

    If turtleshape is a polygon, the interior of that polygon is drawn
    with the newly set fillcolor.

    Example:
    >>> fillcolor('violet')
    >>> fillcolor()
    'violet'
    >>> colormode(255)
    >>> fillcolor('#ffffff')
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

    Arguments:
    Several input formats are allowed.
    They use 0 to 3 arguments as follows:
      - color()
        Return the current pencolor and the current fillcolor as
        a pair of color specification strings or tuples as returned
        by pencolor() and fillcolor().
      - color(colorstring), color((r,g,b)), color(r,g,b)
        Inputs as in pencolor(), set both, fillcolor and pencolor,
        to the given value.
      - color(colorstring1, colorstring2), color((r1,g1,b1), (r2,g2,b2))
        Equivalent to pencolor(colorstring1) and fillcolor(colorstring2)
        and analogously if the other input format is used.

    If turtleshape is a polygon, outline and interior of that polygon
    is drawn with the newly set colors.
    For more info see: pencolor, fillcolor

    Example:
    >>> color('red', 'green')
    >>> color()
    ('red', 'green')
    >>> colormode(255)
    >>> color(('#285078', '#a0c8f0'))
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
    """Makes the turtle visible.

    Aliases: showturtle | st

    No argument.

    Example:
    >>> hideturtle()
    >>> showturtle()
    """

def hideturtle() -> None:
    """Makes the turtle invisible.

    Aliases: hideturtle | ht

    No argument.

    It's a good idea to do this while you're in the
    middle of a complicated drawing, because hiding
    the turtle speeds up the drawing observably.

    Example:
    >>> hideturtle()
    """

def isvisible() -> bool:
    """Return True if the Turtle is shown, False if it's hidden.

    No argument.

    Example:
    >>> hideturtle()
    >>> print(isvisible())
    False
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
    """Set or disable undobuffer.

    Argument:
    size -- an integer or None

    If size is an integer an empty undobuffer of given size is installed.
    Size gives the maximum number of turtle-actions that can be undone
    by the undo() function.
    If size is None, no undobuffer is present.

    Example:
    >>> setundobuffer(42)
    """

def undobufferentries() -> int:
    """Return count of entries in the undobuffer.

    No argument.

    Example:
    >>> while undobufferentries():
    ...     undo()
    """

@overload
def shape(name: None = None) -> str:
    """Set turtle shape to shape with given name / return current shapename.

    Optional argument:
    name -- a string, which is a valid shapename

    Set turtle shape to shape with given name or, if name is not given,
    return name of current shape.
    Shape with name must exist in the TurtleScreen's shape dictionary.
    Initially there are the following polygon shapes:
    'arrow', 'turtle', 'circle', 'square', 'triangle', 'classic'.
    To learn about how to deal with shapes see Screen-method register_shape.

    Example:
    >>> shape()
    'arrow'
    >>> shape("turtle")
    >>> shape()
    'turtle'
    """

@overload
def shape(name: str) -> None: ...

if sys.version_info >= (3, 12):
    def teleport(x: float | None = None, y: float | None = None, *, fill_gap: bool = False) -> None:
        """Instantly move turtle to an absolute position.

        Arguments:
        x -- a number      or     None
        y -- a number             None
        fill_gap -- a boolean     This argument must be specified by name.

        call: teleport(x, y)         # two coordinates
        --or: teleport(x)            # teleport to x position, keeping y as is
        --or: teleport(y=y)          # teleport to y position, keeping x as is
        --or: teleport(x, y, fill_gap=True)
                                     # teleport but fill the gap in between

        Move turtle to an absolute position. Unlike goto(x, y), a line will not
        be drawn. The turtle's orientation does not change. If currently
        filling, the polygon(s) teleported from will be filled after leaving,
        and filling will begin again after teleporting. This can be disabled
        with fill_gap=True, which makes the imaginary line traveled during
        teleporting act as a fill barrier like in goto(x, y).

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
    """Set/return turtle's stretchfactors/outline. Set resizemode to "user".

    Optional arguments:
       stretch_wid : positive number
       stretch_len : positive number
       outline  : positive number

    Return or set the pen's attributes x/y-stretchfactors and/or outline.
    Set resizemode to "user".
    If and only if resizemode is set to "user", the turtle will be displayed
    stretched according to its stretchfactors:
    stretch_wid is stretchfactor perpendicular to orientation
    stretch_len is stretchfactor in direction of turtles orientation.
    outline determines the width of the shapes's outline.

    Examples:
    >>> resizemode("user")
    >>> shapesize(5, 5, 12)
    >>> shapesize(outline=8)
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
    """Return fillstate (True if filling, False else).

    No argument.

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
    """Draw a dot with diameter size, using color.

    Optional arguments:
    size -- an integer >= 1 (if given)
    color -- a colorstring or a numeric color tuple

    Draw a circular dot with diameter size, using color.
    If size is not given, the maximum of pensize+4 and 2*pensize is used.

    Example:
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

    No argument.

    Start recording the vertices of a polygon. Current turtle position
    is first point of polygon.

    Example:
    >>> begin_poly()
    """

def end_poly() -> None:
    """Stop recording the vertices of a polygon.

    No argument.

    Stop recording the vertices of a polygon. Current turtle position is
    last point of polygon. This will be connected with the first point.

    Example:
    >>> end_poly()
    """

def get_poly() -> _PolygonCoords | None:
    """Return the lastly recorded polygon.

    No argument.

    Example:
    >>> p = get_poly()
    >>> register_shape("myFavouriteShape", p)
    """

def getscreen() -> TurtleScreen:
    """Return the TurtleScreen object, the turtle is drawing  on.

    No argument.

    Return the TurtleScreen object, the turtle is drawing  on.
    So TurtleScreen-methods can be called for that object.

    Example:
    >>> ts = getscreen()
    >>> ts
    <TurtleScreen object at 0x0106B770>
    >>> ts.bgcolor("pink")
    """

def getturtle() -> Turtle:
    """Return the Turtleobject itself.

    No argument.

    Only reasonable use: as a function to return the 'anonymous turtle':

    Example:
    >>> pet = getturtle()
    >>> pet.fd(50)
    >>> pet
    <Turtle object at 0x0187D810>
    >>> turtles()
    [<Turtle object at 0x0187D810>]
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

    No argument.

    Create and return a clone of the turtle with same position, heading
    and turtle properties.

    Example (for a Turtle instance named mick):
    mick = Turtle()
    joe = mick.clone()
    """

# Extra functions present only in the global scope:

done = mainloop
