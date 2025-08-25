"""
Configuration functions for the logging package for Python. The core package
is based on PEP 282 and comments thereto in comp.lang.python, and influenced
by Apache's log4j system.

Copyright (C) 2001-2022 Vinay Sajip. All Rights Reserved.

To use, simply 'import logging' and log away!
"""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Callable, Hashable, Iterable, Mapping, Sequence
from configparser import RawConfigParser
from re import Pattern
from threading import Thread
from typing import IO, Any, Final, Literal, SupportsIndex, TypedDict, overload, type_check_only
from typing_extensions import Required, TypeAlias, disjoint_base

from . import Filter, Filterer, Formatter, Handler, Logger, _FilterType, _FormatStyle, _Level

DEFAULT_LOGGING_CONFIG_PORT: Final = 9030
RESET_ERROR: Final[int]  # undocumented
IDENTIFIER: Final[Pattern[str]]  # undocumented

if sys.version_info >= (3, 11):
    @type_check_only
    class _RootLoggerConfiguration(TypedDict, total=False):
        level: _Level
        filters: Sequence[str | _FilterType]
        handlers: Sequence[str]

else:
    @type_check_only
    class _RootLoggerConfiguration(TypedDict, total=False):
        level: _Level
        filters: Sequence[str]
        handlers: Sequence[str]

@type_check_only
class _LoggerConfiguration(_RootLoggerConfiguration, TypedDict, total=False):
    propagate: bool

_FormatterConfigurationTypedDict = TypedDict(
    "_FormatterConfigurationTypedDict", {"class": str, "format": str, "datefmt": str, "style": _FormatStyle}, total=False
)

@type_check_only
class _FilterConfigurationTypedDict(TypedDict):
    name: str

# Formatter and filter configs can specify custom factories via the special `()` key.
# If that is the case, the dictionary can contain any additional keys
# https://docs.python.org/3/library/logging.config.html#user-defined-objects
_FormatterConfiguration: TypeAlias = _FormatterConfigurationTypedDict | dict[str, Any]
_FilterConfiguration: TypeAlias = _FilterConfigurationTypedDict | dict[str, Any]
# Handler config can have additional keys even when not providing a custom factory so we just use `dict`.
_HandlerConfiguration: TypeAlias = dict[str, Any]

@type_check_only
class _DictConfigArgs(TypedDict, total=False):
    version: Required[Literal[1]]
    formatters: dict[str, _FormatterConfiguration]
    filters: dict[str, _FilterConfiguration]
    handlers: dict[str, _HandlerConfiguration]
    loggers: dict[str, _LoggerConfiguration]
    root: _RootLoggerConfiguration
    incremental: bool
    disable_existing_loggers: bool

# Accept dict[str, Any] to avoid false positives if called with a dict
# type, since dict types are not compatible with TypedDicts.
#
# Also accept a TypedDict type, to allow callers to use TypedDict
# types, and for somewhat stricter type checking of dict literals.
def dictConfig(config: _DictConfigArgs | dict[str, Any]) -> None:
    """Configure logging using a dictionary."""

if sys.version_info >= (3, 10):
    def fileConfig(
        fname: StrOrBytesPath | IO[str] | RawConfigParser,
        defaults: Mapping[str, str] | None = None,
        disable_existing_loggers: bool = True,
        encoding: str | None = None,
    ) -> None:
        """
        Read the logging configuration from a ConfigParser-format file.

        This can be called several times from an application, allowing an end user
        the ability to select from various pre-canned configurations (if the
        developer provides a mechanism to present the choices and load the chosen
        configuration).
        """

else:
    def fileConfig(
        fname: StrOrBytesPath | IO[str] | RawConfigParser,
        defaults: Mapping[str, str] | None = None,
        disable_existing_loggers: bool = True,
    ) -> None:
        """
        Read the logging configuration from a ConfigParser-format file.

        This can be called several times from an application, allowing an end user
        the ability to select from various pre-canned configurations (if the
        developer provides a mechanism to present the choices and load the chosen
        configuration).
        """

def valid_ident(s: str) -> Literal[True]: ...  # undocumented
def listen(port: int = 9030, verify: Callable[[bytes], bytes | None] | None = None) -> Thread:
    """
    Start up a socket server on the specified port, and listen for new
    configurations.

    These will be sent as a file suitable for processing by fileConfig().
    Returns a Thread object on which you can call start() to start the server,
    and which you can join() when appropriate. To stop the server, call
    stopListening().

    Use the ``verify`` argument to verify any bytes received across the wire
    from a client. If specified, it should be a callable which receives a
    single argument - the bytes of configuration data received across the
    network - and it should return either ``None``, to indicate that the
    passed in bytes could not be verified and should be discarded, or a
    byte string which is then passed to the configuration machinery as
    normal. Note that you can return transformed bytes, e.g. by decrypting
    the bytes passed in.
    """

