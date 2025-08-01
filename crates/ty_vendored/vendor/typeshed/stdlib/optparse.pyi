"""A powerful, extensible, and easy-to-use option parser.

By Greg Ward <gward@python.net>

Originally distributed as Optik.

For support, use the optik-users@lists.sourceforge.net mailing list
(http://lists.sourceforge.net/lists/listinfo/optik-users).

Simple usage example:

   from optparse import OptionParser

   parser = OptionParser()
   parser.add_option("-f", "--file", dest="filename",
                     help="write report to FILE", metavar="FILE")
   parser.add_option("-q", "--quiet",
                     action="store_false", dest="verbose", default=True,
                     help="don't print status messages to stdout")

   (options, args) = parser.parse_args()
"""

import builtins
from _typeshed import MaybeNone, SupportsWrite
from abc import abstractmethod
from collections.abc import Callable, Iterable, Mapping, Sequence
from typing import Any, ClassVar, Final, Literal, NoReturn, overload
from typing_extensions import Self

__all__ = [
    "Option",
    "make_option",
    "SUPPRESS_HELP",
    "SUPPRESS_USAGE",
    "Values",
    "OptionContainer",
    "OptionGroup",
    "OptionParser",
    "HelpFormatter",
    "IndentedHelpFormatter",
    "TitledHelpFormatter",
    "OptParseError",
    "OptionError",
    "OptionConflictError",
    "OptionValueError",
    "BadOptionError",
    "check_choice",
]
NO_DEFAULT: Final = ("NO", "DEFAULT")
SUPPRESS_HELP: Final = "SUPPRESSHELP"
SUPPRESS_USAGE: Final = "SUPPRESSUSAGE"

# Can return complex, float, or int depending on the option's type
def check_builtin(option: Option, opt: str, value: str) -> complex: ...
def check_choice(option: Option, opt: str, value: str) -> str: ...

class OptParseError(Exception):
    msg: str
    def __init__(self, msg: str) -> None: ...

class BadOptionError(OptParseError):
    """
    Raised if an invalid option is seen on the command line.
    """

    opt_str: str
    def __init__(self, opt_str: str) -> None: ...

class AmbiguousOptionError(BadOptionError):
    """
    Raised if an ambiguous option is seen on the command line.
    """

    possibilities: Iterable[str]
    def __init__(self, opt_str: str, possibilities: Sequence[str]) -> None: ...

class OptionError(OptParseError):
    """
    Raised if an Option instance is created with invalid or
    inconsistent arguments.
    """

    option_id: str
    def __init__(self, msg: str, option: Option) -> None: ...

class OptionConflictError(OptionError):
    """
    Raised if conflicting options are added to an OptionParser.
    """

class OptionValueError(OptParseError):
    """
    Raised if an invalid option value is encountered on the command
    line.
    """

class HelpFormatter:
    """
    Abstract base class for formatting option help.  OptionParser
    instances should use one of the HelpFormatter subclasses for
    formatting help; by default IndentedHelpFormatter is used.

    Instance attributes:
      parser : OptionParser
        the controlling OptionParser instance
      indent_increment : int
        the number of columns to indent per nesting level
      max_help_position : int
        the maximum starting column for option help text
      help_position : int
        the calculated starting column for option help text;
        initially the same as the maximum
      width : int
        total number of columns for output (pass None to constructor for
        this value to be taken from the $COLUMNS environment variable)
      level : int
        current indentation level
      current_indent : int
        current indentation level (in columns)
      help_width : int
        number of columns available for option help text (calculated)
      default_tag : str
        text to replace with each option's default value, "%default"
        by default.  Set to false value to disable default value expansion.
      option_strings : { Option : str }
        maps Option instances to the snippet of help text explaining
        the syntax of that option, e.g. "-h, --help" or
        "-fFILE, --file=FILE"
      _short_opt_fmt : str
        format string controlling how short options with values are
        printed in help text.  Must be either "%s%s" ("-fFILE") or
        "%s %s" ("-f FILE"), because those are the two syntaxes that
        Optik supports.
      _long_opt_fmt : str
        similar but for long options; must be either "%s %s" ("--file FILE")
        or "%s=%s" ("--file=FILE").
    """

    NO_DEFAULT_VALUE: str
    _long_opt_fmt: str
    _short_opt_fmt: str
    current_indent: int
    default_tag: str
    help_position: int
    help_width: int | MaybeNone  # initialized as None and computed later as int when storing option strings
    indent_increment: int
    level: int
    max_help_position: int
    option_strings: dict[Option, str]
    parser: OptionParser
    short_first: bool | Literal[0, 1]
    width: int
    def __init__(
        self, indent_increment: int, max_help_position: int, width: int | None, short_first: bool | Literal[0, 1]
    ) -> None: ...
    def dedent(self) -> None: ...
    def expand_default(self, option: Option) -> str: ...
    def format_description(self, description: str | None) -> str: ...
    def format_epilog(self, epilog: str | None) -> str: ...
    @abstractmethod
    def format_heading(self, heading: str) -> str: ...
    def format_option(self, option: Option) -> str: ...
    def format_option_strings(self, option: Option) -> str:
        """Return a comma-separated list of option strings & metavariables."""

    @abstractmethod
    def format_usage(self, usage: str) -> str: ...
    def indent(self) -> None: ...
    def set_long_opt_delimiter(self, delim: str) -> None: ...
    def set_parser(self, parser: OptionParser) -> None: ...
    def set_short_opt_delimiter(self, delim: str) -> None: ...
    def store_option_strings(self, parser: OptionParser) -> None: ...

