"""Utilities needed to emulate Python's interactive interpreter."""

import sys
from codeop import CommandCompiler, compile_command as compile_command
from collections.abc import Callable
from types import CodeType
from typing import Any

__all__ = ["InteractiveInterpreter", "InteractiveConsole", "interact", "compile_command"]

class InteractiveInterpreter:
    """Base class for InteractiveConsole.

    This class deals with parsing and interpreter state (the user's
    namespace); it doesn't deal with input buffering or prompting or
    input file naming (the filename is always passed in explicitly).

    """

    locals: dict[str, Any]  # undocumented
    compile: CommandCompiler  # undocumented
    def __init__(self, locals: dict[str, Any] | None = None) -> None:
        """Constructor.

        The optional 'locals' argument specifies a mapping to use as the
        namespace in which code will be executed; it defaults to a newly
        created dictionary with key "__name__" set to "__console__" and
        key "__doc__" set to None.

        """

    def runsource(self, source: str, filename: str = "<input>", symbol: str = "single") -> bool:
        """Compile and run some source in the interpreter.

        Arguments are as for compile_command().

        One of several things can happen:

        1) The input is incorrect; compile_command() raised an
        exception (SyntaxError or OverflowError).  A syntax traceback
        will be printed by calling the showsyntaxerror() method.

        2) The input is incomplete, and more input is required;
        compile_command() returned None.  Nothing happens.

        3) The input is complete; compile_command() returned a code
        object.  The code is executed by calling self.runcode() (which
        also handles run-time exceptions, except for SystemExit).

        The return value is True in case 2, False in the other cases (unless
        an exception is raised).  The return value can be used to
        decide whether to use sys.ps1 or sys.ps2 to prompt the next
        line.

        """

    def runcode(self, code: CodeType) -> None:
        """Execute a code object.

        When an exception occurs, self.showtraceback() is called to
        display a traceback.  All exceptions are caught except
        SystemExit, which is reraised.

        A note about KeyboardInterrupt: this exception may occur
        elsewhere in this code, and may not always be caught.  The
        caller should be prepared to deal with it.

        """
    if sys.version_info >= (3, 13):
        def showsyntaxerror(self, filename: str | None = None, *, source: str = "") -> None:
            """Display the syntax error that just occurred.

            This doesn't display a stack trace because there isn't one.

            If a filename is given, it is stuffed in the exception instead
            of what was there before (because Python's parser always uses
            "<string>" when reading from a string).

            The output is written by self.write(), below.

            """
    else:
        def showsyntaxerror(self, filename: str | None = None) -> None:
            """Display the syntax error that just occurred.

            This doesn't display a stack trace because there isn't one.

            If a filename is given, it is stuffed in the exception instead
            of what was there before (because Python's parser always uses
            "<string>" when reading from a string).

            The output is written by self.write(), below.

            """

    def showtraceback(self) -> None:
        """Display the exception that just occurred.

        We remove the first stack item because it is our own code.

        The output is written by self.write(), below.

        """

    def write(self, data: str) -> None:
        """Write a string.

        The base implementation writes to sys.stderr; a subclass may
        replace this with a different implementation.

        """

class InteractiveConsole(InteractiveInterpreter):
    """Closely emulate the behavior of the interactive Python interpreter.

    This class builds on InteractiveInterpreter and adds prompting
    using the familiar sys.ps1 and sys.ps2, and input buffering.

    """

    buffer: list[str]  # undocumented
    filename: str  # undocumented
    if sys.version_info >= (3, 13):
        def __init__(
            self, locals: dict[str, Any] | None = None, filename: str = "<console>", *, local_exit: bool = False
        ) -> None:
            """Constructor.

            The optional locals argument will be passed to the
            InteractiveInterpreter base class.

            The optional filename argument should specify the (file)name
            of the input stream; it will show up in tracebacks.

            """

        def push(self, line: str, filename: str | None = None) -> bool:
            """Push a line to the interpreter.

            The line should not have a trailing newline; it may have
            internal newlines.  The line is appended to a buffer and the
            interpreter's runsource() method is called with the
            concatenated contents of the buffer as source.  If this
            indicates that the command was executed or invalid, the buffer
            is reset; otherwise, the command is incomplete, and the buffer
            is left as it was after the line was appended.  The return
            value is 1 if more input is required, 0 if the line was dealt
            with in some way (this is the same as runsource()).

            """
    else:
        def __init__(self, locals: dict[str, Any] | None = None, filename: str = "<console>") -> None:
            """Constructor.

            The optional locals argument will be passed to the
            InteractiveInterpreter base class.

            The optional filename argument should specify the (file)name
            of the input stream; it will show up in tracebacks.

            """

        def push(self, line: str) -> bool:
            """Push a line to the interpreter.

            The line should not have a trailing newline; it may have
            internal newlines.  The line is appended to a buffer and the
            interpreter's runsource() method is called with the
            concatenated contents of the buffer as source.  If this
            indicates that the command was executed or invalid, the buffer
            is reset; otherwise, the command is incomplete, and the buffer
            is left as it was after the line was appended.  The return
            value is 1 if more input is required, 0 if the line was dealt
            with in some way (this is the same as runsource()).

            """

    def interact(self, banner: str | None = None, exitmsg: str | None = None) -> None:
        """Closely emulate the interactive Python console.

        The optional banner argument specifies the banner to print
        before the first interaction; by default it prints a banner
        similar to the one printed by the real Python interpreter,
        followed by the current class name in parentheses (so as not
        to confuse this with the real interpreter -- since it's so
        close!).

        The optional exitmsg argument specifies the exit message
        printed when exiting. Pass the empty string to suppress
        printing an exit message. If exitmsg is not given or None,
        a default message is printed.

        """

    def resetbuffer(self) -> None:
        """Reset the input buffer."""

    def raw_input(self, prompt: str = "") -> str:
        """Write a prompt and read a line.

        The returned line does not include the trailing newline.
        When the user enters the EOF key sequence, EOFError is raised.

        The base implementation uses the built-in function
        input(); a subclass may replace this with a different
        implementation.

        """

if sys.version_info >= (3, 13):
    def interact(
        banner: str | None = None,
        readfunc: Callable[[str], str] | None = None,
        local: dict[str, Any] | None = None,
        exitmsg: str | None = None,
        local_exit: bool = False,
    ) -> None:
        """Closely emulate the interactive Python interpreter.

        This is a backwards compatible interface to the InteractiveConsole
        class.  When readfunc is not specified, it attempts to import the
        readline module to enable GNU readline if it is available.

        Arguments (all optional, all default to None):

        banner -- passed to InteractiveConsole.interact()
        readfunc -- if not None, replaces InteractiveConsole.raw_input()
        local -- passed to InteractiveInterpreter.__init__()
        exitmsg -- passed to InteractiveConsole.interact()
        local_exit -- passed to InteractiveConsole.__init__()

        """

else:
    def interact(
        banner: str | None = None,
        readfunc: Callable[[str], str] | None = None,
        local: dict[str, Any] | None = None,
        exitmsg: str | None = None,
    ) -> None:
        """Closely emulate the interactive Python interpreter.

        This is a backwards compatible interface to the InteractiveConsole
        class.  When readfunc is not specified, it attempts to import the
        readline module to enable GNU readline if it is available.

        Arguments (all optional, all default to None):

        banner -- passed to InteractiveConsole.interact()
        readfunc -- if not None, replaces InteractiveConsole.raw_input()
        local -- passed to InteractiveInterpreter.__init__()
        exitmsg -- passed to InteractiveConsole.interact()

        """
