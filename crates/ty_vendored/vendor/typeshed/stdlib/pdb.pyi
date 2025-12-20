"""
The Python Debugger Pdb
=======================

To use the debugger in its simplest form:

        >>> import pdb
        >>> pdb.run('<a statement>')

The debugger's prompt is '(Pdb) '.  This will stop in the first
function call in <a statement>.

Alternatively, if a statement terminated with an unhandled exception,
you can use pdb's post-mortem facility to inspect the contents of the
traceback:

        >>> <a statement>
        <exception traceback>
        >>> import pdb
        >>> pdb.pm()

The commands recognized by the debugger are listed in the next
section.  Most can be abbreviated as indicated; e.g., h(elp) means
that 'help' can be typed as 'h' or 'help' (but not as 'he' or 'hel',
nor as 'H' or 'Help' or 'HELP').  Optional arguments are enclosed in
square brackets.  Alternatives in the command syntax are separated
by a vertical bar (|).

A blank line repeats the previous command literally, except for
'list', where it lists the next 11 lines.

Commands that the debugger doesn't recognize are assumed to be Python
statements and are executed in the context of the program being
debugged.  Python statements can also be prefixed with an exclamation
point ('!').  This is a powerful way to inspect the program being
debugged; it is even possible to change variables or call functions.
When an exception occurs in such a statement, the exception name is
printed but the debugger's state is not changed.

The debugger supports aliases, which can save typing.  And aliases can
have parameters (see the alias help entry) which allows one a certain
level of adaptability to the context under examination.

Multiple commands may be entered on a single line, separated by the
pair ';;'.  No intelligence is applied to separating the commands; the
input is split at the first ';;', even if it is in the middle of a
quoted string.

If a file ".pdbrc" exists in your home directory or in the current
directory, it is read in and executed as if it had been typed at the
debugger prompt.  This is particularly useful for aliases.  If both
files exist, the one in the home directory is read first and aliases
defined there can be overridden by the local file.  This behavior can be
disabled by passing the "readrc=False" argument to the Pdb constructor.

Aside from aliases, the debugger is not directly programmable; but it
is implemented as a class from which you can derive your own debugger
class, which you can make as fancy as you like.


Debugger commands
=================

h(elp)

Without argument, print the list of available commands.
With a command name as argument, print help about that command.
"help pdb" shows the full pdb documentation.
"help exec" gives help on the ! command.

w(here) [count]

Print a stack trace. If count is not specified, print the full stack.
If count is 0, print the current frame entry. If count is positive,
print count entries from the most recent frame. If count is negative,
print -count entries from the least recent frame.
An arrow indicates the "current frame", which determines the
context of most commands.  'bt' is an alias for this command.

d(own) [count]

Move the current frame count (default one) levels down in the
stack trace (to a newer frame).

u(p) [count]

Move the current frame count (default one) levels up in the
stack trace (to an older frame).

b(reak) [ ([filename:]lineno | function) [, condition] ]

Without argument, list all breaks.

With a line number argument, set a break at this line in the
current file.  With a function name, set a break at the first
executable line of that function.  If a second argument is
present, it is a string specifying an expression which must
evaluate to true before the breakpoint is honored.

The line number may be prefixed with a filename and a colon,
to specify a breakpoint in another file (probably one that
hasn't been loaded yet).  The file is searched for on
sys.path; the .py suffix may be omitted.

tbreak [ ([filename:]lineno | function) [, condition] ]

Same arguments as break, but sets a temporary breakpoint: it
is automatically deleted when first hit.

cl(ear) [filename:lineno | bpnumber ...]

With a space separated list of breakpoint numbers, clear
those breakpoints.  Without argument, clear all breaks (but
first ask confirmation).  With a filename:lineno argument,
clear all breaks at that line in that file.

disable bpnumber [bpnumber ...]

Disables the breakpoints given as a space separated list of
breakpoint numbers.  Disabling a breakpoint means it cannot
cause the program to stop execution, but unlike clearing a
breakpoint, it remains in the list of breakpoints and can be
(re-)enabled.

enable bpnumber [bpnumber ...]

Enables the breakpoints given as a space separated list of
breakpoint numbers.

ignore bpnumber [count]

Set the ignore count for the given breakpoint number.  If
count is omitted, the ignore count is set to 0.  A breakpoint
becomes active when the ignore count is zero.  When non-zero,
the count is decremented each time the breakpoint is reached
and the breakpoint is not disabled and any associated
condition evaluates to true.

condition bpnumber [condition]

Set a new condition for the breakpoint, an expression which
must evaluate to true before the breakpoint is honored.  If
condition is absent, any existing condition is removed; i.e.,
the breakpoint is made unconditional.

(Pdb) commands [bpnumber]
(com) ...
(com) end
(Pdb)

Specify a list of commands for breakpoint number bpnumber.
The commands themselves are entered on the following lines.
Type a line containing just 'end' to terminate the commands.
The commands are executed when the breakpoint is hit.

To remove all commands from a breakpoint, type commands and
follow it immediately with end; that is, give no commands.

With no bpnumber argument, commands refers to the last
breakpoint set.

You can use breakpoint commands to start your program up
again.  Simply use the continue command, or step, or any other
command that resumes execution.

Specifying any command resuming execution (currently continue,
step, next, return, jump, quit and their abbreviations)
terminates the command list (as if that command was
immediately followed by end).  This is because any time you
resume execution (even with a simple next or step), you may
encounter another breakpoint -- which could have its own
command list, leading to ambiguities about which list to
execute.

If you use the 'silent' command in the command list, the usual
message about stopping at a breakpoint is not printed.  This
may be desirable for breakpoints that are to print a specific
message and then continue.  If none of the other commands
print anything, you will see no sign that the breakpoint was
reached.

s(tep)

Execute the current line, stop at the first possible occasion
(either in a function that is called or in the current
function).

n(ext)

Continue execution until the next line in the current function
is reached or it returns.

unt(il) [lineno]

Without argument, continue execution until the line with a
number greater than the current one is reached.  With a line
number, continue execution until a line with a number greater
or equal to that is reached.  In both cases, also stop when
the current frame returns.

j(ump) lineno

Set the next line that will be executed.  Only available in
the bottom-most frame.  This lets you jump back and execute
code again, or jump forward to skip code that you don't want
to run.

It should be noted that not all jumps are allowed -- for
instance it is not possible to jump into the middle of a
for loop or out of a finally clause.

r(eturn)

Continue execution until the current function returns.

retval

Print the return value for the last return of a function.

run [args...]

Restart the debugged python program. If a string is supplied
it is split with "shlex", and the result is used as the new
sys.argv.  History, breakpoints, actions and debugger options
are preserved.  "restart" is an alias for "run".

c(ont(inue))

Continue execution, only stop when a breakpoint is encountered.

l(ist) [first[, last] | .]

List source code for the current file.  Without arguments,
list 11 lines around the current line or continue the previous
listing.  With . as argument, list 11 lines around the current
line.  With one argument, list 11 lines starting at that line.
With two arguments, list the given range; if the second
argument is less than the first, it is a count.

The current line in the current frame is indicated by "->".
If an exception is being debugged, the line where the
exception was originally raised or propagated is indicated by
">>", if it differs from the current line.

ll | longlist

List the whole source code for the current function or frame.

a(rgs)

Print the argument list of the current function.

p expression

Print the value of the expression.

pp expression

Pretty-print the value of the expression.

whatis expression

Print the type of the argument.

source expression

Try to get source code for the given object and display it.

display [expression]

Display the value of the expression if it changed, each time execution
stops in the current frame.

Without expression, list all display expressions for the current frame.

undisplay [expression]

Do not display the expression any more in the current frame.

Without expression, clear all display expressions for the current frame.

interact

Start an interactive interpreter whose global namespace
contains all the (global and local) names found in the current scope.

alias [name [command]]

Create an alias called 'name' that executes 'command'.  The
command must *not* be enclosed in quotes.  Replaceable
parameters can be indicated by %1, %2, and so on, while %* is
replaced by all the parameters.  If no command is given, the
current alias for name is shown. If no name is given, all
aliases are listed.

Aliases may be nested and can contain anything that can be
legally typed at the pdb prompt.  Note!  You *can* override
internal pdb commands with aliases!  Those internal commands
are then hidden until the alias is removed.  Aliasing is
recursively applied to the first word of the command line; all
other words in the line are left alone.

As an example, here are two useful aliases (especially when
placed in the .pdbrc file):

# Print instance variables (usage "pi classInst")
alias pi for k in %1.__dict__.keys(): print("%1.",k,"=",%1.__dict__[k])
# Print instance variables in self
alias ps pi self

unalias name

Delete the specified alias.

debug code

Enter a recursive debugger that steps through the code
argument (which is an arbitrary expression or statement to be
executed in the current environment).

q(uit) | exit

Quit from the debugger. The program being executed is aborted.

(!) statement

Execute the (one-line) statement in the context of the current
stack frame.  The exclamation point can be omitted unless the
first word of the statement resembles a debugger command, e.g.:
(Pdb) ! n=42
(Pdb)

To assign to a global variable you must always prefix the command with
a 'global' command, e.g.:
(Pdb) global list_options; list_options = ['-l']
(Pdb)
"""