class IndentedHelpFormatter(HelpFormatter):
    """Format help with indented section bodies."""

    def __init__(
        self,
        indent_increment: int = 2,
        max_help_position: int = 24,
        width: int | None = None,
        short_first: bool | Literal[0, 1] = 1,
    ) -> None: ...
    def format_heading(self, heading: str) -> str: ...
    def format_usage(self, usage: str) -> str: ...

class TitledHelpFormatter(HelpFormatter):
    """Format help with underlined section headers."""

    def __init__(
        self,
        indent_increment: int = 0,
        max_help_position: int = 24,
        width: int | None = None,
        short_first: bool | Literal[0, 1] = 0,
    ) -> None: ...
    def format_heading(self, heading: str) -> str: ...
    def format_usage(self, usage: str) -> str: ...

class Option:
    """
    Instance attributes:
      _short_opts : [string]
      _long_opts : [string]

      action : string
      type : string
      dest : string
      default : any
      nargs : int
      const : any
      choices : [string]
      callback : function
      callback_args : (any*)
      callback_kwargs : { string : any }
      help : string
      metavar : string
    """

    ACTIONS: tuple[str, ...]
    ALWAYS_TYPED_ACTIONS: tuple[str, ...]
    ATTRS: list[str]
    CHECK_METHODS: list[Callable[[Self], object]] | None
    CONST_ACTIONS: tuple[str, ...]
    STORE_ACTIONS: tuple[str, ...]
    TYPED_ACTIONS: tuple[str, ...]
    TYPES: tuple[str, ...]
    TYPE_CHECKER: dict[str, Callable[[Option, str, str], object]]
    _long_opts: list[str]
    _short_opts: list[str]
    action: str
    type: str | None
    dest: str | None
    default: Any  # default can be "any" type
    nargs: int
    const: Any | None  # const can be "any" type
    choices: list[str] | tuple[str, ...] | None
    # Callback args and kwargs cannot be expressed in Python's type system.
    # Revisit if ParamSpec is ever changed to work with packed args/kwargs.
    callback: Callable[..., object] | None
    callback_args: tuple[Any, ...] | None
    callback_kwargs: dict[str, Any] | None
    help: str | None
    metavar: str | None
    def __init__(
        self,
        *opts: str | None,
        # The following keywords are handled by the _set_attrs method. All default to
        # `None` except for `default`, which defaults to `NO_DEFAULT`.
        action: str | None = None,
        type: str | builtins.type | None = None,
        dest: str | None = None,
        default: Any = ...,  # = NO_DEFAULT
        nargs: int | None = None,
        const: Any | None = None,
        choices: list[str] | tuple[str, ...] | None = None,
        callback: Callable[..., object] | None = None,
        callback_args: tuple[Any, ...] | None = None,
        callback_kwargs: dict[str, Any] | None = None,
        help: str | None = None,
        metavar: str | None = None,
    ) -> None: ...
    def _check_action(self) -> None: ...
    def _check_callback(self) -> None: ...
    def _check_choice(self) -> None: ...
    def _check_const(self) -> None: ...
    def _check_dest(self) -> None: ...
    def _check_nargs(self) -> None: ...
    def _check_opt_strings(self, opts: Iterable[str | None]) -> list[str]: ...
    def _check_type(self) -> None: ...
    def _set_attrs(self, attrs: dict[str, Any]) -> None: ...  # accepted attrs depend on the ATTRS attribute
    def _set_opt_strings(self, opts: Iterable[str]) -> None: ...
    def check_value(self, opt: str, value: str) -> Any: ...  # return type cannot be known statically
    def convert_value(self, opt: str, value: str | tuple[str, ...] | None) -> Any: ...  # return type cannot be known statically
    def get_opt_string(self) -> str: ...
    def process(self, opt: str, value: str | tuple[str, ...] | None, values: Values, parser: OptionParser) -> int: ...
    # value of take_action can be "any" type
    def take_action(self, action: str, dest: str, opt: str, value: Any, values: Values, parser: OptionParser) -> int: ...
    def takes_value(self) -> bool: ...

