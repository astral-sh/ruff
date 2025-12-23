"""Command-line parsing library

This module is an optparse-inspired command-line parsing library that:

    - handles both optional and positional arguments
    - produces highly informative usage messages
    - supports parsers that dispatch to sub-parsers

The following is a simple usage example that sums integers from the
command-line and writes the result to a file::

    parser = argparse.ArgumentParser(
        description='sum the integers at the command line')
    parser.add_argument(
        'integers', metavar='int', nargs='+', type=int,
        help='an integer to be summed')
    parser.add_argument(
        '--log',
        help='the file where the sum should be written')
    args = parser.parse_args()
    with (open(args.log, 'w') if args.log is not None
          else contextlib.nullcontext(sys.stdout)) as log:
        log.write('%s' % sum(args.integers))

The module contains the following public classes:

    - ArgumentParser -- The main entry point for command-line parsing. As the
        example above shows, the add_argument() method is used to populate
        the parser with actions for optional and positional arguments. Then
        the parse_args() method is invoked to convert the args at the
        command-line into an object with attributes.

    - ArgumentError -- The exception raised by ArgumentParser objects when
        there are errors with the parser's actions. Errors raised while
        parsing the command-line are caught by ArgumentParser and emitted
        as command-line messages.

    - FileType -- A factory for defining types of files to be created. As the
        example above shows, instances of FileType are typically passed as
        the type= argument of add_argument() calls. Deprecated since
        Python 3.14.

    - Action -- The base class for parser actions. Typically actions are
        selected by passing strings like 'store_true' or 'append_const' to
        the action= argument of add_argument(). However, for greater
        customization of ArgumentParser actions, subclasses of Action may
        be defined and passed as the action= argument.

    - HelpFormatter, RawDescriptionHelpFormatter, RawTextHelpFormatter,
        ArgumentDefaultsHelpFormatter -- Formatter classes which
        may be passed as the formatter_class= argument to the
        ArgumentParser constructor. HelpFormatter is the default,
        RawDescriptionHelpFormatter and RawTextHelpFormatter tell the parser
        not to change the formatting for help text, and
        ArgumentDefaultsHelpFormatter adds information about argument defaults
        to the help.

All other classes in this module are considered implementation details.
(Also note that HelpFormatter and RawDescriptionHelpFormatter are only
considered public as object names -- the API of the formatter objects is
still considered an implementation detail.)
"""

import sys
from _typeshed import SupportsWrite, sentinel
from collections.abc import Callable, Generator, Iterable, Sequence
from re import Pattern
from typing import IO, Any, ClassVar, Final, Generic, NoReturn, Protocol, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias, deprecated

__all__ = [
    "ArgumentParser",
    "ArgumentError",
    "ArgumentTypeError",
    "FileType",
    "HelpFormatter",
    "ArgumentDefaultsHelpFormatter",
    "RawDescriptionHelpFormatter",
    "RawTextHelpFormatter",
    "MetavarTypeHelpFormatter",
    "Namespace",
    "Action",
    "BooleanOptionalAction",
    "ONE_OR_MORE",
    "OPTIONAL",
    "PARSER",
    "REMAINDER",
    "SUPPRESS",
    "ZERO_OR_MORE",
]

_T = TypeVar("_T")
_ActionT = TypeVar("_ActionT", bound=Action)
_ArgumentParserT = TypeVar("_ArgumentParserT", bound=ArgumentParser)
_N = TypeVar("_N")
_ActionType: TypeAlias = Callable[[str], Any] | FileType | str

ONE_OR_MORE: Final = "+"
OPTIONAL: Final = "?"
PARSER: Final = "A..."
REMAINDER: Final = "..."
SUPPRESS: Final = "==SUPPRESS=="
ZERO_OR_MORE: Final = "*"
_UNRECOGNIZED_ARGS_ATTR: Final = "_unrecognized_args"  # undocumented

class ArgumentError(Exception):
    """An error from creating or using an argument (optional or positional).

    The string value of this exception is the message, augmented with
    information about the argument that caused it.
    """

    argument_name: str | None
    message: str
    def __init__(self, argument: Action | None, message: str) -> None: ...