import signal
import sys
from bdb import Bdb, _Backend
from cmd import Cmd
from collections.abc import Callable, Iterable, Mapping, Sequence
from inspect import _SourceObjectType
from linecache import _ModuleGlobals
from rlcompleter import Completer
from types import CodeType, FrameType, TracebackType
from typing import IO, Any, ClassVar, Final, Literal, TypeVar
from typing_extensions import ParamSpec, Self, TypeAlias

__all__ = ["run", "pm", "Pdb", "runeval", "runctx", "runcall", "set_trace", "post_mortem", "help"]
if sys.version_info >= (3, 14):
    __all__ += ["set_default_backend", "get_default_backend"]

_T = TypeVar("_T")
_P = ParamSpec("_P")
_Mode: TypeAlias = Literal["inline", "cli"]

line_prefix: Final[str]  # undocumented

class Restart(Exception):
    """Causes a debugger to be restarted for the debugged python program."""

def run(statement: str, globals: dict[str, Any] | None = None, locals: Mapping[str, Any] | None = None) -> None:
    """Execute the *statement* (given as a string or a code object)
    under debugger control.

    The debugger prompt appears before any code is executed; you can set
    breakpoints and type continue, or you can step through the statement
    using step or next.

    The optional *globals* and *locals* arguments specify the
    environment in which the code is executed; by default the
    dictionary of the module __main__ is used (see the explanation of
    the built-in exec() or eval() functions.).
    """