make_option = Option

class OptionContainer:
    """
    Abstract base class.

    Class attributes:
      standard_option_list : [Option]
        list of standard options that will be accepted by all instances
        of this parser class (intended to be overridden by subclasses).

    Instance attributes:
      option_list : [Option]
        the list of Option objects contained by this OptionContainer
      _short_opt : { string : Option }
        dictionary mapping short option strings, eg. "-f" or "-X",
        to the Option instances that implement them.  If an Option
        has multiple short option strings, it will appear in this
        dictionary multiple times. [1]
      _long_opt : { string : Option }
        dictionary mapping long option strings, eg. "--file" or
        "--exclude", to the Option instances that implement them.
        Again, a given Option can occur multiple times in this
        dictionary. [1]
      defaults : { string : any }
        dictionary mapping option destination names to default
        values for each destination [1]

    [1] These mappings are common to (shared by) all components of the
        controlling OptionParser, where they are initially created.

    """

    _long_opt: dict[str, Option]
    _short_opt: dict[str, Option]
    conflict_handler: str
    defaults: dict[str, Any]  # default values can be "any" type
    description: str | None
    option_class: type[Option]
    def __init__(
        self, option_class: type[Option], conflict_handler: Literal["error", "resolve"], description: str | None
    ) -> None: ...
    def _check_conflict(self, option: Option) -> None: ...
    def _create_option_mappings(self) -> None: ...
    def _share_option_mappings(self, parser: OptionParser) -> None: ...
    @overload
    def add_option(self, opt: Option, /) -> Option:
        """add_option(Option)
        add_option(opt_str, ..., kwarg=val, ...)
        """

    @overload
    def add_option(
        self,
        opt_str: str,
        /,
        *opts: str | None,
        action: str | None = None,
        type: str | builtins.type | None = None,
        dest: str | None = None,
        default: Any = ...,  # = NO_DEFAULT
        nargs: int | None = None,
        const: Any | None = None,
        choices: list[str] | tuple[str, ...] | None = None,
        callback: Callable[..., object] | None = None,
        callback_args: tuple[Any, ...] | None = None,
        callback_kwargs: dict[str, Any] | None = None,
        help: str | None = None,
        metavar: str | None = None,
        **kwargs,  # Allow arbitrary keyword arguments for user defined option_class
    ) -> Option: ...
    def add_options(self, option_list: Iterable[Option]) -> None: ...
    def destroy(self) -> None:
        """see OptionParser.destroy()."""

    def format_option_help(self, formatter: HelpFormatter) -> str: ...
    def format_description(self, formatter: HelpFormatter) -> str: ...
    def format_help(self, formatter: HelpFormatter) -> str: ...
    def get_description(self) -> str | None: ...
    def get_option(self, opt_str: str) -> Option | None: ...
    def has_option(self, opt_str: str) -> bool: ...
    def remove_option(self, opt_str: str) -> None: ...
    def set_conflict_handler(self, handler: Literal["error", "resolve"]) -> None: ...
    def set_description(self, description: str | None) -> None: ...

class OptionGroup(OptionContainer):
    option_list: list[Option]
    parser: OptionParser
    title: str
    def __init__(self, parser: OptionParser, title: str, description: str | None = None) -> None: ...
    def _create_option_list(self) -> None: ...
    def set_title(self, title: str) -> None: ...