# undocumented
class _AttributeHolder:
    """Abstract base class that provides __repr__.

    The __repr__ method returns a string in the format::
        ClassName(attr=name, attr=name, ...)
    The attributes are determined either by a class-level attribute,
    '_kwarg_names', or by inspecting the instance __dict__.
    """

    def _get_kwargs(self) -> list[tuple[str, Any]]: ...
    def _get_args(self) -> list[Any]: ...

# undocumented
class _ActionsContainer:
    description: str | None
    prefix_chars: str
    argument_default: Any
    conflict_handler: str

    _registries: dict[str, dict[Any, Any]]
    _actions: list[Action]
    _option_string_actions: dict[str, Action]
    _action_groups: list[_ArgumentGroup]
    _mutually_exclusive_groups: list[_MutuallyExclusiveGroup]
    _defaults: dict[str, Any]
    _negative_number_matcher: Pattern[str]
    _has_negative_number_optionals: list[bool]
    def __init__(self, description: str | None, prefix_chars: str, argument_default: Any, conflict_handler: str) -> None: ...
    def register(self, registry_name: str, value: Any, object: Any) -> None: ...
    def _registry_get(self, registry_name: str, value: Any, default: Any = None) -> Any: ...
    def set_defaults(self, **kwargs: Any) -> None: ...
    def get_default(self, dest: str) -> Any: ...
    def add_argument(
        self,
        *name_or_flags: str,
        # str covers predefined actions ("store_true", "count", etc.)
        # and user registered actions via the `register` method.
        action: str | type[Action] = ...,
        # more precisely, Literal["?", "*", "+", "...", "A...", "==SUPPRESS=="],
        # but using this would make it hard to annotate callers that don't use a
        # literal argument and for subclasses to override this method.
        nargs: int | str | None = None,
        const: Any = ...,
        default: Any = ...,
        type: _ActionType = ...,
        choices: Iterable[_T] | None = ...,
        required: bool = ...,
        help: str | None = ...,
        metavar: str | tuple[str, ...] | None = ...,
        dest: str | None = ...,
        version: str = ...,
        **kwargs: Any,
    ) -> Action:
        """
        add_argument(dest, ..., name=value, ...)
        add_argument(option_string, option_string, ..., name=value, ...)
        """

    def add_argument_group(
        self,
        title: str | None = None,
        description: str | None = None,
        *,
        prefix_chars: str = ...,
        argument_default: Any = ...,
        conflict_handler: str = ...,
    ) -> _ArgumentGroup: ...
    def add_mutually_exclusive_group(self, *, required: bool = False) -> _MutuallyExclusiveGroup: ...
    def _add_action(self, action: _ActionT) -> _ActionT: ...
    def _remove_action(self, action: Action) -> None: ...
    def _add_container_actions(self, container: _ActionsContainer) -> None: ...
    def _get_positional_kwargs(self, dest: str, **kwargs: Any) -> dict[str, Any]: ...
    def _get_optional_kwargs(self, *args: Any, **kwargs: Any) -> dict[str, Any]: ...
    def _pop_action_class(self, kwargs: Any, default: type[Action] | None = None) -> type[Action]: ...
    def _get_handler(self) -> Callable[[Action, Iterable[tuple[str, Action]]], Any]: ...
    def _check_conflict(self, action: Action) -> None: ...
    def _handle_conflict_error(self, action: Action, conflicting_actions: Iterable[tuple[str, Action]]) -> NoReturn: ...
    def _handle_conflict_resolve(self, action: Action, conflicting_actions: Iterable[tuple[str, Action]]) -> None: ...

@type_check_only
class _FormatterClass(Protocol):
    def __call__(self, *, prog: str) -> HelpFormatter: ...