def runeval(expression: str, globals: dict[str, Any] | None = None, locals: Mapping[str, Any] | None = None) -> Any:
    """Evaluate the *expression* (given as a string or a code object)
    under debugger control.

    When runeval() returns, it returns the value of the expression.
    Otherwise this function is similar to run().
    """

def runctx(statement: str, globals: dict[str, Any], locals: Mapping[str, Any]) -> None: ...
def runcall(func: Callable[_P, _T], *args: _P.args, **kwds: _P.kwargs) -> _T | None:
    """Call the function (a function or method object, not a string)
    with the given arguments.

    When runcall() returns, it returns whatever the function call
    returned. The debugger prompt appears as soon as the function is
    entered.
    """

if sys.version_info >= (3, 14):
    def set_default_backend(backend: _Backend) -> None:
        """Set the default backend to use for Pdb instances."""

    def get_default_backend() -> _Backend:
        """Get the default backend to use for Pdb instances."""

    def set_trace(*, header: str | None = None, commands: Iterable[str] | None = None) -> None:
        """Enter the debugger at the calling stack frame.

        This is useful to hard-code a breakpoint at a given point in a
        program, even if the code is not otherwise being debugged (e.g. when
        an assertion fails). If given, *header* is printed to the console
        just before debugging begins. *commands* is an optional list of
        pdb commands to run when the debugger starts.
        """

    async def set_trace_async(*, header: str | None = None, commands: Iterable[str] | None = None) -> None:
        """Enter the debugger at the calling stack frame, but in async mode.

        This should be used as await pdb.set_trace_async(). Users can do await
        if they enter the debugger with this function. Otherwise it's the same
        as set_trace().
        """