def stopListening() -> None:
    """
    Stop the listening server which was created with a call to listen().
    """

class ConvertingMixin:  # undocumented
    """For ConvertingXXX's, this mixin class provides common functions"""

    def convert_with_key(self, key: Any, value: Any, replace: bool = True) -> Any: ...
    def convert(self, value: Any) -> Any: ...

class ConvertingDict(dict[Hashable, Any], ConvertingMixin):  # undocumented
    """A converting dictionary wrapper."""

    def __getitem__(self, key: Hashable) -> Any: ...
    def get(self, key: Hashable, default: Any = None) -> Any: ...
    def pop(self, key: Hashable, default: Any = None) -> Any: ...

class ConvertingList(list[Any], ConvertingMixin):  # undocumented
    """A converting list wrapper."""

    @overload
    def __getitem__(self, key: SupportsIndex) -> Any: ...
    @overload
    def __getitem__(self, key: slice) -> Any: ...
    def pop(self, idx: SupportsIndex = -1) -> Any: ...

if sys.version_info >= (3, 12):
    class ConvertingTuple(tuple[Any, ...], ConvertingMixin):  # undocumented
        """A converting tuple wrapper."""

        @overload
        def __getitem__(self, key: SupportsIndex) -> Any: ...
        @overload
        def __getitem__(self, key: slice) -> Any: ...

else:
    @disjoint_base
    class ConvertingTuple(tuple[Any, ...], ConvertingMixin):  # undocumented
        """A converting tuple wrapper."""

        @overload
        def __getitem__(self, key: SupportsIndex) -> Any: ...
        @overload
        def __getitem__(self, key: slice) -> Any: ...

class BaseConfigurator:
    """
    The configurator base class which defines some useful defaults.
    """

    CONVERT_PATTERN: Pattern[str]
    WORD_PATTERN: Pattern[str]
    DOT_PATTERN: Pattern[str]
    INDEX_PATTERN: Pattern[str]
    DIGIT_PATTERN: Pattern[str]
    value_converters: dict[str, str]
    importer: Callable[..., Any]

    config: dict[str, Any]  # undocumented

    def __init__(self, config: _DictConfigArgs | dict[str, Any]) -> None: ...
    def resolve(self, s: str) -> Any:
        """
        Resolve strings to objects using standard import and attribute
        syntax.
        """

    def ext_convert(self, value: str) -> Any:
        """Default converter for the ext:// protocol."""

    def cfg_convert(self, value: str) -> Any:
        """Default converter for the cfg:// protocol."""

    def convert(self, value: Any) -> Any:
        """
        Convert values to an appropriate type. dicts, lists and tuples are
        replaced by their converting alternatives. Strings are checked to
        see if they have a conversion format and are converted if they do.
        """

    def configure_custom(self, config: dict[str, Any]) -> Any:
        """Configure an object with a user-supplied factory."""

    def as_tuple(self, value: list[Any] | tuple[Any, ...]) -> tuple[Any, ...]:
        """Utility function which converts lists to tuples."""

class DictConfigurator(BaseConfigurator):
    """
    Configure logging using a dictionary-like object to describe the
    configuration.
    """

    def configure(self) -> None:  # undocumented
        """Do the configuration."""

    def configure_formatter(self, config: _FormatterConfiguration) -> Formatter | Any:  # undocumented
        """Configure a formatter from a dictionary."""

    def configure_filter(self, config: _FilterConfiguration) -> Filter | Any:  # undocumented
        """Configure a filter from a dictionary."""

    def add_filters(self, filterer: Filterer, filters: Iterable[_FilterType]) -> None:  # undocumented
        """Add filters to a filterer from a list of names."""

    def configure_handler(self, config: _HandlerConfiguration) -> Handler | Any:  # undocumented
        """Configure a handler from a dictionary."""

    def add_handlers(self, logger: Logger, handlers: Iterable[str]) -> None:  # undocumented
        """Add handlers to a logger from a list of names."""

    def common_logger_config(
        self, logger: Logger, config: _LoggerConfiguration, incremental: bool = False
    ) -> None:  # undocumented
        """
        Perform configuration which is common to root and non-root loggers.
        """

    def configure_logger(self, name: str, config: _LoggerConfiguration, incremental: bool = False) -> None:  # undocumented
        """Configure a non-root logger from a dictionary."""

    def configure_root(self, config: _LoggerConfiguration, incremental: bool = False) -> None:  # undocumented
        """Configure a root logger from a dictionary."""

dictConfigClass = DictConfigurator