class ArgumentParser(_AttributeHolder, _ActionsContainer):
    """Object for parsing command line strings into Python objects.

    Keyword Arguments:
        - prog -- The name of the program (default:
            ``os.path.basename(sys.argv[0])``)
        - usage -- A usage message (default: auto-generated from arguments)
        - description -- A description of what the program does
        - epilog -- Text following the argument descriptions
        - parents -- Parsers whose arguments should be copied into this one
        - formatter_class -- HelpFormatter class for printing help messages
        - prefix_chars -- Characters that prefix optional arguments
        - fromfile_prefix_chars -- Characters that prefix files containing
            additional arguments
        - argument_default -- The default value for all arguments
        - conflict_handler -- String indicating how to handle conflicts
        - add_help -- Add a -h/-help option
        - allow_abbrev -- Allow long options to be abbreviated unambiguously
        - exit_on_error -- Determines whether or not ArgumentParser exits with
            error info when an error occurs
        - suggest_on_error - Enables suggestions for mistyped argument choices
            and subparser names (default: ``False``)
        - color - Allow color output in help messages (default: ``False``)
    """

    prog: str
    usage: str | None
    epilog: str | None
    formatter_class: _FormatterClass
    fromfile_prefix_chars: str | None
    add_help: bool
    allow_abbrev: bool
    exit_on_error: bool

    if sys.version_info >= (3, 14):
        suggest_on_error: bool
        color: bool

    # undocumented
    _positionals: _ArgumentGroup
    _optionals: _ArgumentGroup
    _subparsers: _ArgumentGroup | None

    # Note: the constructor arguments are also used in _SubParsersAction.add_parser.
    if sys.version_info >= (3, 14):
        def __init__(
            self,
            prog: str | None = None,
            usage: str | None = None,
            description: str | None = None,
            epilog: str | None = None,
            parents: Sequence[ArgumentParser] = [],
            formatter_class: _FormatterClass = ...,
            prefix_chars: str = "-",
            fromfile_prefix_chars: str | None = None,
            argument_default: Any = None,
            conflict_handler: str = "error",
            add_help: bool = True,
            allow_abbrev: bool = True,
            exit_on_error: bool = True,
            *,
            suggest_on_error: bool = False,
            color: bool = True,
        ) -> None: ...
    else:
        def __init__(
            self,
            prog: str | None = None,
            usage: str | None = None,
            description: str | None = None,
            epilog: str | None = None,
            parents: Sequence[ArgumentParser] = [],
            formatter_class: _FormatterClass = ...,
            prefix_chars: str = "-",
            fromfile_prefix_chars: str | None = None,
            argument_default: Any = None,
            conflict_handler: str = "error",
            add_help: bool = True,
            allow_abbrev: bool = True,
            exit_on_error: bool = True,
        ) -> None: ...

    @overload
    def parse_args(self, args: Sequence[str] | None = None, namespace: None = None) -> Namespace: ...
    @overload
    def parse_args(self, args: Sequence[str] | None, namespace: _N) -> _N: ...
    @overload
    def parse_args(self, *, namespace: _N) -> _N: ...
    @overload
    def add_subparsers(
        self: _ArgumentParserT,
        *,
        title: str = "subcommands",
        description: str | None = None,
        prog: str | None = None,
        action: type[Action] = ...,
        option_string: str = ...,
        dest: str | None = None,
        required: bool = False,
        help: str | None = None,
        metavar: str | None = None,
    ) -> _SubParsersAction[_ArgumentParserT]: ...
    @overload
    def add_subparsers(
        self,
        *,
        title: str = "subcommands",
        description: str | None = None,
        prog: str | None = None,
        parser_class: type[_ArgumentParserT],
        action: type[Action] = ...,
        option_string: str = ...,
        dest: str | None = None,
        required: bool = False,
        help: str | None = None,
        metavar: str | None = None,
    ) -> _SubParsersAction[_ArgumentParserT]: ...
    def print_usage(self, file: SupportsWrite[str] | None = None) -> None: ...
    def print_help(self, file: SupportsWrite[str] | None = None) -> None: ...
    def format_usage(self) -> str: ...
    def format_help(self) -> str: ...
    @overload
    def parse_known_args(self, args: Sequence[str] | None = None, namespace: None = None) -> tuple[Namespace, list[str]]: ...
    @overload
    def parse_known_args(self, args: Sequence[str] | None, namespace: _N) -> tuple[_N, list[str]]: ...
    @overload
    def parse_known_args(self, *, namespace: _N) -> tuple[_N, list[str]]: ...
    def convert_arg_line_to_args(self, arg_line: str) -> list[str]: ...
    def exit(self, status: int = 0, message: str | None = None) -> NoReturn: ...
    def error(self, message: str) -> NoReturn:
        """error(message: string)

        Prints a usage message incorporating the message to stderr and
        exits.

        If you override this in a subclass, it should not return -- it
        should either exit or raise an exception.
        """

    @overload
    def parse_intermixed_args(self, args: Sequence[str] | None = None, namespace: None = None) -> Namespace: ...
    @overload
    def parse_intermixed_args(self, args: Sequence[str] | None, namespace: _N) -> _N: ...
    @overload
    def parse_intermixed_args(self, *, namespace: _N) -> _N: ...
    @overload
    def parse_known_intermixed_args(
        self, args: Sequence[str] | None = None, namespace: None = None
    ) -> tuple[Namespace, list[str]]: ...
    @overload
    def parse_known_intermixed_args(self, args: Sequence[str] | None, namespace: _N) -> tuple[_N, list[str]]: ...
    @overload
    def parse_known_intermixed_args(self, *, namespace: _N) -> tuple[_N, list[str]]: ...
    # undocumented
    def _get_optional_actions(self) -> list[Action]: ...
    def _get_positional_actions(self) -> list[Action]: ...
    if sys.version_info >= (3, 12):
        def _parse_known_args(
            self, arg_strings: list[str], namespace: Namespace, intermixed: bool
        ) -> tuple[Namespace, list[str]]: ...
    else:
        def _parse_known_args(self, arg_strings: list[str], namespace: Namespace) -> tuple[Namespace, list[str]]: ...

    def _read_args_from_files(self, arg_strings: list[str]) -> list[str]: ...
    def _match_argument(self, action: Action, arg_strings_pattern: str) -> int: ...
    def _match_arguments_partial(self, actions: Sequence[Action], arg_strings_pattern: str) -> list[int]: ...
    if sys.version_info >= (3, 12):
        def _parse_optional(self, arg_string: str) -> list[tuple[Action | None, str, str | None, str | None]] | None: ...
    else:
        def _parse_optional(self, arg_string: str) -> tuple[Action | None, str, str | None] | None: ...

    def _get_option_tuples(self, option_string: str) -> list[tuple[Action, str, str | None]]: ...
    def _get_nargs_pattern(self, action: Action) -> str: ...
    def _get_values(self, action: Action, arg_strings: list[str]) -> Any: ...
    def _get_value(self, action: Action, arg_string: str) -> Any: ...
    def _check_value(self, action: Action, value: Any) -> None: ...
    def _get_formatter(self) -> HelpFormatter: ...
    def _print_message(self, message: str, file: SupportsWrite[str] | None = None) -> None: ...

