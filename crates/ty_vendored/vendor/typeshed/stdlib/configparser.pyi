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
    def before_get(self, parser: _Parser, section: _SectionName, option: str, value: str, defaults: _Section) -> str: ...
    def before_set(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...
    def before_read(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...
    def before_write(self, parser: _Parser, section: _SectionName, option: str, value: str) -> str: ...

class BasicInterpolation(Interpolation): ...
class ExtendedInterpolation(Interpolation): ...

if sys.version_info < (3, 13):
    @deprecated(
        "Deprecated since Python 3.2; removed in Python 3.13. Use `BasicInterpolation` or `ExtendedInterpolation` instead."
    )
    class LegacyInterpolation(Interpolation):
        def before_get(self, parser: _Parser, section: _SectionName, option: str, value: str, vars: _Section) -> str: ...

class RawConfigParser(_Parser):
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
    def sections(self) -> _SectionNameList: ...
    def add_section(self, section: _SectionName) -> None: ...
    def has_section(self, section: _SectionName) -> bool: ...
    def options(self, section: _SectionName) -> list[str]: ...
    def has_option(self, section: _SectionName, option: str) -> bool: ...
    def read(self, filenames: StrOrBytesPath | Iterable[StrOrBytesPath], encoding: str | None = None) -> list[str]: ...
    def read_file(self, f: Iterable[str], source: str | None = None) -> None: ...
    def read_string(self, string: str, source: str = "<string>") -> None: ...
    def read_dict(self, dictionary: Mapping[str, Mapping[str, Any]], source: str = "<dict>") -> None: ...
    if sys.version_info < (3, 12):
        @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `parser.read_file()` instead.")
        def readfp(self, fp: Iterable[str], filename: str | None = None) -> None: ...
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
    def get(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> str | MaybeNone: ...
    @overload
    def get(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T
    ) -> str | _T | MaybeNone: ...
    @overload
    def items(self, *, raw: bool = False, vars: _Section | None = None) -> ItemsView[str, SectionProxy]: ...
    @overload
    def items(self, section: _SectionName, raw: bool = False, vars: _Section | None = None) -> list[tuple[str, str]]: ...
    def set(self, section: _SectionName, option: str, value: str | None = None) -> None: ...
    def write(self, fp: SupportsWrite[str], space_around_delimiters: bool = True) -> None: ...
    def remove_option(self, section: _SectionName, option: str) -> bool: ...
    def remove_section(self, section: _SectionName) -> bool: ...
    def optionxform(self, optionstr: str) -> str: ...
    @property
    def converters(self) -> ConverterMapping: ...

class ConfigParser(RawConfigParser):
    # This is incompatible with MutableMapping so we ignore the type
    @overload  # type: ignore[override]
    def get(self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None) -> str: ...
    @overload
    def get(
        self, section: _SectionName, option: str, *, raw: bool = False, vars: _Section | None = None, fallback: _T
    ) -> str | _T: ...

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `ConfigParser` instead.")
    class SafeConfigParser(ConfigParser): ...

class SectionProxy(MutableMapping[str, str]):
    def __init__(self, parser: RawConfigParser, name: str) -> None: ...
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
    ) -> str | None: ...
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
    GETTERCRE: ClassVar[Pattern[Any]]
    def __init__(self, parser: RawConfigParser) -> None: ...
    def __getitem__(self, key: str) -> _ConverterCallback: ...
    def __setitem__(self, key: str, value: _ConverterCallback | None) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...

class Error(Exception):
    message: str
    def __init__(self, msg: str = "") -> None: ...

class NoSectionError(Error):
    section: _SectionName
    def __init__(self, section: _SectionName) -> None: ...

class DuplicateSectionError(Error):
    section: _SectionName
    source: str | None
    lineno: int | None
    def __init__(self, section: _SectionName, source: str | None = None, lineno: int | None = None) -> None: ...

class DuplicateOptionError(Error):
    section: _SectionName
    option: str
    source: str | None
    lineno: int | None
    def __init__(self, section: _SectionName, option: str, source: str | None = None, lineno: int | None = None) -> None: ...

class NoOptionError(Error):
    section: _SectionName
    option: str
    def __init__(self, option: str, section: _SectionName) -> None: ...

class InterpolationError(Error):
    section: _SectionName
    option: str
    def __init__(self, option: str, section: _SectionName, msg: str) -> None: ...

class InterpolationDepthError(InterpolationError):
    def __init__(self, option: str, section: _SectionName, rawval: object) -> None: ...

class InterpolationMissingOptionError(InterpolationError):
    reference: str
    def __init__(self, option: str, section: _SectionName, rawval: object, reference: str) -> None: ...

class InterpolationSyntaxError(InterpolationError): ...

class ParsingError(Error):
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
        def filename(self) -> str: ...
        @filename.setter
        @deprecated("Deprecated since Python 3.2; removed in Python 3.12. Use `source` instead.")
        def filename(self, value: str) -> None: ...

class MissingSectionHeaderError(ParsingError):
    lineno: int
    line: str
    def __init__(self, filename: str, lineno: int, line: str) -> None: ...

if sys.version_info >= (3, 13):
    class MultilineContinuationError(ParsingError):
        lineno: int
        line: str
        def __init__(self, filename: str, lineno: int, line: str) -> None: ...

if sys.version_info >= (3, 14):
    class UnnamedSectionDisabledError(Error):
        msg: Final = "Support for UNNAMED_SECTION is disabled."
        def __init__(self) -> None: ...

    class InvalidWriteError(Error): ...
