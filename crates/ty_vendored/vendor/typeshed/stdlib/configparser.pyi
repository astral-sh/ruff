"""Configuration file parser.

A configuration file consists of sections, lead by a "[section]" header,
and followed by "name: value" entries, with continuations and such in
the style of RFC 822.

Intrinsic defaults can be specified by passing them into the
ConfigParser constructor as a dictionary.

class:

ConfigParser -- responsible for parsing a list of
                    configuration files, and managing the parsed database.

    methods:

    __init__(defaults=None, dict_type=_default_dict, allow_no_value=False,
             delimiters=('=', ':'), comment_prefixes=('#', ';'),
             inline_comment_prefixes=None, strict=True,
             empty_lines_in_values=True, default_section='DEFAULT',
             interpolation=<unset>, converters=<unset>,
             allow_unnamed_section=False):
        Create the parser. When `defaults` is given, it is initialized into the
        dictionary or intrinsic defaults. The keys must be strings, the values
        must be appropriate for %()s string interpolation.

        When `dict_type` is given, it will be used to create the dictionary
        objects for the list of sections, for the options within a section, and
        for the default values.

        When `delimiters` is given, it will be used as the set of substrings
        that divide keys from values.

        When `comment_prefixes` is given, it will be used as the set of
        substrings that prefix comments in empty lines. Comments can be
        indented.

        When `inline_comment_prefixes` is given, it will be used as the set of
        substrings that prefix comments in non-empty lines.

        When `strict` is True, the parser won't allow for any section or option
        duplicates while reading from a single source (file, string or
        dictionary). Default is True.

        When `empty_lines_in_values` is False (default: True), each empty line
        marks the end of an option. Otherwise, internal empty lines of
        a multiline option are kept as part of the value.

        When `allow_no_value` is True (default: False), options without
        values are accepted; the value presented for these is None.

        When `default_section` is given, the name of the special section is
        named accordingly. By default it is called ``"DEFAULT"`` but this can
        be customized to point to any other valid section name. Its current
        value can be retrieved using the ``parser_instance.default_section``
        attribute and may be modified at runtime.

        When `interpolation` is given, it should be an Interpolation subclass
        instance. It will be used as the handler for option value
        pre-processing when using getters. RawConfigParser objects don't do
        any sort of interpolation, whereas ConfigParser uses an instance of
        BasicInterpolation. The library also provides a ``zc.buildout``
        inspired ExtendedInterpolation implementation.

        When `converters` is given, it should be a dictionary where each key
        represents the name of a type converter and each value is a callable
        implementing the conversion from string to the desired datatype. Every
        converter gets its corresponding get*() method on the parser object and
        section proxies.

        When `allow_unnamed_section` is True (default: False), options
        without section are accepted: the section for these is
        ``configparser.UNNAMED_SECTION``.

    sections()
        Return all the configuration section names, sans DEFAULT.

    has_section(section)
        Return whether the given section exists.

    has_option(section, option)
        Return whether the given option exists in the given section.

    options(section)
        Return list of configuration options for the named section.

    read(filenames, encoding=None)
        Read and parse the iterable of named configuration files, given by
        name.  A single filename is also allowed.  Non-existing files
        are ignored.  Return list of successfully read files.

    read_file(f, filename=None)
        Read and parse one configuration file, given as a file object.
        The filename defaults to f.name; it is only used in error
        messages (if f has no `name` attribute, the string `<???>` is used).

    read_string(string)
        Read configuration from a given string.

    read_dict(dictionary)
        Read configuration from a dictionary. Keys are section names,
        values are dictionaries with keys and values that should be present
        in the section. If the used dictionary type preserves order, sections
        and their keys will be added in order. Values are automatically
        converted to strings.

    get(section, option, raw=False, vars=None, fallback=_UNSET)
        Return a string value for the named option.  All % interpolations are
        expanded in the return values, based on the defaults passed into the
        constructor and the DEFAULT section.  Additional substitutions may be
        provided using the `vars` argument, which must be a dictionary whose
        contents override any pre-existing defaults. If `option` is a key in
        `vars`, the value from `vars` is used.

    getint(section, options, raw=False, vars=None, fallback=_UNSET)
        Like get(), but convert value to an integer.

    getfloat(section, options, raw=False, vars=None, fallback=_UNSET)
        Like get(), but convert value to a float.

    getboolean(section, options, raw=False, vars=None, fallback=_UNSET)
        Like get(), but convert value to a boolean (currently case
        insensitively defined as 0, false, no, off for False, and 1, true,
        yes, on for True).  Returns False or True.

    items(section=_UNSET, raw=False, vars=None)
        If section is given, return a list of tuples with (name, value) for
        each option in the section. Otherwise, return a list of tuples with
        (section_name, section_proxy) for each section, including DEFAULTSECT.

    remove_section(section)
        Remove the given file section and all its options.

    remove_option(section, option)
        Remove the given option from the given section.

    set(section, option, value)
        Set the given option.

    write(fp, space_around_delimiters=True)
        Write the configuration state in .ini format. If
        `space_around_delimiters` is True (the default), delimiters
        between keys and values are surrounded by spaces.
"""

