"""
Logging package for Python. Based on PEP 282 and comments thereto in
comp.lang.python.

Copyright (C) 2001-2022 Vinay Sajip. All Rights Reserved.

To use, simply 'import logging' and log away!
"""

import sys
import threading
from _typeshed import StrPath, SupportsWrite
from collections.abc import Callable, Iterable, Mapping, MutableMapping, Sequence
from io import TextIOWrapper
from re import Pattern
from string import Template
from time import struct_time
from types import FrameType, GenericAlias, TracebackType
from typing import Any, ClassVar, Final, Generic, Literal, Protocol, TextIO, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias, deprecated

__all__ = [
    "BASIC_FORMAT",
    "BufferingFormatter",
    "CRITICAL",
    "DEBUG",
    "ERROR",
    "FATAL",
    "FileHandler",
    "Filter",
    "Formatter",
    "Handler",
    "INFO",
    "LogRecord",
    "Logger",
    "LoggerAdapter",
    "NOTSET",
    "NullHandler",
    "StreamHandler",
    "WARN",
    "WARNING",
    "addLevelName",
    "basicConfig",
    "captureWarnings",
    "critical",
    "debug",
    "disable",
    "error",
    "exception",
    "fatal",
    "getLevelName",
    "getLogger",
    "getLoggerClass",
    "info",
    "log",
    "makeLogRecord",
    "setLoggerClass",
    "shutdown",
    "warning",
    "getLogRecordFactory",
    "setLogRecordFactory",
    "lastResort",
    "raiseExceptions",
    "warn",
]

if sys.version_info >= (3, 11):
    __all__ += ["getLevelNamesMapping"]
if sys.version_info >= (3, 12):
    __all__ += ["getHandlerByName", "getHandlerNames"]

_SysExcInfoType: TypeAlias = tuple[type[BaseException], BaseException, TracebackType | None] | tuple[None, None, None]
_ExcInfoType: TypeAlias = None | bool | _SysExcInfoType | BaseException
_ArgsType: TypeAlias = tuple[object, ...] | Mapping[str, object]
_Level: TypeAlias = int | str
_FormatStyle: TypeAlias = Literal["%", "{", "$"]

if sys.version_info >= (3, 12):
    @type_check_only
    class _SupportsFilter(Protocol):
        def filter(self, record: LogRecord, /) -> bool | LogRecord: ...

    _FilterType: TypeAlias = Filter | Callable[[LogRecord], bool | LogRecord] | _SupportsFilter
else:
    @type_check_only
    class _SupportsFilter(Protocol):
        def filter(self, record: LogRecord, /) -> bool: ...

    _FilterType: TypeAlias = Filter | Callable[[LogRecord], bool] | _SupportsFilter

raiseExceptions: bool
logThreads: bool
logMultiprocessing: bool
logProcesses: bool
_srcfile: str | None

def currentframe() -> FrameType: ...

_levelToName: dict[int, str]
_nameToLevel: dict[str, int]

class Filterer:
    """
    A base class for loggers and handlers which allows them to share
    common code.
    """

    filters: list[_FilterType]
    def addFilter(self, filter: _FilterType) -> None:
        """
        Add the specified filter to this handler.
        """

    def removeFilter(self, filter: _FilterType) -> None:
        """
        Remove the specified filter from this handler.
        """
    if sys.version_info >= (3, 12):
        def filter(self, record: LogRecord) -> bool | LogRecord:
            """
            Determine if a record is loggable by consulting all the filters.

            The default is to allow the record to be logged; any filter can veto
            this by returning a false value.
            If a filter attached to a handler returns a log record instance,
            then that instance is used in place of the original log record in
            any further processing of the event by that handler.
            If a filter returns any other true value, the original log record
            is used in any further processing of the event by that handler.

            If none of the filters return false values, this method returns
            a log record.
            If any of the filters return a false value, this method returns
            a false value.

            .. versionchanged:: 3.2

               Allow filters to be just callables.

            .. versionchanged:: 3.12
               Allow filters to return a LogRecord instead of
               modifying it in place.
            """
    else:
        def filter(self, record: LogRecord) -> bool:
            """
            Determine if a record is loggable by consulting all the filters.

            The default is to allow the record to be logged; any filter can veto
            this and the record is then dropped. Returns a zero value if a record
            is to be dropped, else non-zero.

            .. versionchanged:: 3.2

               Allow filters to be just callables.
            """

