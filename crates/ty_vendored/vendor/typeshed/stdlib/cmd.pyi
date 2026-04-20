"""A generic class to build line-oriented command interpreters.

Interpreters constructed with this class obey the following conventions:

1. End of file on input is processed as the command 'EOF'.
2. A command is parsed out of each line by collecting the prefix composed
   of characters in the identchars member.
3. A command 'foo' is dispatched to a method 'do_foo()'; the do_ method
   is passed a single argument consisting of the remainder of the line.
4. Typing an empty line repeats the last command.  (Actually, it calls the
   method 'emptyline', which may be overridden in a subclass.)
5. There is a predefined 'help' method.  Given an argument 'topic', it
   calls the command 'help_topic'.  With no arguments, it lists all topics
   with defined help_ functions, broken into up to three topics; documented
   commands, miscellaneous help topics, and undocumented commands.
6. The command '?' is a synonym for 'help'.  The command '!' is a synonym
   for 'shell', if a do_shell method exists.
7. If completion is enabled, completing commands will be done automatically,
   and completing of commands args is done by calling complete_foo() with
   arguments text, line, begidx, endidx.  text is string we are matching
   against, all returned matches must begin with it.  line is the current
   input line (lstripped), begidx and endidx are the beginning and end
   indexes of the text being matched, which could be used to provide
   different completion depending upon which position the argument is in.

The 'default' method may be overridden to intercept commands for which there
is no do_ method.

The 'completedefault' method may be overridden to intercept completions for
commands that have no complete_ method.

The data member 'self.ruler' sets the character used to draw separator lines
in the help messages.  If empty, no ruler line is drawn.  It defaults to "=".

If the value of 'self.intro' is nonempty when the cmdloop method is called,
it is printed out on interpreter startup.  This value may be overridden
via an optional argument to the cmdloop() method.

The data members 'self.doc_header', 'self.misc_header', and
'self.undoc_header' set the headers used for the help function's
listings of documented functions, miscellaneous topics, and undocumented
functions respectively.
"""

from collections.abc import Callable
from typing import IO, Any, Final
from typing_extensions import LiteralString

__all__ = ["Cmd"]

PROMPT: Final = "(Cmd) "
IDENTCHARS: Final[LiteralString]  # Too big to be `Literal`

class Cmd:
    """A simple framework for writing line-oriented command interpreters.

    These are often useful for test harnesses, administrative tools, and
    prototypes that will later be wrapped in a more sophisticated interface.

    A Cmd instance or subclass instance is a line-oriented interpreter
    framework.  There is no good reason to instantiate Cmd itself; rather,
    it's useful as a superclass of an interpreter class you define yourself
    in order to inherit Cmd's methods and encapsulate action methods.

    """

    prompt: str
    identchars: str
    ruler: str
    lastcmd: str
    intro: Any | None
    doc_leader: str
    doc_header: str
    misc_header: str
    undoc_header: str
    nohelp: str
    use_rawinput: bool
    stdin: IO[str]
    stdout: IO[str]
    cmdqueue: list[str]
    completekey: str
    def __init__(self, completekey: str = "tab", stdin: IO[str] | None = None, stdout: IO[str] | None = None) -> None:
        """Instantiate a line-oriented interpreter framework.

        The optional argument 'completekey' is the readline name of a
        completion key; it defaults to the Tab key. If completekey is
        not None and the readline module is available, command completion
        is done automatically. The optional arguments stdin and stdout
        specify alternate input and output file objects; if not specified,
        sys.stdin and sys.stdout are used.

        """
    old_completer: Callable[[str, int], str | None] | None
    def cmdloop(self, intro: Any | None = None) -> None:
        """Repeatedly issue a prompt, accept input, parse an initial prefix
        off the received input, and dispatch to action methods, passing them
        the remainder of the line as argument.

        """

    def precmd(self, line: str) -> str:
        """Hook method executed just before the command line is
        interpreted, but after the input prompt is generated and issued.

        """

    def postcmd(self, stop: bool, line: str) -> bool:
        """Hook method executed just after a command dispatch is finished."""

    def preloop(self) -> None:
        """Hook method executed once when the cmdloop() method is called."""

    def postloop(self) -> None:
        """Hook method executed once when the cmdloop() method is about to
        return.

        """

    def parseline(self, line: str) -> tuple[str | None, str | None, str]:
        """Parse the line into a command name and a string containing
        the arguments.  Returns a tuple containing (command, args, line).
        'command' and 'args' may be None if the line couldn't be parsed.
        """

    def onecmd(self, line: str) -> bool:
        """Interpret the argument as though it had been typed in response
        to the prompt.

        This may be overridden, but should not normally need to be;
        see the precmd() and postcmd() methods for useful execution hooks.
        The return value is a flag indicating whether interpretation of
        commands by the interpreter should stop.

        """

    def emptyline(self) -> bool:
        """Called when an empty line is entered in response to the prompt.

        If this method is not overridden, it repeats the last nonempty
        command entered.

        """

    def default(self, line: str) -> None:
        """Called on an input line when the command prefix is not recognized.

        If this method is not overridden, it prints an error message and
        returns.

        """

    def completedefault(self, *ignored: Any) -> list[str]:
        """Method called to complete an input line when no command-specific
        complete_*() method is available.

        By default, it returns an empty list.

        """

    def completenames(self, text: str, *ignored: Any) -> list[str]: ...
    completion_matches: list[str] | None
    def complete(self, text: str, state: int) -> list[str] | None:
        """Return the next possible completion for 'text'.

        If a command has not been entered, then complete against command list.
        Otherwise try to call complete_<command> to get list of completions.
        """

    def get_names(self) -> list[str]: ...
    # Only the first element of args matters.
    def complete_help(self, *args: Any) -> list[str]: ...
    def do_help(self, arg: str) -> bool | None:
        """List available commands with "help" or detailed help with "help cmd"."""

    def print_topics(self, header: str, cmds: list[str] | None, cmdlen: Any, maxcol: int) -> None: ...
    def columnize(self, list: list[str] | None, displaywidth: int = 80) -> None:
        """Display a list of strings as a compact set of columns.

        Each column is only as wide as necessary.
        Columns are separated by two spaces (one was not legible enough).
        """