import sys
from _typeshed import MaybeNone, StrOrBytesPath, SupportsWrite
from collections.abc import Callable, ItemsView, Iterable, Iterator, Mapping, MutableMapping, Sequence
from re import Pattern
from typing import Any, ClassVar, Final, Literal, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, deprecated

if sys.version_info >= (3, 14):
    __all__ = (
        "NoSectionError",
        "DuplicateOptionError",
        "DuplicateSectionError",
        "NoOptionError",
        "InterpolationError",
        "InterpolationDepthError",
        "InterpolationMissingOptionError",
        "InterpolationSyntaxError",
        "ParsingError",
        "MissingSectionHeaderError",
        "MultilineContinuationError",
        "UnnamedSectionDisabledError",
        "InvalidWriteError",
        "ConfigParser",
        "RawConfigParser",
        "Interpolation",
        "BasicInterpolation",
        "ExtendedInterpolation",
        "SectionProxy",
        "ConverterMapping",
        "DEFAULTSECT",
        "MAX_INTERPOLATION_DEPTH",
        "UNNAMED_SECTION",
    )
elif sys.version_info >= (3, 13):
    __all__ = (
        "NoSectionError",
        "DuplicateOptionError",
        "DuplicateSectionError",
        "NoOptionError",
        "InterpolationError",
        "InterpolationDepthError",
        "InterpolationMissingOptionError",
        "InterpolationSyntaxError",
        "ParsingError",
        "MissingSectionHeaderError",
        "ConfigParser",
        "RawConfigParser",
        "Interpolation",
        "BasicInterpolation",
        "ExtendedInterpolation",
        "SectionProxy",
        "ConverterMapping",
        "DEFAULTSECT",
        "MAX_INTERPOLATION_DEPTH",
        "UNNAMED_SECTION",
        "MultilineContinuationError",
    )
elif sys.version_info >= (3, 12):
    __all__ = (
        "NoSectionError",
        "DuplicateOptionError",
        "DuplicateSectionError",
        "NoOptionError",
        "InterpolationError",
        "InterpolationDepthError",
        "InterpolationMissingOptionError",
        "InterpolationSyntaxError",
        "ParsingError",
        "MissingSectionHeaderError",
        "ConfigParser",
        "RawConfigParser",
        "Interpolation",
        "BasicInterpolation",
        "ExtendedInterpolation",
        "LegacyInterpolation",
        "SectionProxy",
        "ConverterMapping",
        "DEFAULTSECT",
        "MAX_INTERPOLATION_DEPTH",
    )
else:
    __all__ = [
        "NoSectionError",
        "DuplicateOptionError",
        "DuplicateSectionError",
        "NoOptionError",
        "InterpolationError",
        "InterpolationDepthError",
        "InterpolationMissingOptionError",
        "InterpolationSyntaxError",
        "ParsingError",
        "MissingSectionHeaderError",
        "ConfigParser",
        "SafeConfigParser",
        "RawConfigParser",
        "Interpolation",
        "BasicInterpolation",
        "ExtendedInterpolation",
        "LegacyInterpolation",
        "SectionProxy",
        "ConverterMapping",
        "DEFAULTSECT",
        "MAX_INTERPOLATION_DEPTH",
    ]

if sys.version_info >= (3, 13):
    @type_check_only
    class _UNNAMED_SECTION: ...

    UNNAMED_SECTION: _UNNAMED_SECTION

    _SectionName: TypeAlias = str | _UNNAMED_SECTION
    # A list of sections can only include an unnamed section if the parser was initialized with
    # allow_unnamed_section=True. Any prevents users from having to use explicit
    # type checks if allow_unnamed_section is False (the default).
    _SectionNameList: TypeAlias = list[Any]