class HelpFormatter:
    """Formatter for generating usage messages and argument help strings.

    Only the name of this class is considered a public API. All the methods
    provided by the class are considered an implementation detail.
    """

    # undocumented
    _prog: str
    _indent_increment: int
    _max_help_position: int
    _width: int
    _current_indent: int
    _level: int
    _action_max_length: int
    _root_section: _Section
    _current_section: _Section
    _whitespace_matcher: Pattern[str]
    _long_break_matcher: Pattern[str]

    class _Section:
        formatter: HelpFormatter
        heading: str | None
        parent: Self | None
        items: list[tuple[Callable[..., str], Iterable[Any]]]
        def __init__(self, formatter: HelpFormatter, parent: Self | None, heading: str | None = None) -> None: ...
        def format_help(self) -> str: ...

    if sys.version_info >= (3, 14):
        def __init__(
            self, prog: str, indent_increment: int = 2, max_help_position: int = 24, width: int | None = None, color: bool = True
        ) -> None: ...
    else:
        def __init__(
            self, prog: str, indent_increment: int = 2, max_help_position: int = 24, width: int | None = None
        ) -> None: ...

    def _indent(self) -> None: ...
    def _dedent(self) -> None: ...
    def _add_item(self, func: Callable[..., str], args: Iterable[Any]) -> None: ...
    def start_section(self, heading: str | None) -> None: ...
    def end_section(self) -> None: ...
    def add_text(self, text: str | None) -> None: ...
    def add_usage(
        self, usage: str | None, actions: Iterable[Action], groups: Iterable[_MutuallyExclusiveGroup], prefix: str | None = None
    ) -> None: ...
    def add_argument(self, action: Action) -> None: ...
    def add_arguments(self, actions: Iterable[Action]) -> None: ...
    def format_help(self) -> str: ...
    def _join_parts(self, part_strings: Iterable[str]) -> str: ...
    def _format_usage(
        self, usage: str | None, actions: Iterable[Action], groups: Iterable[_MutuallyExclusiveGroup], prefix: str | None
    ) -> str: ...
    def _format_actions_usage(self, actions: Iterable[Action], groups: Iterable[_MutuallyExclusiveGroup]) -> str: ...
    def _format_text(self, text: str) -> str: ...
    def _format_action(self, action: Action) -> str: ...
    def _format_action_invocation(self, action: Action) -> str: ...
    def _metavar_formatter(self, action: Action, default_metavar: str) -> Callable[[int], tuple[str, ...]]: ...
    def _format_args(self, action: Action, default_metavar: str) -> str: ...
    def _expand_help(self, action: Action) -> str: ...
    def _iter_indented_subactions(self, action: Action) -> Generator[Action, None, None]: ...
    def _split_lines(self, text: str, width: int) -> list[str]: ...
    def _fill_text(self, text: str, width: int, indent: str) -> str: ...
    def _get_help_string(self, action: Action) -> str | None: ...
    def _get_default_metavar_for_optional(self, action: Action) -> str: ...
    def _get_default_metavar_for_positional(self, action: Action) -> str: ...