class Manager:  # undocumented
    """
    There is [under normal circumstances] just one Manager instance, which
    holds the hierarchy of loggers.
    """

    root: RootLogger
    disable: int
    emittedNoHandlerWarning: bool
    loggerDict: dict[str, Logger | PlaceHolder]
    loggerClass: type[Logger] | None
    logRecordFactory: Callable[..., LogRecord] | None
    def __init__(self, rootnode: RootLogger) -> None:
        """
        Initialize the manager with the root node of the logger hierarchy.
        """

    def getLogger(self, name: str) -> Logger:
        """
        Get a logger with the specified name (channel name), creating it
        if it doesn't yet exist. This name is a dot-separated hierarchical
        name, such as "a", "a.b", "a.b.c" or similar.

        If a PlaceHolder existed for the specified name [i.e. the logger
        didn't exist but a child of it did], replace it with the created
        logger and fix up the parent/child references which pointed to the
        placeholder to now point to the logger.
        """

    def setLoggerClass(self, klass: type[Logger]) -> None:
        """
        Set the class to be used when instantiating a logger with this Manager.
        """

    def setLogRecordFactory(self, factory: Callable[..., LogRecord]) -> None:
        """
        Set the factory to be used when instantiating a log record with this
        Manager.
        """

class Logger(Filterer):
    """
    Instances of the Logger class represent a single logging channel. A
    "logging channel" indicates an area of an application. Exactly how an
    "area" is defined is up to the application developer. Since an
    application can have any number of areas, logging channels are identified
    by a unique string. Application areas can be nested (e.g. an area
    of "input processing" might include sub-areas "read CSV files", "read
    XLS files" and "read Gnumeric files"). To cater for this natural nesting,
    channel names are organized into a namespace hierarchy where levels are
    separated by periods, much like the Java or Python package namespace. So
    in the instance given above, channel names might be "input" for the upper
    level, and "input.csv", "input.xls" and "input.gnu" for the sub-levels.
    There is no arbitrary limit to the depth of nesting.
    """

    name: str  # undocumented
    level: int  # undocumented
    parent: Logger | None  # undocumented
    propagate: bool
    handlers: list[Handler]  # undocumented
    disabled: bool  # undocumented
    root: ClassVar[RootLogger]  # undocumented
    manager: Manager  # undocumented
    def __init__(self, name: str, level: _Level = 0) -> None:
        """
        Initialize the logger with a name and an optional level.
        """

    def setLevel(self, level: _Level) -> None:
        """
        Set the logging level of this logger.  level must be an int or a str.
        """

    def isEnabledFor(self, level: int) -> bool:
        """
        Is this logger enabled for level 'level'?
        """

    def getEffectiveLevel(self) -> int:
        """
        Get the effective level for this logger.

        Loop through this logger and its parents in the logger hierarchy,
        looking for a non-zero logging level. Return the first one found.
        """

    def getChild(self, suffix: str) -> Self:  # see python/typing#980
        """
        Get a logger which is a descendant to this one.

        This is a convenience method, such that

        logging.getLogger('abc').getChild('def.ghi')

        is the same as

        logging.getLogger('abc.def.ghi')

        It's useful, for example, when the parent logger is named using
        __name__ rather than a literal string.
        """
    if sys.version_info >= (3, 12):
        def getChildren(self) -> set[Logger]: ...

    def debug(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with severity 'DEBUG'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.debug("Houston, we have a %s", "thorny problem", exc_info=True)
        """

    def info(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with severity 'INFO'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.info("Houston, we have a %s", "notable problem", exc_info=True)
        """

    def warning(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with severity 'WARNING'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.warning("Houston, we have a %s", "bit of a problem", exc_info=True)
        """

    @deprecated("Deprecated since Python 3.3. Use `Logger.warning()` instead.")
    def warn(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None: ...
    def error(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with severity 'ERROR'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.error("Houston, we have a %s", "major problem", exc_info=True)
        """

    def exception(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = True,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Convenience method for logging an ERROR with exception information.
        """

    def critical(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with severity 'CRITICAL'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.critical("Houston, we have a %s", "major disaster", exc_info=True)
        """

    def log(
        self,
        level: int,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
    ) -> None:
        """
        Log 'msg % args' with the integer severity 'level'.

        To pass exception information, use the keyword argument exc_info with
        a true value, e.g.

        logger.log(level, "We have a %s", "mysterious problem", exc_info=True)
        """

    def _log(
        self,
        level: int,
        msg: object,
        args: _ArgsType,
        exc_info: _ExcInfoType | None = None,
        extra: Mapping[str, object] | None = None,
        stack_info: bool = False,
        stacklevel: int = 1,
    ) -> None:  # undocumented
        """
        Low-level logging routine which creates a LogRecord and then calls
        all the handlers of this logger to handle the record.
        """
    fatal = critical
    def addHandler(self, hdlr: Handler) -> None:
        """
        Add the specified handler to this logger.
        """

    def removeHandler(self, hdlr: Handler) -> None:
        """
        Remove the specified handler from this logger.
        """

    def findCaller(self, stack_info: bool = False, stacklevel: int = 1) -> tuple[str, int, str, str | None]:
        """
        Find the stack frame of the caller so that we can note the source
        file name, line number and function name.
        """

    def handle(self, record: LogRecord) -> None:
        """
        Call the handlers for the specified record.

        This method is used for unpickled records received from a socket, as
        well as those created locally. Logger-level filtering is applied.
        """

    def makeRecord(
        self,
        name: str,
        level: int,
        fn: str,
        lno: int,
        msg: object,
        args: _ArgsType,
        exc_info: _SysExcInfoType | None,
        func: str | None = None,
        extra: Mapping[str, object] | None = None,
        sinfo: str | None = None,
    ) -> LogRecord:
        """
        A factory method which can be overridden in subclasses to create
        specialized LogRecords.
        """

    def hasHandlers(self) -> bool:
        """
        See if this logger has any handlers configured.

        Loop through all handlers for this logger and its parents in the
        logger hierarchy. Return True if a handler was found, else False.
        Stop searching up the hierarchy whenever a logger with the "propagate"
        attribute set to zero is found - that will be the last logger which
        is checked for the existence of handlers.
        """

    def callHandlers(self, record: LogRecord) -> None:  # undocumented
        """
        Pass a record to all relevant handlers.

        Loop through all handlers for this logger and its parents in the
        logger hierarchy. If no handler was found, output a one-off error
        message to sys.stderr. Stop searching up the hierarchy whenever a
        logger with the "propagate" attribute set to zero is found - that
        will be the last logger whose handlers are called.
        """

CRITICAL: Final = 50
FATAL: Final = CRITICAL
ERROR: Final = 40
WARNING: Final = 30
WARN: Final = WARNING
INFO: Final = 20
DEBUG: Final = 10
NOTSET: Final = 0

class Handler(Filterer):
    """
    Handler instances dispatch logging events to specific destinations.

    The base handler class. Acts as a placeholder which defines the Handler
    interface. Handlers can optionally use Formatter instances to format
    records as desired. By default, no formatter is specified; in this case,
    the 'raw' message as determined by record.message is logged.
    """

    level: int  # undocumented
    formatter: Formatter | None  # undocumented
    lock: threading.Lock | None  # undocumented
    name: str | None  # undocumented
    def __init__(self, level: _Level = 0) -> None:
        """
        Initializes the instance - basically setting the formatter to None
        and the filter list to empty.
        """

    def get_name(self) -> str: ...  # undocumented
    def set_name(self, name: str) -> None: ...  # undocumented
    def createLock(self) -> None:
        """
        Acquire a thread lock for serializing access to the underlying I/O.
        """

    def acquire(self) -> None:
        """
        Acquire the I/O thread lock.
        """

    def release(self) -> None:
        """
        Release the I/O thread lock.
        """

    def setLevel(self, level: _Level) -> None:
        """
        Set the logging level of this handler.  level must be an int or a str.
        """

    def setFormatter(self, fmt: Formatter | None) -> None:
        """
        Set the formatter for this handler.
        """

    def flush(self) -> None:
        """
        Ensure all logging output has been flushed.

        This version does nothing and is intended to be implemented by
        subclasses.
        """

    def close(self) -> None:
        """
        Tidy up any resources used by the handler.

        This version removes the handler from an internal map of handlers,
        _handlers, which is used for handler lookup by name. Subclasses
        should ensure that this gets called from overridden close()
        methods.
        """

    def handle(self, record: LogRecord) -> bool:
        """
        Conditionally emit the specified logging record.

        Emission depends on filters which may have been added to the handler.
        Wrap the actual emission of the record with acquisition/release of
        the I/O thread lock.

        Returns an instance of the log record that was emitted
        if it passed all filters, otherwise a false value is returned.
        """

    def handleError(self, record: LogRecord) -> None:
        """
        Handle errors which occur during an emit() call.

        This method should be called from handlers when an exception is
        encountered during an emit() call. If raiseExceptions is false,
        exceptions get silently ignored. This is what is mostly wanted
        for a logging system - most users will not care about errors in
        the logging system, they are more interested in application errors.
        You could, however, replace this with a custom handler if you wish.
        The record which was being processed is passed in to this method.
        """

    def format(self, record: LogRecord) -> str:
        """
        Format the specified record.

        If a formatter is set, use it. Otherwise, use the default formatter
        for the module.
        """

    def emit(self, record: LogRecord) -> None:
        """
        Do whatever it takes to actually log the specified logging record.

        This version is intended to be implemented by subclasses and so
        raises a NotImplementedError.
        """

if sys.version_info >= (3, 12):
    def getHandlerByName(name: str) -> Handler | None:
        """
        Get a handler with the specified *name*, or None if there isn't one with
        that name.
        """

    def getHandlerNames() -> frozenset[str]:
        """
        Return all known handler names as an immutable set.
        """

class Formatter:
    """
    Formatter instances are used to convert a LogRecord to text.

    Formatters need to know how a LogRecord is constructed. They are
    responsible for converting a LogRecord to (usually) a string which can
    be interpreted by either a human or an external system. The base Formatter
    allows a formatting string to be specified. If none is supplied, the
    style-dependent default value, "%(message)s", "{message}", or
    "${message}", is used.

    The Formatter can be initialized with a format string which makes use of
    knowledge of the LogRecord attributes - e.g. the default value mentioned
    above makes use of the fact that the user's message and arguments are pre-
    formatted into a LogRecord's message attribute. Currently, the useful
    attributes in a LogRecord are described by:

    %(name)s            Name of the logger (logging channel)
    %(levelno)s         Numeric logging level for the message (DEBUG, INFO,
                        WARNING, ERROR, CRITICAL)
    %(levelname)s       Text logging level for the message ("DEBUG", "INFO",
                        "WARNING", "ERROR", "CRITICAL")
    %(pathname)s        Full pathname of the source file where the logging
                        call was issued (if available)
    %(filename)s        Filename portion of pathname
    %(module)s          Module (name portion of filename)
    %(lineno)d          Source line number where the logging call was issued
                        (if available)
    %(funcName)s        Function name
    %(created)f         Time when the LogRecord was created (time.time_ns() / 1e9
                        return value)
    %(asctime)s         Textual time when the LogRecord was created
    %(msecs)d           Millisecond portion of the creation time
    %(relativeCreated)d Time in milliseconds when the LogRecord was created,
                        relative to the time the logging module was loaded
                        (typically at application startup time)
    %(thread)d          Thread ID (if available)
    %(threadName)s      Thread name (if available)
    %(taskName)s        Task name (if available)
    %(process)d         Process ID (if available)
    %(processName)s     Process name (if available)
    %(message)s         The result of record.getMessage(), computed just as
                        the record is emitted
    """

    converter: Callable[[float | None], struct_time]
    _fmt: str | None  # undocumented
    datefmt: str | None  # undocumented
    _style: PercentStyle  # undocumented
    default_time_format: str
    default_msec_format: str | None

    if sys.version_info >= (3, 10):
        def __init__(
            self,
            fmt: str | None = None,
            datefmt: str | None = None,
            style: _FormatStyle = "%",
            validate: bool = True,
            *,
            defaults: Mapping[str, Any] | None = None,
        ) -> None:
            """
            Initialize the formatter with specified format strings.

            Initialize the formatter either with the specified format string, or a
            default as described above. Allow for specialized date formatting with
            the optional datefmt argument. If datefmt is omitted, you get an
            ISO8601-like (or RFC 3339-like) format.

            Use a style parameter of '%', '{' or '$' to specify that you want to
            use one of %-formatting, :meth:`str.format` (``{}``) formatting or
            :class:`string.Template` formatting in your format string.

            .. versionchanged:: 3.2
               Added the ``style`` parameter.
            """
    else:
        def __init__(
            self, fmt: str | None = None, datefmt: str | None = None, style: _FormatStyle = "%", validate: bool = True
        ) -> None:
            """
            Initialize the formatter with specified format strings.

            Initialize the formatter either with the specified format string, or a
            default as described above. Allow for specialized date formatting with
            the optional datefmt argument. If datefmt is omitted, you get an
            ISO8601-like (or RFC 3339-like) format.

            Use a style parameter of '%', '{' or '$' to specify that you want to
            use one of %-formatting, :meth:`str.format` (``{}``) formatting or
            :class:`string.Template` formatting in your format string.

            .. versionchanged:: 3.2
               Added the ``style`` parameter.
            """

    def format(self, record: LogRecord) -> str:
        """
        Format the specified record as text.

        The record's attribute dictionary is used as the operand to a
        string formatting operation which yields the returned string.
        Before formatting the dictionary, a couple of preparatory steps
        are carried out. The message attribute of the record is computed
        using LogRecord.getMessage(). If the formatting string uses the
        time (as determined by a call to usesTime(), formatTime() is
        called to format the event time. If there is exception information,
        it is formatted using formatException() and appended to the message.
        """

    def formatTime(self, record: LogRecord, datefmt: str | None = None) -> str:
        """
        Return the creation time of the specified LogRecord as formatted text.

        This method should be called from format() by a formatter which
        wants to make use of a formatted time. This method can be overridden
        in formatters to provide for any specific requirement, but the
        basic behaviour is as follows: if datefmt (a string) is specified,
        it is used with time.strftime() to format the creation time of the
        record. Otherwise, an ISO8601-like (or RFC 3339-like) format is used.
        The resulting string is returned. This function uses a user-configurable
        function to convert the creation time to a tuple. By default,
        time.localtime() is used; to change this for a particular formatter
        instance, set the 'converter' attribute to a function with the same
        signature as time.localtime() or time.gmtime(). To change it for all
        formatters, for example if you want all logging times to be shown in GMT,
        set the 'converter' attribute in the Formatter class.
        """

    def formatException(self, ei: _SysExcInfoType) -> str:
        """
        Format and return the specified exception information as a string.

        This default implementation just uses
        traceback.print_exception()
        """

    def formatMessage(self, record: LogRecord) -> str: ...  # undocumented
    def formatStack(self, stack_info: str) -> str:
        """
        This method is provided as an extension point for specialized
        formatting of stack information.

        The input data is a string as returned from a call to
        :func:`traceback.print_stack`, but with the last trailing newline
        removed.

        The base implementation just returns the value passed in.
        """

    def usesTime(self) -> bool:  # undocumented
        """
        Check if the format uses the creation time of the record.
        """

class BufferingFormatter:
    """
    A formatter suitable for formatting a number of records.
    """

    linefmt: Formatter
    def __init__(self, linefmt: Formatter | None = None) -> None:
        """
        Optionally specify a formatter which will be used to format each
        individual record.
        """

    def formatHeader(self, records: Sequence[LogRecord]) -> str:
        """
        Return the header string for the specified records.
        """

    def formatFooter(self, records: Sequence[LogRecord]) -> str:
        """
        Return the footer string for the specified records.
        """

    def format(self, records: Sequence[LogRecord]) -> str:
        """
        Format the specified records and return the result as a string.
        """

class Filter:
    """
    Filter instances are used to perform arbitrary filtering of LogRecords.

    Loggers and Handlers can optionally use Filter instances to filter
    records as desired. The base filter class only allows events which are
    below a certain point in the logger hierarchy. For example, a filter
    initialized with "A.B" will allow events logged by loggers "A.B",
    "A.B.C", "A.B.C.D", "A.B.D" etc. but not "A.BB", "B.A.B" etc. If
    initialized with the empty string, all events are passed.
    """

    name: str  # undocumented
    nlen: int  # undocumented
    def __init__(self, name: str = "") -> None:
        """
        Initialize a filter.

        Initialize with the name of the logger which, together with its
        children, will have its events allowed through the filter. If no
        name is specified, allow every event.
        """
    if sys.version_info >= (3, 12):
        def filter(self, record: LogRecord) -> bool | LogRecord:
            """
            Determine if the specified record is to be logged.

            Returns True if the record should be logged, or False otherwise.
            If deemed appropriate, the record may be modified in-place.
            """
    else:
        def filter(self, record: LogRecord) -> bool:
            """
            Determine if the specified record is to be logged.

            Returns True if the record should be logged, or False otherwise.
            If deemed appropriate, the record may be modified in-place.
            """

class LogRecord:
    """
    A LogRecord instance represents an event being logged.

    LogRecord instances are created every time something is logged. They
    contain all the information pertinent to the event being logged. The
    main information passed in is in msg and args, which are combined
    using str(msg) % args to create the message field of the record. The
    record also includes information such as when the record was created,
    the source line where the logging call was made, and any exception
    information to be logged.
    """

    # args can be set to None by logging.handlers.QueueHandler
    # (see https://bugs.python.org/issue44473)
    args: _ArgsType | None
    asctime: str
    created: float
    exc_info: _SysExcInfoType | None
    exc_text: str | None
    filename: str
    funcName: str
    levelname: str
    levelno: int
    lineno: int
    module: str
    msecs: float
    # Only created when logging.Formatter.format is called. See #6132.
    message: str
    msg: str | Any  # The runtime accepts any object, but will be a str in 99% of cases
    name: str
    pathname: str
    process: int | None
    processName: str | None
    relativeCreated: float
    stack_info: str | None
    thread: int | None
    threadName: str | None
    if sys.version_info >= (3, 12):
        taskName: str | None

    def __init__(
        self,
        name: str,
        level: int,
        pathname: str,
        lineno: int,
        msg: object,
        args: _ArgsType | None,
        exc_info: _SysExcInfoType | None,
        func: str | None = None,
        sinfo: str | None = None,
    ) -> None:
        """
        Initialize a logging record with interesting information.
        """

    def getMessage(self) -> str:
        """
        Return the message for this LogRecord.

        Return the message for this LogRecord after merging any user-supplied
        arguments with the message.
        """
    # Allows setting contextual information on LogRecord objects as per the docs, see #7833
    def __setattr__(self, name: str, value: Any, /) -> None: ...

_L = TypeVar("_L", bound=Logger | LoggerAdapter[Any])

class LoggerAdapter(Generic[_L]):
    """
    An adapter for loggers which makes it easier to specify contextual
    information in logging output.
    """

    logger: _L
    manager: Manager  # undocumented

    if sys.version_info >= (3, 13):
        def __init__(self, logger: _L, extra: Mapping[str, object] | None = None, merge_extra: bool = False) -> None:
            """
            Initialize the adapter with a logger and an optional dict-like object
            which provides contextual information. This constructor signature
            allows easy stacking of LoggerAdapters, if so desired.

            You can effectively pass keyword arguments as shown in the
            following example:

            adapter = LoggerAdapter(someLogger, dict(p1=v1, p2="v2"))

            By default, LoggerAdapter objects will drop the "extra" argument
            passed on the individual log calls to use its own instead.

            Initializing it with merge_extra=True will instead merge both
            maps when logging, the individual call extra taking precedence
            over the LoggerAdapter instance extra

            .. versionchanged:: 3.13
               The *merge_extra* argument was added.
            """
    elif sys.version_info >= (3, 10):
        def __init__(self, logger: _L, extra: Mapping[str, object] | None = None) -> None:
            """
            Initialize the adapter with a logger and a dict-like object which
            provides contextual information. This constructor signature allows
            easy stacking of LoggerAdapters, if so desired.

            You can effectively pass keyword arguments as shown in the
            following example:

            adapter = LoggerAdapter(someLogger, dict(p1=v1, p2="v2"))
            """
    else:
        def __init__(self, logger: _L, extra: Mapping[str, object]) -> None:
            """
            Initialize the adapter with a logger and a dict-like object which
            provides contextual information. This constructor signature allows
            easy stacking of LoggerAdapters, if so desired.

            You can effectively pass keyword arguments as shown in the
            following example:

            adapter = LoggerAdapter(someLogger, dict(p1=v1, p2="v2"))
            """
    if sys.version_info >= (3, 10):
        extra: Mapping[str, object] | None
    else:
        extra: Mapping[str, object]

    if sys.version_info >= (3, 13):
        merge_extra: bool

    def process(self, msg: Any, kwargs: MutableMapping[str, Any]) -> tuple[Any, MutableMapping[str, Any]]:
        """
        Process the logging message and keyword arguments passed in to
        a logging call to insert contextual information. You can either
        manipulate the message itself, the keyword args or both. Return
        the message and kwargs modified (or not) to suit your needs.

        Normally, you'll only need to override this one method in a
        LoggerAdapter subclass for your specific needs.
        """

    def debug(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate a debug call to the underlying logger.
        """

    def info(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate an info call to the underlying logger.
        """

    def warning(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate a warning call to the underlying logger.
        """

    @deprecated("Deprecated since Python 3.3. Use `LoggerAdapter.warning()` instead.")
    def warn(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None: ...
    def error(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate an error call to the underlying logger.
        """

    def exception(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = True,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate an exception call to the underlying logger.
        """

    def critical(
        self,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate a critical call to the underlying logger.
        """

    def log(
        self,
        level: int,
        msg: object,
        *args: object,
        exc_info: _ExcInfoType = None,
        stack_info: bool = False,
        stacklevel: int = 1,
        extra: Mapping[str, object] | None = None,
        **kwargs: object,
    ) -> None:
        """
        Delegate a log call to the underlying logger, after adding
        contextual information from this adapter instance.
        """

    def isEnabledFor(self, level: int) -> bool:
        """
        Is this logger enabled for level 'level'?
        """

    def getEffectiveLevel(self) -> int:
        """
        Get the effective level for the underlying logger.
        """

    def setLevel(self, level: _Level) -> None:
        """
        Set the specified level on the underlying logger.
        """

    def hasHandlers(self) -> bool:
        """
        See if the underlying logger has any handlers.
        """
    if sys.version_info >= (3, 11):
        def _log(
            self,
            level: int,
            msg: object,
            args: _ArgsType,
            *,
            exc_info: _ExcInfoType | None = None,
            extra: Mapping[str, object] | None = None,
            stack_info: bool = False,
        ) -> None:  # undocumented
            """
            Low-level log implementation, proxied to allow nested logger adapters.
            """
    else:
        def _log(
            self,
            level: int,
            msg: object,
            args: _ArgsType,
            exc_info: _ExcInfoType | None = None,
            extra: Mapping[str, object] | None = None,
            stack_info: bool = False,
        ) -> None:  # undocumented
            """
            Low-level log implementation, proxied to allow nested logger adapters.
            """

    @property
    def name(self) -> str: ...  # undocumented
    if sys.version_info >= (3, 11):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """Represent a PEP 585 generic type

            E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
            """

def getLogger(name: str | None = None) -> Logger:
    """
    Return a logger with the specified name, creating it if necessary.

    If no name is specified, return the root logger.
    """

def getLoggerClass() -> type[Logger]:
    """
    Return the class to be used when instantiating a logger.
    """

def getLogRecordFactory() -> Callable[..., LogRecord]:
    """
    Return the factory to be used when instantiating a log record.
    """

def debug(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'DEBUG' on the root logger. If the logger has
    no handlers, call basicConfig() to add a console handler with a pre-defined
    format.
    """

def info(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'INFO' on the root logger. If the logger has
    no handlers, call basicConfig() to add a console handler with a pre-defined
    format.
    """

def warning(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'WARNING' on the root logger. If the logger has
    no handlers, call basicConfig() to add a console handler with a pre-defined
    format.
    """

@deprecated("Deprecated since Python 3.3. Use `warning()` instead.")
def warn(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None: ...
def error(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'ERROR' on the root logger. If the logger has
    no handlers, call basicConfig() to add a console handler with a pre-defined
    format.
    """

def critical(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'CRITICAL' on the root logger. If the logger
    has no handlers, call basicConfig() to add a console handler with a
    pre-defined format.
    """

def exception(
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = True,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log a message with severity 'ERROR' on the root logger, with exception
    information. If the logger has no handlers, basicConfig() is called to add
    a console handler with a pre-defined format.
    """

def log(
    level: int,
    msg: object,
    *args: object,
    exc_info: _ExcInfoType = None,
    stack_info: bool = False,
    stacklevel: int = 1,
    extra: Mapping[str, object] | None = None,
) -> None:
    """
    Log 'msg % args' with the integer severity 'level' on the root logger. If
    the logger has no handlers, call basicConfig() to add a console handler
    with a pre-defined format.
    """

fatal = critical

def disable(level: int = 50) -> None:
    """
    Disable all logging calls of severity 'level' and below.
    """

def addLevelName(level: int, levelName: str) -> None:
    """
    Associate 'levelName' with 'level'.

    This is used when converting levels to text during message formatting.
    """

@overload
def getLevelName(level: int) -> str:
    """
    Return the textual or numeric representation of logging level 'level'.

    If the level is one of the predefined levels (CRITICAL, ERROR, WARNING,
    INFO, DEBUG) then you get the corresponding string. If you have
    associated levels with names using addLevelName then the name you have
    associated with 'level' is returned.

    If a numeric value corresponding to one of the defined levels is passed
    in, the corresponding string representation is returned.

    If a string representation of the level is passed in, the corresponding
    numeric value is returned.

    If no matching numeric or string value is passed in, the string
    'Level %s' % level is returned.
    """

@overload
@deprecated("The str -> int case is considered a mistake.")
def getLevelName(level: str) -> Any: ...

if sys.version_info >= (3, 11):
    def getLevelNamesMapping() -> dict[str, int]: ...

def makeLogRecord(dict: Mapping[str, object]) -> LogRecord:
    """
    Make a LogRecord whose attributes are defined by the specified dictionary,
    This function is useful for converting a logging event received over
    a socket connection (which is sent as a dictionary) into a LogRecord
    instance.
    """

def basicConfig(
    *,
    filename: StrPath | None = ...,
    filemode: str = ...,
    format: str = ...,
    datefmt: str | None = ...,
    style: _FormatStyle = ...,
    level: _Level | None = ...,
    stream: SupportsWrite[str] | None = ...,
    handlers: Iterable[Handler] | None = ...,
    force: bool | None = ...,
    encoding: str | None = ...,
    errors: str | None = ...,
) -> None:
    """
    Do basic configuration for the logging system.

    This function does nothing if the root logger already has handlers
    configured, unless the keyword argument *force* is set to ``True``.
    It is a convenience method intended for use by simple scripts
    to do one-shot configuration of the logging package.

    The default behaviour is to create a StreamHandler which writes to
    sys.stderr, set a formatter using the BASIC_FORMAT format string, and
    add the handler to the root logger.

    A number of optional keyword arguments may be specified, which can alter
    the default behaviour.

    filename  Specifies that a FileHandler be created, using the specified
              filename, rather than a StreamHandler.
    filemode  Specifies the mode to open the file, if filename is specified
              (if filemode is unspecified, it defaults to 'a').
    format    Use the specified format string for the handler.
    datefmt   Use the specified date/time format.
    style     If a format string is specified, use this to specify the
              type of format string (possible values '%', '{', '$', for
              %-formatting, :meth:`str.format` and :class:`string.Template`
              - defaults to '%').
    level     Set the root logger level to the specified level.
    stream    Use the specified stream to initialize the StreamHandler. Note
              that this argument is incompatible with 'filename' - if both
              are present, 'stream' is ignored.
    handlers  If specified, this should be an iterable of already created
              handlers, which will be added to the root logger. Any handler
              in the list which does not have a formatter assigned will be
              assigned the formatter created in this function.
    force     If this keyword  is specified as true, any existing handlers
              attached to the root logger are removed and closed, before
              carrying out the configuration as specified by the other
              arguments.
    encoding  If specified together with a filename, this encoding is passed to
              the created FileHandler, causing it to be used when the file is
              opened.
    errors    If specified together with a filename, this value is passed to the
              created FileHandler, causing it to be used when the file is
              opened in text mode. If not specified, the default value is
              `backslashreplace`.

    Note that you could specify a stream created using open(filename, mode)
    rather than passing the filename and mode in. However, it should be
    remembered that StreamHandler does not close its stream (since it may be
    using sys.stdout or sys.stderr), whereas FileHandler closes its stream
    when the handler is closed.

    .. versionchanged:: 3.2
       Added the ``style`` parameter.

    .. versionchanged:: 3.3
       Added the ``handlers`` parameter. A ``ValueError`` is now thrown for
       incompatible arguments (e.g. ``handlers`` specified together with
       ``filename``/``filemode``, or ``filename``/``filemode`` specified
       together with ``stream``, or ``handlers`` specified together with
       ``stream``.

    .. versionchanged:: 3.8
       Added the ``force`` parameter.

    .. versionchanged:: 3.9
       Added the ``encoding`` and ``errors`` parameters.
    """

def shutdown(handlerList: Sequence[Any] = ...) -> None:  # handlerList is undocumented
    """
    Perform any cleanup actions in the logging system (e.g. flushing
    buffers).

    Should be called at application exit.
    """

def setLoggerClass(klass: type[Logger]) -> None:
    """
    Set the class to be used when instantiating a logger. The class should
    define __init__() such that only a name argument is required, and the
    __init__() should call Logger.__init__()
    """

def captureWarnings(capture: bool) -> None:
    """
    If capture is true, redirect all warnings to the logging package.
    If capture is False, ensure that warnings are not redirected to logging
    but to their original destinations.
    """

def setLogRecordFactory(factory: Callable[..., LogRecord]) -> None:
    """
    Set the factory to be used when instantiating a log record.

    :param factory: A callable which will be called to instantiate
    a log record.
    """

lastResort: Handler | None

_StreamT = TypeVar("_StreamT", bound=SupportsWrite[str])

class StreamHandler(Handler, Generic[_StreamT]):
    """
    A handler class which writes logging records, appropriately formatted,
    to a stream. Note that this class does not close the stream, as
    sys.stdout or sys.stderr may be used.
    """

    stream: _StreamT  # undocumented
    terminator: str
    @overload
    def __init__(self: StreamHandler[TextIO], stream: None = None) -> None:
        """
        Initialize the handler.

        If stream is not specified, sys.stderr is used.
        """

    @overload
    def __init__(self: StreamHandler[_StreamT], stream: _StreamT) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]  #11780
    def setStream(self, stream: _StreamT) -> _StreamT | None:
        """
        Sets the StreamHandler's stream to the specified value,
        if it is different.

        Returns the old stream, if the stream was changed, or None
        if it wasn't.
        """
    if sys.version_info >= (3, 11):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """Represent a PEP 585 generic type

            E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
            """

class FileHandler(StreamHandler[TextIOWrapper]):
    """
    A handler class which writes formatted logging records to disk files.
    """

    baseFilename: str  # undocumented
    mode: str  # undocumented
    encoding: str | None  # undocumented
    delay: bool  # undocumented
    errors: str | None  # undocumented
    def __init__(
        self, filename: StrPath, mode: str = "a", encoding: str | None = None, delay: bool = False, errors: str | None = None
    ) -> None:
        """
        Open the specified file and use it as the stream for logging.
        """

    def _open(self) -> TextIOWrapper:  # undocumented
        """
        Open the current base file with the (original) mode and encoding.
        Return the resulting stream.
        """

class NullHandler(Handler):
    """
    This handler does nothing. It's intended to be used to avoid the
    "No handlers could be found for logger XXX" one-off warning. This is
    important for library code, which may contain code to log events. If a user
    of the library does not configure logging, the one-off warning might be
    produced; to avoid this, the library developer simply needs to instantiate
    a NullHandler and add it to the top-level logger of the library module or
    package.
    """

class PlaceHolder:  # undocumented
    """
    PlaceHolder instances are used in the Manager logger hierarchy to take
    the place of nodes for which no loggers have been defined. This class is
    intended for internal use only and not as part of the public API.
    """

    loggerMap: dict[Logger, None]
    def __init__(self, alogger: Logger) -> None:
        """
        Initialize with the specified logger being a child of this placeholder.
        """

    def append(self, alogger: Logger) -> None:
        """
        Add the specified logger as a child of this placeholder.
        """

# Below aren't in module docs but still visible

class RootLogger(Logger):
    """
    A root logger is not that different to any other logger, except that
    it must have a logging level and there is only one instance of it in
    the hierarchy.
    """

    def __init__(self, level: int) -> None:
        """
        Initialize the logger with the name "root".
        """

root: RootLogger

class PercentStyle:  # undocumented
    default_format: str
    asctime_format: str
    asctime_search: str
    validation_pattern: Pattern[str]
    _fmt: str
    if sys.version_info >= (3, 10):
        def __init__(self, fmt: str, *, defaults: Mapping[str, Any] | None = None) -> None: ...
    else:
        def __init__(self, fmt: str) -> None: ...

    def usesTime(self) -> bool: ...
    def validate(self) -> None:
        """Validate the input format, ensure it matches the correct style"""

    def format(self, record: Any) -> str: ...

class StrFormatStyle(PercentStyle):  # undocumented
    fmt_spec: Pattern[str]
    field_spec: Pattern[str]

class StringTemplateStyle(PercentStyle):  # undocumented
    _tpl: Template

_STYLES: Final[dict[str, tuple[PercentStyle, str]]]

BASIC_FORMAT: Final = "%(levelname)s:%(name)s:%(message)s"