else:
    _SectionName: TypeAlias = str
    _SectionNameList: TypeAlias = list[str]

_Section: TypeAlias = Mapping[str, str]
_Parser: TypeAlias = MutableMapping[str, _Section]
_ConverterCallback: TypeAlias = Callable[[str], Any]
_ConvertersMap: TypeAlias = dict[str, _ConverterCallback]
_T = TypeVar("_T")

DEFAULTSECT: Final = "DEFAULT"
MAX_INTERPOLATION_DEPTH: Final = 10

class Interpolation:
    """Dummy interpolation that passes the value through with no changes."""

    def before_get(self, parser: _Parser, section: _SectionName, option: str, value: str, defaults: _Section) -> str: ...
    def before_set(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...
    def before_read(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...
    def before_write(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...

class BasicInterpolation(Interpolation):
    """Interpolation as implemented in the classic ConfigParser.

    The option values can contain format strings which refer to other values in
    the same section, or values in the special default section.

    For example:

        something: %(dir)s/whatever

    would resolve the "%(dir)s" to the value of dir.  All reference
    expansions are done late, on demand. If a user needs to use a bare % in
    a configuration file, she can escape it by writing %%. Other % usage
    is considered a user error and raises `InterpolationSyntaxError`.
    """

class ExtendedInterpolation(Interpolation):
    """Advanced variant of interpolation, supports the syntax used by
    `zc.buildout`. Enables interpolation between sections.
    """

if sys.version_info < (3, 13):
    @deprecated(
        "Deprecated since Python 3.2; removed in Python 3.13. Use `BasicInterpolation` or `ExtendedInterpolation` instead."
    )
    class LegacyInterpolation(Interpolation):
        """Deprecated interpolation used in old versions of ConfigParser.
        Use BasicInterpolation or ExtendedInterpolation instead.
        """

        def before_get(self, parser: _Parser, section: _SectionName, option: str, value: str, vars: _Section) -> str: ...

class RawConfigParser(_Parser):
    """ConfigParser that does not do interpolation."""

    _SECT_TMPL: ClassVar[str]  # undocumented
    _OPT_TMPL: ClassVar[str]  # undocumented
    _OPT_NV_TMPL: ClassVar[str]  # undocumented

    SECTCRE: Pattern[str]
    OPTCRE: ClassVar[Pattern[str]]
    OPTCRE_NV: ClassVar[Pattern[str]]  # undocumented
    NONSPACECRE: ClassVar[Pattern[str]]  # undocumented

    BOOLEAN_STATES: ClassVar[Mapping[str, bool]]  # undocumented
    default_section: str
    if sys.version_info >= (3, 13):
        @overload
        def __init__(
            self,
            defaults: Mapping[str, str | None] | None = None,
            dict_type: type[Mapping[str, str]] = ...,
            *,
            allow_no_value: Literal[True],
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
            allow_unnamed_section: bool = False,
        ) -> None: ...
        @overload
        def __init__(
            self,
            defaults: Mapping[str, str | None] | None,
            dict_type: type[Mapping[str, str]],
            allow_no_value: Literal[True],
            *,
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
            allow_unnamed_section: bool = False,
        ) -> None: ...
        @overload
        def __init__(
            self,
            defaults: _Section | None = None,
            dict_type: type[Mapping[str, str]] = ...,
            allow_no_value: bool = False,
            *,
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
            allow_unnamed_section: bool = False,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self,
            defaults: Mapping[str, str | None] | None = None,
            dict_type: type[Mapping[str, str]] = ...,
            *,
            allow_no_value: Literal[True],
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
        ) -> None: ...
        @overload
        def __init__(
            self,
            defaults: Mapping[str, str | None] | None,
            dict_type: type[Mapping[str, str]],
            allow_no_value: Literal[True],
            *,
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
        ) -> None: ...
        @overload
        def __init__(
            self,
            defaults: _Section | None = None,
            dict_type: type[Mapping[str, str]] = ...,
            allow_no_value: bool = False,
            *,
            delimiters: Sequence[str] = ("=", ":"),
            comment_prefixes: Sequence[str] = ("#", ";"),
            inline_comment_prefixes: Sequence[str] | None = None,
            strict: bool = True,
            empty_lines_in_values: bool = True,
            default_section: str = "DEFAULT",
            interpolation: Interpolation | None = ...,
            converters: _ConvertersMap = ...,
        ) -> None: ...

    def __len__(self) -> int: ...
    def __getitem__(self, key: _SectionName) -> SectionProxy: ...
    def __setitem__(self, key: _SectionName, value: _Section) -> None: ...
    def __delitem__(self, key: _SectionName) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    def __contains__(self, key: object) -> bool: ...
    def defaults(self) -> _Section: ...
    def sections(self) -> _SectionNameList:
        """Return a list of section names, excluding [DEFAULT]"""

    def add_section(self, section: _SectionName) -> None:
        """Create a new section in the configuration.

        Raise DuplicateSectionError if a section by the specified name
        already exists. Raise ValueError if name is DEFAULT.
        """

    def has_section(self, section: _SectionName) -> bool:
        """Indicate whether the named section is present in the configuration.

        The DEFAULT section is not acknowledged.
        """

    def options(self, section: _SectionName) -> list[str]:
        """Return a list of option names for the given section name."""

    def has_option(self, section: _SectionName, option: str) -> bool:
        """Check for the existence of a given option in a given section.
        If the specified `section` is None or an empty string, DEFAULT is
        assumed. If the specified `section` does not exist, returns False.
        """

    def read(self, filenames: StrOrBytesPath | Iterable[StrOrBytesPath], encoding: str | None = None) -> list[str]:
        """Read and parse a filename or an iterable of filenames.

        Files that cannot be opened are silently ignored; this is
        designed so that you can specify an iterable of potential
        configuration file locations (e.g. current directory, user's
        home directory, systemwide directory), and all existing
        configuration files in the iterable will be read.  A single
        filename may also be given.

        Return list of successfully read files.
        """

    def read_file(self, f: Iterable[str], source: str | None = None) -> None:
        """Like read() but the argument must be a file-like object.

        The `f` argument must be iterable, returning one line at a time.
        Optional second argument is the `source` specifying the name of the
        file being read. If not given, it is taken from f.name. If `f` has no
        `name` attribute, `<???>` is used.
        """

    def read_string(self, string: str, source: str = "<string>") -> None:
        """Read configuration from a given string."""

    def read_dict(self, dictionary: Mapping[str, Mapping[str, Any]], source: str = "<dict>") -> None:
        """Read configuration from a dictionary.

        Keys are section names, values are dictionaries with keys and values
        that should be present in the section. If the used dictionary type
        preserves order, sections and their keys will be added in order.

        All types held in the dictionary are converted to strings during
        reading, including section names, option names and keys.

        Optional second argument is the `source` specifying the name of the
        dictionary being read.
        """
    if sys.version_info < (3, 12):
        @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `parser.read_file()` instead.")
        def readfp(self, fp: Iterable[str], filename: str | None = None) -> None:
            """Deprecated, use read_file instead."""
    # These get* methods are partially applied (with the same names) in
    # SectionProxy; the stubs should be kept updated together
    @overload
    def getint(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> int: ...
    @overload
    def getint(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T = ...
    ) -> int | _T: ...
    @overload
    def getfloat(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> float: ...
    @overload
    def getfloat(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T = ...
    ) -> float | _T: ...
    @overload
    def getboolean(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> bool: ...
    @overload
    def getboolean(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T = ...
    ) -> bool | _T: ...
    def _get_conv(
        self,
        section: _SectionName,
        option: str,
        conv: Callable[[str], _T],
        *,
        raw: bool = False,
        vars: _Section | None = None,
        fallback: _T = ...,
    ) -> _T: ...
    # This is incompatible with MutableMapping so we ignore the type
    @overload  # type: ignore[override]
    def get(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> str | MaybeNone:
        """Get an option value for a given section.

        If `vars` is provided, it must be a dictionary. The option is looked up
        in `vars` (if provided), `section`, and in `DEFAULTSECT` in that order.
        If the key is not found and `fallback` is provided, it is used as
        a fallback value. `None` can be provided as a `fallback` value.

        If interpolation is enabled and the optional argument `raw` is False,
        all interpolations are expanded in the return values.

        Arguments `raw`, `vars`, and `fallback` are keyword only.

        The section DEFAULT is special.
        """

    @overload
    def get(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T
    ) -> str | _T | MaybeNone: ...
    @overload
    def items(self, *, raw: bool = False, vars: _Section | None = None) -> ItemsView[str, SectionProxy]:
        """Return a list of (name, value) tuples for each option in a section.

        All % interpolations are expanded in the return values, based on the
        defaults passed into the constructor, unless the optional argument
        `raw` is true.  Additional substitutions may be provided using the
        `vars` argument, which must be a dictionary whose contents overrides
        any pre-existing defaults.

        The section DEFAULT is special.
        """

    @overload
    def items(self, section: _SectionName, raw: bool = False, vars: _Section | None = None) -> list[tuple[str, str]]: ...
    def set(self, section: _SectionName, option: str, value: str | None = None) -> None:
        """Set an option."""

    def write(self, fp: SupportsWrite[str], space_around_delimiters: bool = True) -> None:
        """Write an .ini-format representation of the configuration state.

        If `space_around_delimiters` is True (the default), delimiters
        between keys and values are surrounded by spaces.

        Please note that comments in the original configuration file are not
        preserved when writing the configuration back.
        """

    def remove_option(self, section: _SectionName, option: str) -> bool:
        """Remove an option."""

    def remove_section(self, section: _SectionName) -> bool:
        """Remove a file section."""

    def optionxform(self, optionstr: str) -> str: ...
    @property
    def converters(self) -> ConverterMapping: ...

class ConfigParser(RawConfigParser):
    """ConfigParser implementing interpolation."""

    # This is incompatible with MutableMapping so we ignore the type
    @overload  # type: ignore[override]
    def get(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> str:
        """Get an option value for a given section.

        If `vars` is provided, it must be a dictionary. The option is looked up
        in `vars` (if provided), `section`, and in `DEFAULTSECT` in that order.
        If the key is not found and `fallback` is provided, it is used as
        a fallback value. `None` can be provided as a `fallback` value.

        If interpolation is enabled and the optional argument `raw` is False,
        all interpolations are expanded in the return values.

        Arguments `raw`, `vars`, and `fallback` are keyword only.

        The section DEFAULT is special.
        """

    @overload
    def get(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T
    ) -> str | _T: ...

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `ConfigParser` instead.")
    class SafeConfigParser(ConfigParser):
        """ConfigParser alias for backwards compatibility purposes."""

class SectionProxy(MutableMapping[str, str]):
    """A proxy for a single section from a parser."""

    def __init__(self, parser: RawConfigParser, name: str) -> None:
        """Creates a view on a section of the specified `name` in `parser`."""

    def __getitem__(self, key: str) -> str: ...
    def __setitem__(self, key: str, value: str) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __contains__(self, key: object) -> bool: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[str]: ...
    @property
    def parser(self) -> RawConfigParser: ...
    @property
    def name(self) -> str: ...
    # This is incompatible with MutableMapping so we ignore the type
    @overload  # type: ignore[override]
    def get(
        self,
        option: str,
        fallback: None = None,
        *,
        raw: bool = False,
        vars: _Section | None = None,
        _impl: Any | None = None,
        **kwargs: Any,  # passed to the underlying parser's get() method
    ) -> str | None:
        """Get an option value.

        Unless `fallback` is provided, `None` will be returned if the option
        is not found.

        """

    @overload
    def get(
        self,
        option: str,
        fallback: _T,
        *,
        raw: bool = False,
        vars: _Section | None = None,
        _impl: Any | None = None,
        **kwargs: Any,  # passed to the underlying parser's get() method
    ) -> str | _T: ...
    # These are partially-applied version of the methods with the same names in
    # RawConfigParser; the stubs should be kept updated together
    @overload
    def getint(self, option: str, *, raw: bool = False, vars: _Section | None = None) -> int | None: ...
    @overload
    def getint(self, option: str, fallback: _T = ..., *, raw: bool = False, vars: _Section | None = None) -> int | _T: ...
    @overload
    def getfloat(self, option: str, *, raw: bool = False, vars: _Section | None = None) -> float | None: ...
    @overload
    def getfloat(self, option: str, fallback: _T = ..., *, raw: bool = False, vars: _Section | None = None) -> float | _T: ...
    @overload
    def getboolean(self, option: str, *, raw: bool = False, vars: _Section | None = None) -> bool | None: ...
    @overload
    def getboolean(self, option: str, fallback: _T = ..., *, raw: bool = False, vars: _Section | None = None) -> bool | _T: ...
    # SectionProxy can have arbitrary attributes when custom converters are used
    def __getattr__(self, key: str) -> Callable[..., Any]: ...

class ConverterMapping(MutableMapping[str, _ConverterCallback | None]):
    """Enables reuse of get*() methods between the parser and section proxies.

    If a parser class implements a getter directly, the value for the given
    key will be ``None``. The presence of the converter name here enables
    section proxies to find and use the implementation on the parser class.
    """

    GETTERCRE: ClassVar[Pattern[Any]]
    def __init__(self, parser: RawConfigParser) -> None: ...
    def __getitem__(self, key: str) -> _ConverterCallback: ...
    def __setitem__(self, key: str, value: _ConverterCallback | None) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...

class Error(Exception):
    """Base class for ConfigParser exceptions."""

    message: str
    def __init__(self, msg: str = "") -> None: ...

class NoSectionError(Error):
    """Raised when no section matches a requested option."""

    section: _SectionName
    def __init__(self, section: _SectionName) -> None: ...

class DuplicateSectionError(Error):
    """Raised when a section is repeated in an input source.

    Possible repetitions that raise this exception are: multiple creation
    using the API or in strict parsers when a section is found more than once
    in a single input file, string or dictionary.
    """

    section: _SectionName
    source: str | None
    lineno: int | None
    def __init__(self, section: _SectionName, source: str | None = None, lineno: int | None = None) -> None: ...

class DuplicateOptionError(Error):
    """Raised by strict parsers when an option is repeated in an input source.

    Current implementation raises this exception only when an option is found
    more than once in a single file, string or dictionary.
    """

    section: _SectionName
    option: str
    source: str | None
    lineno: int | None
    def __init__(self, section: _SectionName, option: str, source: str | None = None, lineno: int | None = None) -> None: ...

class NoOptionError(Error):
    """A requested option was not found."""

    section: _SectionName
    option: str
    def __init__(self, option: str, section: _SectionName) -> None: ...

class InterpolationError(Error):
    """Base class for interpolation-related exceptions."""

    section: _SectionName
    option: str
    def __init__(self, option: str, section: _SectionName, msg: str) -> None: ...

class InterpolationDepthError(InterpolationError):
    """Raised when substitutions are nested too deeply."""

    def __init__(self, option: str, section: _SectionName, rawval: object) -> None: ...

class InterpolationMissingOptionError(InterpolationError):
    """A string substitution required a setting which was not available."""

    reference: str
    def __init__(self, option: str, section: _SectionName, rawval: object, reference: str) -> None: ...

class InterpolationSyntaxError(InterpolationError):
    """Raised when the source text contains invalid syntax.

    Current implementation raises this exception when the source text into
    which substitutions are made does not conform to the required syntax.
    """

class ParsingError(Error):
    """Raised when a configuration file does not follow legal syntax."""

    source: str
    errors: list[tuple[int, str]]
    if sys.version_info >= (3, 13):
        def __init__(self, source: str, *args: object) -> None: ...
        def combine(self, others: Iterable[ParsingError]) -> ParsingError: ...
    elif sys.version_info >= (3, 12):
        def __init__(self, source: str) -> None: ...
    else:
        @overload
        def __init__(self, source: str) -> None: ...
        @overload
        @deprecated("The `filename` parameter removed in Python 3.12. Use `source` instead.")
        def __init__(self, source: None, filename: str | None) -> None: ...
        @overload
        @deprecated("The `filename` parameter removed in Python 3.12. Use `source` instead.")
        def __init__(self, source: None = None, *, filename: str | None) -> None: ...

    def append(self, lineno: int, line: str) -> None: ...

    if sys.version_info < (3, 12):
        @property
        @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `source` instead.")
        def filename(self) -> str:
            """Deprecated, use `source'."""

        @filename.setter
        @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `source` instead.")
        def filename(self, value: str) -> None: ...

class MissingSectionHeaderError(ParsingError):
    """Raised when a key-value pair is found before any section header."""

    lineno: int
    line: str
    def __init__(self, filename: str, lineno: int, line: str) -> None: ...

if sys.version_info >= (3, 13):
    class MultilineContinuationError(ParsingError):
        """Raised when a key without value is followed by continuation line"""

        lineno: int
        line: str
        def __init__(self, filename: str, lineno: int, line: str) -> None: ...

if sys.version_info >= (3, 14):
    class UnnamedSectionDisabledError(Error):
        """Raised when an attempt to use UNNAMED_SECTION is made with the
        feature disabled.
        """

        msg: Final = "Support for UNNAMED_SECTION is disabled."
        def __init__(self) -> None: ...

    class InvalidWriteError(Error):
        """Raised when attempting to write data that the parser would read back differently.
        ex: writing a key which begins with the section header pattern would read back as a
        new section
        """