class RawDescriptionHelpFormatter(HelpFormatter):
    """Help message formatter which retains any formatting in descriptions.

    Only the name of this class is considered a public API. All the methods
    provided by the class are considered an implementation detail.
    """

class RawTextHelpFormatter(RawDescriptionHelpFormatter):
    """Help message formatter which retains formatting of all help text.

    Only the name of this class is considered a public API. All the methods
    provided by the class are considered an implementation detail.
    """

class ArgumentDefaultsHelpFormatter(HelpFormatter):
    """Help message formatter which adds default values to argument help.

    Only the name of this class is considered a public API. All the methods
    provided by the class are considered an implementation detail.
    """

class MetavarTypeHelpFormatter(HelpFormatter):
    """Help message formatter which uses the argument 'type' as the default
    metavar value (instead of the argument 'dest')

    Only the name of this class is considered a public API. All the methods
    provided by the class are considered an implementation detail.
    """

class Action(_AttributeHolder):
    """Information about how to convert command line strings to Python objects.

    Action objects are used by an ArgumentParser to represent the information
    needed to parse a single argument from one or more strings from the
    command line. The keyword arguments to the Action constructor are also
    all attributes of Action instances.

    Keyword Arguments:

        - option_strings -- A list of command-line option strings which
            should be associated with this action.

        - dest -- The name of the attribute to hold the created object(s)

        - nargs -- The number of command-line arguments that should be
            consumed. By default, one argument will be consumed and a single
            value will be produced.  Other values include:
                - N (an integer) consumes N arguments (and produces a list)
                - '?' consumes zero or one arguments
                - '*' consumes zero or more arguments (and produces a list)
                - '+' consumes one or more arguments (and produces a list)
            Note that the difference between the default and nargs=1 is that
            with the default, a single value will be produced, while with
            nargs=1, a list containing a single value will be produced.

        - const -- The value to be produced if the option is specified and the
            option uses an action that takes no values.

        - default -- The value to be produced if the option is not specified.

        - type -- A callable that accepts a single string argument, and
            returns the converted value.  The standard Python types str, int,
            float, and complex are useful examples of such callables.  If None,
            str is used.

        - choices -- A container of values that should be allowed. If not None,
            after a command-line argument has been converted to the appropriate
            type, an exception will be raised if it is not a member of this
            collection.

        - required -- True if the action must always be specified at the
            command line. This is only meaningful for optional command-line
            arguments.

        - help -- The help string describing the argument.

        - metavar -- The name to be used for the option's argument with the
            help string. If None, the 'dest' value will be used as the name.
    """

    option_strings: Sequence[str]
    dest: str
    nargs: int | str | None
    const: Any
    default: Any
    type: _ActionType | None
    choices: Iterable[Any] | None
    required: bool
    help: str | None
    metavar: str | tuple[str, ...] | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            nargs: int | str | None = None,
            const: _T | None = None,
            default: _T | str | None = None,
            type: Callable[[str], _T] | FileType | None = None,
            choices: Iterable[_T] | None = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
            deprecated: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            nargs: int | str | None = None,
            const: _T | None = None,
            default: _T | str | None = None,
            type: Callable[[str], _T] | FileType | None = None,
            choices: Iterable[_T] | None = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...

    def __call__(
        self, parser: ArgumentParser, namespace: Namespace, values: str | Sequence[Any] | None, option_string: str | None = None
    ) -> None: ...
    def format_usage(self) -> str: ...