class Values:
    def __init__(self, defaults: Mapping[str, object] | None = None) -> None: ...
    def _update(self, dict: Mapping[str, object], mode: Literal["careful", "loose"]) -> None: ...
    def _update_careful(self, dict: Mapping[str, object]) -> None:
        """
        Update the option values from an arbitrary dictionary, but only
        use keys from dict that already have a corresponding attribute
        in self.  Any keys in dict without a corresponding attribute
        are silently ignored.
        """

    def _update_loose(self, dict: Mapping[str, object]) -> None:
        """
        Update the option values from an arbitrary dictionary,
        using all keys from the dictionary regardless of whether
        they have a corresponding attribute in self or not.
        """

    def ensure_value(self, attr: str, value: object) -> Any: ...  # return type cannot be known statically
    def read_file(self, filename: str, mode: Literal["careful", "loose"] = "careful") -> None: ...
    def read_module(self, modname: str, mode: Literal["careful", "loose"] = "careful") -> None: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    # __getattr__ doesn't exist, but anything passed as a default to __init__
    # is set on the instance.
    def __getattr__(self, name: str) -> Any: ...
    # TODO: mypy infers -> object for __getattr__ if __setattr__ has `value: object`
    def __setattr__(self, name: str, value: Any, /) -> None: ...
    def __eq__(self, other: object) -> bool: ...