else:
    def set_trace(*, header: str | None = None) -> None:
        """Enter the debugger at the calling stack frame.

        This is useful to hard-code a breakpoint at a given point in a
        program, even if the code is not otherwise being debugged (e.g. when
        an assertion fails). If given, *header* is printed to the console
        just before debugging begins.
        """

def post_mortem(t: TracebackType | None = None) -> None:
    """Enter post-mortem debugging of the given *traceback*, or *exception*
    object.

    If no traceback is given, it uses the one of the exception that is
    currently being handled (an exception must be being handled if the
    default is to be used).

    If `t` is an exception object, the `exceptions` command makes it possible to
    list and inspect its chained exceptions (if any).
    """

def pm() -> None:
    """Enter post-mortem debugging of the traceback found in sys.last_exc."""

class Pdb(Bdb, Cmd):
    # Everything here is undocumented, except for __init__

    commands_resuming: ClassVar[list[str]]

    if sys.version_info >= (3, 13):
        MAX_CHAINED_EXCEPTION_DEPTH: Final = 999

    aliases: dict[str, str]
    mainpyfile: str
    _wait_for_mainpyfile: bool
    rcLines: list[str]
    commands: dict[int, list[str]]
    commands_doprompt: dict[int, bool]
    commands_silent: dict[int, bool]
    commands_defining: bool
    commands_bnum: int | None
    lineno: int | None
    stack: list[tuple[FrameType, int]]
    curindex: int
    curframe: FrameType | None
    curframe_locals: Mapping[str, Any]
    if sys.version_info >= (3, 14):
        mode: _Mode | None
        colorize: bool
        def __init__(
            self,
            completekey: str = "tab",
            stdin: IO[str] | None = None,
            stdout: IO[str] | None = None,
            skip: Iterable[str] | None = None,
            nosigint: bool = False,
            readrc: bool = True,
            mode: _Mode | None = None,
            backend: _Backend | None = None,
            colorize: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self,
            completekey: str = "tab",
            stdin: IO[str] | None = None,
            stdout: IO[str] | None = None,
            skip: Iterable[str] | None = None,
            nosigint: bool = False,
            readrc: bool = True,
        ) -> None: ...
    if sys.version_info >= (3, 14):
        def set_trace(self, frame: FrameType | None = None, *, commands: Iterable[str] | None = None) -> None: ...
        async def set_trace_async(self, frame: FrameType | None = None, *, commands: Iterable[str] | None = None) -> None: ...

    def forget(self) -> None: ...
    def setup(self, f: FrameType | None, tb: TracebackType | None) -> None: ...
    if sys.version_info < (3, 11):
        def execRcLines(self) -> None: ...

    if sys.version_info >= (3, 13):
        user_opcode = Bdb.user_line

    def bp_commands(self, frame: FrameType) -> bool:
        """Call every command that was set for the current active breakpoint
        (if there is one).

        Returns True if the normal interaction function must be called,
        False otherwise.
        """
    if sys.version_info >= (3, 13):
        def interaction(self, frame: FrameType | None, tb_or_exc: TracebackType | BaseException | None) -> None: ...
    else:
        def interaction(self, frame: FrameType | None, traceback: TracebackType | None) -> None: ...

    def displayhook(self, obj: object) -> None:
        """Custom displayhook for the exec in default(), which prevents
        assignment of the _ variable in the builtins.
        """

    def handle_command_def(self, line: str) -> bool:
        """Handles one command line during command list definition."""

    def defaultFile(self) -> str:
        """Produce a reasonable default."""

    def lineinfo(self, identifier: str) -> tuple[None, None, None] | tuple[str, str, int]: ...
    if sys.version_info >= (3, 14):
        def checkline(self, filename: str, lineno: int, module_globals: _ModuleGlobals | None = None) -> int:
            """Check whether specified line seems to be executable.

            Return `lineno` if it is, 0 if not (e.g. a docstring, comment, blank
            line or EOF). Warning: testing is not comprehensive.
            """
    else:
        def checkline(self, filename: str, lineno: int) -> int:
            """Check whether specified line seems to be executable.

            Return `lineno` if it is, 0 if not (e.g. a docstring, comment, blank
            line or EOF). Warning: testing is not comprehensive.
            """

    def _getval(self, arg: str) -> object: ...
    if sys.version_info >= (3, 14):
        def print_stack_trace(self, count: int | None = None) -> None: ...
    else:
        def print_stack_trace(self) -> None: ...

    def print_stack_entry(self, frame_lineno: tuple[FrameType, int], prompt_prefix: str = "\n-> ") -> None: ...
    def lookupmodule(self, filename: str) -> str | None:
        """Helper function for break/clear parsing -- may be overridden.

        lookupmodule() translates (possibly incomplete) file or module name
        into an absolute file name.

        filename could be in format of:
            * an absolute path like '/path/to/file.py'
            * a relative path like 'file.py' or 'dir/file.py'
            * a module name like 'module' or 'package.module'

        files and modules will be searched in sys.path.
        """
    if sys.version_info < (3, 11):
        def _runscript(self, filename: str) -> None: ...

    if sys.version_info >= (3, 14):
        def complete_multiline_names(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...

    if sys.version_info >= (3, 13):
        def completedefault(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...

    def do_commands(self, arg: str) -> bool | None:
        """(Pdb) commands [bpnumber]
        (com) ...
        (com) end
        (Pdb)

        Specify a list of commands for breakpoint number bpnumber.
        The commands themselves are entered on the following lines.
        Type a line containing just 'end' to terminate the commands.
        The commands are executed when the breakpoint is hit.

        To remove all commands from a breakpoint, type commands and
        follow it immediately with end; that is, give no commands.

        With no bpnumber argument, commands refers to the last
        breakpoint set.

        You can use breakpoint commands to start your program up
        again.  Simply use the continue command, or step, or any other
        command that resumes execution.

        Specifying any command resuming execution (currently continue,
        step, next, return, jump, quit and their abbreviations)
        terminates the command list (as if that command was
        immediately followed by end).  This is because any time you
        resume execution (even with a simple next or step), you may
        encounter another breakpoint -- which could have its own
        command list, leading to ambiguities about which list to
        execute.

        If you use the 'silent' command in the command list, the usual
        message about stopping at a breakpoint is not printed.  This
        may be desirable for breakpoints that are to print a specific
        message and then continue.  If none of the other commands
        print anything, you will see no sign that the breakpoint was
        reached.
        """
    if sys.version_info >= (3, 14):
        def do_break(self, arg: str, temporary: bool = False) -> bool | None:
            """b(reak) [ ([filename:]lineno | function) [, condition] ]

            Without argument, list all breaks.

            With a line number argument, set a break at this line in the
            current file.  With a function name, set a break at the first
            executable line of that function.  If a second argument is
            present, it is a string specifying an expression which must
            evaluate to true before the breakpoint is honored.

            The line number may be prefixed with a filename and a colon,
            to specify a breakpoint in another file (probably one that
            hasn't been loaded yet).  The file is searched for on
            sys.path; the .py suffix may be omitted.
            """
    else:
        def do_break(self, arg: str, temporary: bool | Literal[0, 1] = 0) -> bool | None:
            """b(reak) [ ([filename:]lineno | function) [, condition] ]

            Without argument, list all breaks.

            With a line number argument, set a break at this line in the
            current file.  With a function name, set a break at the first
            executable line of that function.  If a second argument is
            present, it is a string specifying an expression which must
            evaluate to true before the breakpoint is honored.

            The line number may be prefixed with a filename and a colon,
            to specify a breakpoint in another file (probably one that
            hasn't been loaded yet).  The file is searched for on
            sys.path; the .py suffix may be omitted.
            """

    def do_tbreak(self, arg: str) -> bool | None:
        """tbreak [ ([filename:]lineno | function) [, condition] ]

        Same arguments as break, but sets a temporary breakpoint: it
        is automatically deleted when first hit.
        """

    def do_enable(self, arg: str) -> bool | None:
        """enable bpnumber [bpnumber ...]

        Enables the breakpoints given as a space separated list of
        breakpoint numbers.
        """

    def do_disable(self, arg: str) -> bool | None:
        """disable bpnumber [bpnumber ...]

        Disables the breakpoints given as a space separated list of
        breakpoint numbers.  Disabling a breakpoint means it cannot
        cause the program to stop execution, but unlike clearing a
        breakpoint, it remains in the list of breakpoints and can be
        (re-)enabled.
        """

    def do_condition(self, arg: str) -> bool | None:
        """condition bpnumber [condition]

        Set a new condition for the breakpoint, an expression which
        must evaluate to true before the breakpoint is honored.  If
        condition is absent, any existing condition is removed; i.e.,
        the breakpoint is made unconditional.
        """

    def do_ignore(self, arg: str) -> bool | None:
        """ignore bpnumber [count]

        Set the ignore count for the given breakpoint number.  If
        count is omitted, the ignore count is set to 0.  A breakpoint
        becomes active when the ignore count is zero.  When non-zero,
        the count is decremented each time the breakpoint is reached
        and the breakpoint is not disabled and any associated
        condition evaluates to true.
        """

    def do_clear(self, arg: str) -> bool | None:
        """cl(ear) [filename:lineno | bpnumber ...]

        With a space separated list of breakpoint numbers, clear
        those breakpoints.  Without argument, clear all breaks (but
        first ask confirmation).  With a filename:lineno argument,
        clear all breaks at that line in that file.
        """

    def do_where(self, arg: str) -> bool | None:
        """w(here) [count]

        Print a stack trace. If count is not specified, print the full stack.
        If count is 0, print the current frame entry. If count is positive,
        print count entries from the most recent frame. If count is negative,
        print -count entries from the least recent frame.
        An arrow indicates the "current frame", which determines the
        context of most commands.  'bt' is an alias for this command.
        """
    if sys.version_info >= (3, 13):
        def do_exceptions(self, arg: str) -> bool | None:
            """exceptions [number]

            List or change current exception in an exception chain.

            Without arguments, list all the current exception in the exception
            chain. Exceptions will be numbered, with the current exception indicated
            with an arrow.

            If given an integer as argument, switch to the exception at that index.
            """

    def do_up(self, arg: str) -> bool | None:
        """u(p) [count]

        Move the current frame count (default one) levels up in the
        stack trace (to an older frame).
        """

    def do_down(self, arg: str) -> bool | None:
        """d(own) [count]

        Move the current frame count (default one) levels down in the
        stack trace (to a newer frame).
        """

    def do_until(self, arg: str) -> bool | None:
        """unt(il) [lineno]

        Without argument, continue execution until the line with a
        number greater than the current one is reached.  With a line
        number, continue execution until a line with a number greater
        or equal to that is reached.  In both cases, also stop when
        the current frame returns.
        """

    def do_step(self, arg: str) -> bool | None:
        """s(tep)

        Execute the current line, stop at the first possible occasion
        (either in a function that is called or in the current
        function).
        """

    def do_next(self, arg: str) -> bool | None:
        """n(ext)

        Continue execution until the next line in the current function
        is reached or it returns.
        """

    def do_run(self, arg: str) -> bool | None:
        """run [args...]

        Restart the debugged python program. If a string is supplied
        it is split with "shlex", and the result is used as the new
        sys.argv.  History, breakpoints, actions and debugger options
        are preserved.  "restart" is an alias for "run".
        """

    def do_return(self, arg: str) -> bool | None:
        """r(eturn)

        Continue execution until the current function returns.
        """

    def do_continue(self, arg: str) -> bool | None:
        """c(ont(inue))

        Continue execution, only stop when a breakpoint is encountered.
        """

    def do_jump(self, arg: str) -> bool | None:
        """j(ump) lineno

        Set the next line that will be executed.  Only available in
        the bottom-most frame.  This lets you jump back and execute
        code again, or jump forward to skip code that you don't want
        to run.

        It should be noted that not all jumps are allowed -- for
        instance it is not possible to jump into the middle of a
        for loop or out of a finally clause.
        """

    def do_debug(self, arg: str) -> bool | None:
        """debug code

        Enter a recursive debugger that steps through the code
        argument (which is an arbitrary expression or statement to be
        executed in the current environment).
        """

    def do_quit(self, arg: str) -> bool | None:
        """q(uit) | exit

        Quit from the debugger. The program being executed is aborted.
        """

    def do_EOF(self, arg: str) -> bool | None:
        """EOF

        Handles the receipt of EOF as a command.
        """

    def do_args(self, arg: str) -> bool | None:
        """a(rgs)

        Print the argument list of the current function.
        """

    def do_retval(self, arg: str) -> bool | None:
        """retval

        Print the return value for the last return of a function.
        """

    def do_p(self, arg: str) -> bool | None:
        """p expression

        Print the value of the expression.
        """

    def do_pp(self, arg: str) -> bool | None:
        """pp expression

        Pretty-print the value of the expression.
        """

    def do_list(self, arg: str) -> bool | None:
        """l(ist) [first[, last] | .]

        List source code for the current file.  Without arguments,
        list 11 lines around the current line or continue the previous
        listing.  With . as argument, list 11 lines around the current
        line.  With one argument, list 11 lines starting at that line.
        With two arguments, list the given range; if the second
        argument is less than the first, it is a count.

        The current line in the current frame is indicated by "->".
        If an exception is being debugged, the line where the
        exception was originally raised or propagated is indicated by
        ">>", if it differs from the current line.
        """

    def do_whatis(self, arg: str) -> bool | None:
        """whatis expression

        Print the type of the argument.
        """

    def do_alias(self, arg: str) -> bool | None:
        """alias [name [command]]

        Create an alias called 'name' that executes 'command'.  The
        command must *not* be enclosed in quotes.  Replaceable
        parameters can be indicated by %1, %2, and so on, while %* is
        replaced by all the parameters.  If no command is given, the
        current alias for name is shown. If no name is given, all
        aliases are listed.

        Aliases may be nested and can contain anything that can be
        legally typed at the pdb prompt.  Note!  You *can* override
        internal pdb commands with aliases!  Those internal commands
        are then hidden until the alias is removed.  Aliasing is
        recursively applied to the first word of the command line; all
        other words in the line are left alone.

        As an example, here are two useful aliases (especially when
        placed in the .pdbrc file):

        # Print instance variables (usage "pi classInst")
        alias pi for k in %1.__dict__.keys(): print("%1.",k,"=",%1.__dict__[k])
        # Print instance variables in self
        alias ps pi self
        """

    def do_unalias(self, arg: str) -> bool | None:
        """unalias name

        Delete the specified alias.
        """

    def do_help(self, arg: str) -> bool | None:
        """h(elp)

        Without argument, print the list of available commands.
        With a command name as argument, print help about that command.
        "help pdb" shows the full pdb documentation.
        "help exec" gives help on the ! command.
        """
    do_b = do_break
    do_cl = do_clear
    do_w = do_where
    do_bt = do_where
    do_u = do_up
    do_d = do_down
    do_unt = do_until
    do_s = do_step
    do_n = do_next
    do_restart = do_run
    do_r = do_return
    do_c = do_continue
    do_cont = do_continue
    do_j = do_jump
    do_q = do_quit
    do_exit = do_quit
    do_a = do_args
    do_rv = do_retval
    do_l = do_list
    do_h = do_help
    def help_exec(self) -> None:
        """(!) statement

        Execute the (one-line) statement in the context of the current
        stack frame.  The exclamation point can be omitted unless the
        first word of the statement resembles a debugger command, e.g.:
        (Pdb) ! n=42
        (Pdb)

        To assign to a global variable you must always prefix the command with
        a 'global' command, e.g.:
        (Pdb) global list_options; list_options = ['-l']
        (Pdb)
        """

    def help_pdb(self) -> None: ...
    def sigint_handler(self, signum: signal.Signals, frame: FrameType) -> None: ...
    if sys.version_info >= (3, 13):
        def message(self, msg: str, end: str = "\n") -> None: ...
    else:
        def message(self, msg: str) -> None: ...

    def error(self, msg: str) -> None: ...
    if sys.version_info >= (3, 13):
        def completenames(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...  # type: ignore[override]
    if sys.version_info >= (3, 12):
        def set_convenience_variable(self, frame: FrameType, name: str, value: Any) -> None: ...
    if sys.version_info >= (3, 13):
        # Added in 3.13.8 and 3.14.1
        @property
        def rlcompleter(self) -> type[Completer]:
            """Return the `Completer` class from `rlcompleter`, while avoiding the
            side effects of changing the completer from `import rlcompleter`.

            This is a compromise between GH-138860 and GH-139289. If GH-139289 is
            fixed, then we don't need this and we can just `import rlcompleter` in
            `Pdb.__init__`.
            """

    def _select_frame(self, number: int) -> None: ...
    def _getval_except(self, arg: str, frame: FrameType | None = None) -> object: ...
    def _print_lines(self, lines: Sequence[str], start: int, breaks: Sequence[int] = (), frame: FrameType | None = None) -> None:
        """Print a range of lines."""

    def _cmdloop(self) -> None: ...
    def do_display(self, arg: str) -> bool | None:
        """display [expression]

        Display the value of the expression if it changed, each time execution
        stops in the current frame.

        Without expression, list all display expressions for the current frame.
        """

    def do_interact(self, arg: str) -> bool | None:
        """interact

        Start an interactive interpreter whose global namespace
        contains all the (global and local) names found in the current scope.
        """

    def do_longlist(self, arg: str) -> bool | None:
        """ll | longlist

        List the whole source code for the current function or frame.
        """

    def do_source(self, arg: str) -> bool | None:
        """source expression

        Try to get source code for the given object and display it.
        """

    def do_undisplay(self, arg: str) -> bool | None:
        """undisplay [expression]

        Do not display the expression any more in the current frame.

        Without expression, clear all display expressions for the current frame.
        """
    do_ll = do_longlist
    def _complete_location(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...
    def _complete_bpnumber(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...
    def _complete_expression(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...
    def complete_undisplay(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...
    def complete_unalias(self, text: str, line: str, begidx: int, endidx: int) -> list[str]: ...
    complete_commands = _complete_bpnumber
    complete_break = _complete_location
    complete_b = _complete_location
    complete_tbreak = _complete_location
    complete_enable = _complete_bpnumber
    complete_disable = _complete_bpnumber
    complete_condition = _complete_bpnumber
    complete_ignore = _complete_bpnumber
    complete_clear = _complete_location
    complete_cl = _complete_location
    complete_debug = _complete_expression
    complete_print = _complete_expression
    complete_p = _complete_expression
    complete_pp = _complete_expression
    complete_source = _complete_expression
    complete_whatis = _complete_expression
    complete_display = _complete_expression

    if sys.version_info < (3, 11):
        def _runmodule(self, module_name: str) -> None: ...

# undocumented

def find_function(funcname: str, filename: str) -> tuple[str, str, int] | None: ...
def main() -> None: ...
def help() -> None: ...

if sys.version_info < (3, 10):
    def getsourcelines(obj: _SourceObjectType) -> tuple[list[str], int]: ...

def lasti2lineno(code: CodeType, lasti: int) -> int: ...

class _rstr(str):
    """String that doesn't quote its repr."""

    def __repr__(self) -> Self: ...