if sys.version_info >= (3, 12):
    class BooleanOptionalAction(Action):
        if sys.version_info >= (3, 14):
            def __init__(
                self,
                option_strings: Sequence[str],
                dest: str,
                default: bool | None = None,
                required: bool = False,
                help: str | None = None,
                deprecated: bool = False,
            ) -> None: ...
        elif sys.version_info >= (3, 13):
            @overload
            def __init__(
                self,
                option_strings: Sequence[str],
                dest: str,
                default: bool | None = None,
                *,
                required: bool = False,
                help: str | None = None,
                deprecated: bool = False,
            ) -> None: ...
            @overload
            @deprecated("The `type`, `choices`, and `metavar` parameters are ignored and will be removed in Python 3.14.")
            def __init__(
                self,
                option_strings: Sequence[str],
                dest: str,
                default: _T | bool | None = None,
                type: Callable[[str], _T] | FileType | None = sentinel,
                choices: Iterable[_T] | None = sentinel,
                required: bool = False,
                help: str | None = None,
                metavar: str | tuple[str, ...] | None = sentinel,
                deprecated: bool = False,
            ) -> None: ...
        else:
            @overload
            def __init__(
                self,
                option_strings: Sequence[str],
                dest: str,
                default: bool | None = None,
                *,
                required: bool = False,
                help: str | None = None,
            ) -> None: ...
            @overload
            @deprecated("The `type`, `choices`, and `metavar` parameters are ignored and will be removed in Python 3.14.")
            def __init__(
                self,
                option_strings: Sequence[str],
                dest: str,
                default: _T | bool | None = None,
                type: Callable[[str], _T] | FileType | None = sentinel,
                choices: Iterable[_T] | None = sentinel,
                required: bool = False,
                help: str | None = None,
                metavar: str | tuple[str, ...] | None = sentinel,
            ) -> None: ...

else:
    class BooleanOptionalAction(Action):
        @overload
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            default: bool | None = None,
            *,
            required: bool = False,
            help: str | None = None,
        ) -> None: ...
        @overload
        @deprecated("The `type`, `choices`, and `metavar` parameters are ignored and will be removed in Python 3.14.")
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            default: _T | bool | None = None,
            type: Callable[[str], _T] | FileType | None = None,
            choices: Iterable[_T] | None = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...

class Namespace(_AttributeHolder):
    """Simple object for storing attributes.

    Implements equality by attribute names and values, and provides a simple
    string representation.
    """

    def __init__(self, **kwargs: Any) -> None: ...
    def __getattr__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any, /) -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

if sys.version_info >= (3, 14):
    @deprecated("Deprecated since Python 3.14. Open files after parsing arguments instead.")
    class FileType:
        """Deprecated factory for creating file object types

        Instances of FileType are typically passed as type= arguments to the
        ArgumentParser add_argument() method.

        Keyword Arguments:
            - mode -- A string indicating how the file is to be opened. Accepts the
                same values as the builtin open() function.
            - bufsize -- The file's desired buffer size. Accepts the same values as
                the builtin open() function.
            - encoding -- The file's encoding. Accepts the same values as the
                builtin open() function.
            - errors -- A string indicating how encoding and decoding errors are to
                be handled. Accepts the same value as the builtin open() function.
        """

        # undocumented
        _mode: str
        _bufsize: int
        _encoding: str | None
        _errors: str | None
        def __init__(
            self, mode: str = "r", bufsize: int = -1, encoding: str | None = None, errors: str | None = None
        ) -> None: ...
        def __call__(self, string: str) -> IO[Any]: ...