class OptionParser(OptionContainer):
    """
    Class attributes:
      standard_option_list : [Option]
        list of standard options that will be accepted by all instances
        of this parser class (intended to be overridden by subclasses).

    Instance attributes:
      usage : string
        a usage string for your program.  Before it is displayed
        to the user, "%prog" will be expanded to the name of
        your program (self.prog or os.path.basename(sys.argv[0])).
      prog : string
        the name of the current program (to override
        os.path.basename(sys.argv[0])).
      description : string
        A paragraph of text giving a brief overview of your program.
        optparse reformats this paragraph to fit the current terminal
        width and prints it when the user requests help (after usage,
        but before the list of options).
      epilog : string
        paragraph of help text to print after option help

      option_groups : [OptionGroup]
        list of option groups in this parser (option groups are
        irrelevant for parsing the command-line, but very useful
        for generating help)

      allow_interspersed_args : bool = true
        if true, positional arguments may be interspersed with options.
        Assuming -a and -b each take a single argument, the command-line
          -ablah foo bar -bboo baz
        will be interpreted the same as
          -ablah -bboo -- foo bar baz
        If this flag were false, that command line would be interpreted as
          -ablah -- foo bar -bboo baz
        -- ie. we stop processing options as soon as we see the first
        non-option argument.  (This is the tradition followed by
        Python's getopt module, Perl's Getopt::Std, and other argument-
        parsing libraries, but it is generally annoying to users.)

      process_default_values : bool = true
        if true, option default values are processed similarly to option
        values from the command line: that is, they are passed to the
        type-checking function for the option's type (as long as the
        default value is a string).  (This really only matters if you
        have defined custom types; see SF bug #955889.)  Set it to false
        to restore the behaviour of Optik 1.4.1 and earlier.

      rargs : [string]
        the argument list currently being parsed.  Only set when
        parse_args() is active, and continually trimmed down as
        we consume arguments.  Mainly there for the benefit of
        callback options.
      largs : [string]
        the list of leftover arguments that we have skipped while
        parsing options.  If allow_interspersed_args is false, this
        list is always empty.
      values : Values
        the set of option values currently being accumulated.  Only
        set when parse_args() is active.  Also mainly for callbacks.

    Because of the 'rargs', 'largs', and 'values' attributes,
    OptionParser is not thread-safe.  If, for some perverse reason, you
    need to parse command-line arguments simultaneously in different
    threads, use different OptionParser instances.

    """

    allow_interspersed_args: bool
    epilog: str | None
    formatter: HelpFormatter
    largs: list[str] | None
    option_groups: list[OptionGroup]
    option_list: list[Option]
    process_default_values: bool
    prog: str | None
    rargs: list[str] | None
    standard_option_list: list[Option]
    usage: str | None
    values: Values | None
    version: str
    def __init__(
        self,
        usage: str | None = None,
        option_list: Iterable[Option] | None = None,
        option_class: type[Option] = ...,
        version: str | None = None,
        conflict_handler: str = "error",
        description: str | None = None,
        formatter: HelpFormatter | None = None,
        add_help_option: bool = True,
        prog: str | None = None,
        epilog: str | None = None,
    ) -> None: ...
    def _add_help_option(self) -> None: ...
    def _add_version_option(self) -> None: ...
    def _create_option_list(self) -> None: ...
    def _get_all_options(self) -> list[Option]: ...
    def _get_args(self, args: list[str] | None) -> list[str]: ...
    def _init_parsing_state(self) -> None: ...
    def _match_long_opt(self, opt: str) -> str:
        """_match_long_opt(opt : string) -> string

        Determine which long option string 'opt' matches, ie. which one
        it is an unambiguous abbreviation for.  Raises BadOptionError if
        'opt' doesn't unambiguously match any long option string.
        """

    def _populate_option_list(self, option_list: Iterable[Option] | None, add_help: bool = True) -> None: ...
    def _process_args(self, largs: list[str], rargs: list[str], values: Values) -> None:
        """_process_args(largs : [string],
                         rargs : [string],
                         values : Values)

        Process command-line arguments and populate 'values', consuming
        options and arguments from 'rargs'.  If 'allow_interspersed_args' is
        false, stop at the first non-option argument.  If true, accumulate any
        interspersed non-option arguments in 'largs'.
        """

    def _process_long_opt(self, rargs: list[str], values: Values) -> None: ...
    def _process_short_opts(self, rargs: list[str], values: Values) -> None: ...
    @overload
    def add_option_group(self, opt_group: OptionGroup, /) -> OptionGroup: ...
    @overload
    def add_option_group(self, title: str, /, description: str | None = None) -> OptionGroup: ...
    def check_values(self, values: Values, args: list[str]) -> tuple[Values, list[str]]:
        """
        check_values(values : Values, args : [string])
        -> (values : Values, args : [string])

        Check that the supplied option values and leftover arguments are
        valid.  Returns the option values and leftover arguments
        (possibly adjusted, possibly completely new -- whatever you
        like).  Default implementation just returns the passed-in
        values; subclasses may override as desired.
        """

    def disable_interspersed_args(self) -> None:
        """Set parsing to stop on the first non-option. Use this if
        you have a command processor which runs another command that
        has options of its own and you want to make sure these options
        don't get confused.
        """

    def enable_interspersed_args(self) -> None:
        """Set parsing to not stop on the first non-option, allowing
        interspersing switches with command arguments. This is the
        default behavior. See also disable_interspersed_args() and the
        class documentation description of the attribute
        allow_interspersed_args.
        """

    def error(self, msg: str) -> NoReturn:
        """error(msg : string)

        Print a usage message incorporating 'msg' to stderr and exit.
        If you override this in a subclass, it should not return -- it
        should either exit or raise an exception.
        """

    def exit(self, status: int = 0, msg: str | None = None) -> NoReturn: ...
    def expand_prog_name(self, s: str) -> str: ...
    def format_epilog(self, formatter: HelpFormatter) -> str: ...
    def format_help(self, formatter: HelpFormatter | None = None) -> str: ...
    def format_option_help(self, formatter: HelpFormatter | None = None) -> str: ...
    def get_default_values(self) -> Values: ...
    def get_option_group(self, opt_str: str) -> OptionGroup | None: ...
    def get_prog_name(self) -> str: ...
    def get_usage(self) -> str: ...
    def get_version(self) -> str: ...
    def parse_args(self, args: list[str] | None = None, values: Values | None = None) -> tuple[Values, list[str]]:
        """
        parse_args(args : [string] = sys.argv[1:],
                   values : Values = None)
        -> (values : Values, args : [string])

        Parse the command-line options found in 'args' (default:
        sys.argv[1:]).  Any errors result in a call to 'error()', which
        by default prints the usage message to stderr and calls
        sys.exit() with an error message.  On success returns a pair
        (values, args) where 'values' is a Values instance (with all
        your option values) and 'args' is the list of arguments left
        over after parsing options.
        """

    def print_usage(self, file: SupportsWrite[str] | None = None) -> None:
        """print_usage(file : file = stdout)

        Print the usage message for the current program (self.usage) to
        'file' (default stdout).  Any occurrence of the string "%prog" in
        self.usage is replaced with the name of the current program
        (basename of sys.argv[0]).  Does nothing if self.usage is empty
        or not defined.
        """

    def print_help(self, file: SupportsWrite[str] | None = None) -> None:
        """print_help(file : file = stdout)

        Print an extended help message, listing all options and any
        help text provided with them, to 'file' (default stdout).
        """

    def print_version(self, file: SupportsWrite[str] | None = None) -> None:
        """print_version(file : file = stdout)

        Print the version message for this program (self.version) to
        'file' (default stdout).  As with print_usage(), any occurrence
        of "%prog" in self.version is replaced by the current program's
        name.  Does nothing if self.version is empty or undefined.
        """

    def set_default(self, dest: str, value: Any) -> None: ...  # default value can be "any" type
    def set_defaults(self, **kwargs: Any) -> None: ...  # default values can be "any" type
    def set_process_default_values(self, process: bool) -> None: ...
    def set_usage(self, usage: str | None) -> None: ...