else:
    class FileType:
        """Factory for creating file object types

        Instances of FileType are typically passed as type= arguments to the
        ArgumentParser add_argument() method.

        Keyword Arguments:
            - mode -- A string indicating how the file is to be opened. Accepts the
                same values as the builtin open() function.
            - bufsize -- The file's desired buffer size. Accepts the same values as
                the builtin open() function.
            - encoding -- The file's encoding. Accepts the same values as the
                builtin open() function.
            - errors -- A string indicating how encoding and decoding errors are to
                be handled. Accepts the same value as the builtin open() function.
        """

        # undocumented
        _mode: str
        _bufsize: int
        _encoding: str | None
        _errors: str | None
        def __init__(
            self, mode: str = "r", bufsize: int = -1, encoding: str | None = None, errors: str | None = None
        ) -> None: ...
        def __call__(self, string: str) -> IO[Any]: ...

# undocumented
class _ArgumentGroup(_ActionsContainer):
    title: str | None
    _group_actions: list[Action]
    if sys.version_info >= (3, 14):
        @overload
        def __init__(
            self,
            container: _ActionsContainer,
            title: str | None = None,
            description: str | None = None,
            *,
            argument_default: Any = ...,
            conflict_handler: str = ...,
        ) -> None: ...
        @overload
        @deprecated("Undocumented `prefix_chars` parameter is deprecated since Python 3.14.")
        def __init__(
            self,
            container: _ActionsContainer,
            title: str | None = None,
            description: str | None = None,
            *,
            prefix_chars: str,
            argument_default: Any = ...,
            conflict_handler: str = ...,
        ) -> None: ...
    else:
        def __init__(
            self,
            container: _ActionsContainer,
            title: str | None = None,
            description: str | None = None,
            *,
            prefix_chars: str = ...,
            argument_default: Any = ...,
            conflict_handler: str = ...,
        ) -> None: ...

# undocumented
class _MutuallyExclusiveGroup(_ArgumentGroup):
    required: bool
    _container: _ActionsContainer
    def __init__(self, container: _ActionsContainer, required: bool = False) -> None: ...

# undocumented
class _StoreAction(Action): ...

# undocumented
class _StoreConstAction(Action):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any | None = None,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
            deprecated: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any | None = None,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...

# undocumented
class _StoreTrueAction(_StoreConstAction):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            default: bool = False,
            required: bool = False,
            help: str | None = None,
            deprecated: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self, option_strings: Sequence[str], dest: str, default: bool = False, required: bool = False, help: str | None = None
        ) -> None: ...

# undocumented
class _StoreFalseAction(_StoreConstAction):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            default: bool = True,
            required: bool = False,
            help: str | None = None,
            deprecated: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self, option_strings: Sequence[str], dest: str, default: bool = True, required: bool = False, help: str | None = None
        ) -> None: ...

# undocumented
class _AppendAction(Action): ...

# undocumented
class _ExtendAction(_AppendAction): ...

# undocumented
class _AppendConstAction(Action):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any | None = None,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
            deprecated: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any | None = None,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            const: Any,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            metavar: str | tuple[str, ...] | None = None,
        ) -> None: ...

# undocumented
class _CountAction(Action):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str,
            default: Any = None,
            required: bool = False,
            help: str | None = None,
            deprecated: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self, option_strings: Sequence[str], dest: str, default: Any = None, required: bool = False, help: str | None = None
        ) -> None: ...

# undocumented
class _HelpAction(Action):
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str = "==SUPPRESS==",
            default: str = "==SUPPRESS==",
            help: str | None = None,
            deprecated: bool = False,
        ) -> None: ...
    else:
        def __init__(
            self,
            option_strings: Sequence[str],
            dest: str = "==SUPPRESS==",
            default: str = "==SUPPRESS==",
            help: str | None = None,
        ) -> None: ...

# undocumented
class _VersionAction(Action):
    version: str | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            option_strings: Sequence[str],
            version: str | None = None,
            dest: str = "==SUPPRESS==",
            default: str = "==SUPPRESS==",
            help: str | None = None,
            deprecated: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            option_strings: Sequence[str],
            version: str | None = None,
            dest: str = "==SUPPRESS==",
            default: str = "==SUPPRESS==",
            help: str | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            option_strings: Sequence[str],
            version: str | None = None,
            dest: str = "==SUPPRESS==",
            default: str = "==SUPPRESS==",
            help: str = "show program's version number and exit",
        ) -> None: ...

# undocumented
class _SubParsersAction(Action, Generic[_ArgumentParserT]):
    _ChoicesPseudoAction: type[Any]  # nested class
    _prog_prefix: str
    _parser_class: type[_ArgumentParserT]
    _name_parser_map: dict[str, _ArgumentParserT]
    choices: dict[str, _ArgumentParserT]
    _choices_actions: list[Action]
    def __init__(
        self,
        option_strings: Sequence[str],
        prog: str,
        parser_class: type[_ArgumentParserT],
        dest: str = "==SUPPRESS==",
        required: bool = False,
        help: str | None = None,
        metavar: str | tuple[str, ...] | None = None,
    ) -> None: ...

    # Note: `add_parser` accepts all kwargs of `ArgumentParser.__init__`. It also
    # accepts its own `help` and `aliases` kwargs.
    if sys.version_info >= (3, 14):
        def add_parser(
            self,
            name: str,
            *,
            deprecated: bool = False,
            help: str | None = ...,
            aliases: Sequence[str] = ...,
            # Kwargs from ArgumentParser constructor
            prog: str | None = ...,
            usage: str | None = ...,
            description: str | None = ...,
            epilog: str | None = ...,
            parents: Sequence[_ArgumentParserT] = ...,
            formatter_class: _FormatterClass = ...,
            prefix_chars: str = ...,
            fromfile_prefix_chars: str | None = ...,
            argument_default: Any = ...,
            conflict_handler: str = ...,
            add_help: bool = True,
            allow_abbrev: bool = True,
            exit_on_error: bool = True,
            suggest_on_error: bool = False,
            color: bool = False,
            **kwargs: Any,  # Accepting any additional kwargs for custom parser classes
        ) -> _ArgumentParserT: ...
    elif sys.version_info >= (3, 13):
        def add_parser(
            self,
            name: str,
            *,
            deprecated: bool = False,
            help: str | None = ...,
            aliases: Sequence[str] = ...,
            # Kwargs from ArgumentParser constructor
            prog: str | None = ...,
            usage: str | None = ...,
            description: str | None = ...,
            epilog: str | None = ...,
            parents: Sequence[_ArgumentParserT] = ...,
            formatter_class: _FormatterClass = ...,
            prefix_chars: str = ...,
            fromfile_prefix_chars: str | None = ...,
            argument_default: Any = ...,
            conflict_handler: str = ...,
            add_help: bool = True,
            allow_abbrev: bool = True,
            exit_on_error: bool = True,
            **kwargs: Any,  # Accepting any additional kwargs for custom parser classes
        ) -> _ArgumentParserT: ...
    else:
        def add_parser(
            self,
            name: str,
            *,
            help: str | None = ...,
            aliases: Sequence[str] = ...,
            # Kwargs from ArgumentParser constructor
            prog: str | None = ...,
            usage: str | None = ...,
            description: str | None = ...,
            epilog: str | None = ...,
            parents: Sequence[_ArgumentParserT] = ...,
            formatter_class: _FormatterClass = ...,
            prefix_chars: str = ...,
            fromfile_prefix_chars: str | None = ...,
            argument_default: Any = ...,
            conflict_handler: str = ...,
            add_help: bool = True,
            allow_abbrev: bool = True,
            exit_on_error: bool = True,
            **kwargs: Any,  # Accepting any additional kwargs for custom parser classes
        ) -> _ArgumentParserT: ...

    def _get_subactions(self) -> list[Action]: ...

# undocumented
class ArgumentTypeError(Exception):
    """An error from trying to convert a command line string to a type."""

# undocumented
def _get_action_name(argument: Action | None) -> str | None: ...
