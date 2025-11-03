"""
Additional handlers for the logging package for Python. The core package is
based on PEP 282 and comments thereto in comp.lang.python.

Copyright (C) 2001-2021 Vinay Sajip. All Rights Reserved.

To use, simply 'import logging.handlers' and log away!
"""

import datetime
import http.client
import ssl
import sys
from _typeshed import ReadableBuffer, StrPath
from collections.abc import Callable
from logging import FileHandler, Handler, LogRecord
from re import Pattern
from socket import SocketKind, socket
from threading import Thread
from types import TracebackType
from typing import Any, ClassVar, Final, Protocol, TypeVar, type_check_only
from typing_extensions import Self

_T = TypeVar("_T")

DEFAULT_TCP_LOGGING_PORT: Final = 9020
DEFAULT_UDP_LOGGING_PORT: Final = 9021
DEFAULT_HTTP_LOGGING_PORT: Final = 9022
DEFAULT_SOAP_LOGGING_PORT: Final = 9023
SYSLOG_UDP_PORT: Final = 514
SYSLOG_TCP_PORT: Final = 514

class WatchedFileHandler(FileHandler):
    """
    A handler for logging to a file, which watches the file
    to see if it has changed while in use. This can happen because of
    usage of programs such as newsyslog and logrotate which perform
    log file rotation. This handler, intended for use under Unix,
    watches the file to see if it has changed since the last emit.
    (A file has changed if its device or inode have changed.)
    If it has changed, the old file stream is closed, and the file
    opened to get a new stream.

    This handler is not appropriate for use under Windows, because
    under Windows open files cannot be moved or renamed - logging
    opens the files with exclusive locks - and so there is no need
    for such a handler.

    This handler is based on a suggestion and patch by Chad J.
    Schroeder.
    """

    dev: int  # undocumented
    ino: int  # undocumented
    def __init__(
        self, filename: StrPath, mode: str = "a", encoding: str | None = None, delay: bool = False, errors: str | None = None
    ) -> None: ...
    def _statstream(self) -> None: ...  # undocumented
    def reopenIfNeeded(self) -> None:
        """
        Reopen log file if needed.

        Checks if the underlying file has changed, and if it
        has, close the old stream and reopen the file to get the
        current stream.
        """

class BaseRotatingHandler(FileHandler):
    """
    Base class for handlers that rotate log files at a certain point.
    Not meant to be instantiated directly.  Instead, use RotatingFileHandler
    or TimedRotatingFileHandler.
    """

    namer: Callable[[str], str] | None
    rotator: Callable[[str, str], None] | None
    def __init__(
        self, filename: StrPath, mode: str, encoding: str | None = None, delay: bool = False, errors: str | None = None
    ) -> None:
        """
        Use the specified filename for streamed logging
        """

    def rotation_filename(self, default_name: str) -> str:
        """
        Modify the filename of a log file when rotating.

        This is provided so that a custom filename can be provided.

        The default implementation calls the 'namer' attribute of the
        handler, if it's callable, passing the default name to
        it. If the attribute isn't callable (the default is None), the name
        is returned unchanged.

        :param default_name: The default name for the log file.
        """

    def rotate(self, source: str, dest: str) -> None:
        """
        When rotating, rotate the current log.

        The default implementation calls the 'rotator' attribute of the
        handler, if it's callable, passing the source and dest arguments to
        it. If the attribute isn't callable (the default is None), the source
        is simply renamed to the destination.

        :param source: The source filename. This is normally the base
                       filename, e.g. 'test.log'
        :param dest:   The destination filename. This is normally
                       what the source is rotated to, e.g. 'test.log.1'.
        """

class RotatingFileHandler(BaseRotatingHandler):
    """
    Handler for logging to a set of files, which switches from one file
    to the next when the current file reaches a certain size.
    """

    maxBytes: int  # undocumented
    backupCount: int  # undocumented
    def __init__(
        self,
        filename: StrPath,
        mode: str = "a",
        maxBytes: int = 0,
        backupCount: int = 0,
        encoding: str | None = None,
        delay: bool = False,
        errors: str | None = None,
    ) -> None:
        """
        Open the specified file and use it as the stream for logging.

        By default, the file grows indefinitely. You can specify particular
        values of maxBytes and backupCount to allow the file to rollover at
        a predetermined size.

        Rollover occurs whenever the current log file is nearly maxBytes in
        length. If backupCount is >= 1, the system will successively create
        new files with the same pathname as the base file, but with extensions
        ".1", ".2" etc. appended to it. For example, with a backupCount of 5
        and a base file name of "app.log", you would get "app.log",
        "app.log.1", "app.log.2", ... through to "app.log.5". The file being
        written to is always "app.log" - when it gets filled up, it is closed
        and renamed to "app.log.1", and if files "app.log.1", "app.log.2" etc.
        exist, then they are renamed to "app.log.2", "app.log.3" etc.
        respectively.

        If maxBytes is zero, rollover never occurs.
        """

    def doRollover(self) -> None:
        """
        Do a rollover, as described in __init__().
        """

    def shouldRollover(self, record: LogRecord) -> int:  # undocumented
        """
        Determine if rollover should occur.

        Basically, see if the supplied record would cause the file to exceed
        the size limit we have.
        """

class TimedRotatingFileHandler(BaseRotatingHandler):
    """
    Handler for logging to a file, rotating the log file at certain timed
    intervals.

    If backupCount is > 0, when rollover is done, no more than backupCount
    files are kept - the oldest ones are deleted.
    """

    when: str  # undocumented
    backupCount: int  # undocumented
    utc: bool  # undocumented
    atTime: datetime.time | None  # undocumented
    interval: int  # undocumented
    suffix: str  # undocumented
    dayOfWeek: int  # undocumented
    rolloverAt: int  # undocumented
    extMatch: Pattern[str]  # undocumented
    def __init__(
        self,
        filename: StrPath,
        when: str = "h",
        interval: int = 1,
        backupCount: int = 0,
        encoding: str | None = None,
        delay: bool = False,
        utc: bool = False,
        atTime: datetime.time | None = None,
        errors: str | None = None,
    ) -> None: ...
    def doRollover(self) -> None:
        """
        do a rollover; in this case, a date/time stamp is appended to the filename
        when the rollover happens.  However, you want the file to be named for the
        start of the interval, not the current time.  If there is a backup count,
        then we have to get a list of matching filenames, sort them and remove
        the one with the oldest suffix.
        """

    def shouldRollover(self, record: LogRecord) -> int:  # undocumented
        """
        Determine if rollover should occur.

        record is not used, as we are just comparing times, but it is needed so
        the method signatures are the same
        """

    def computeRollover(self, currentTime: int) -> int:  # undocumented
        """
        Work out the rollover time based on the specified time.
        """

    def getFilesToDelete(self) -> list[str]:  # undocumented
        """
        Determine the files to delete when rolling over.

        More specific than the earlier method, which just used glob.glob().
        """

class SocketHandler(Handler):
    """
    A handler class which writes logging records, in pickle format, to
    a streaming socket. The socket is kept open across logging calls.
    If the peer resets it, an attempt is made to reconnect on the next call.
    The pickle which is sent is that of the LogRecord's attribute dictionary
    (__dict__), so that the receiver does not need to have the logging module
    installed in order to process the logging event.

    To unpickle the record at the receiving end into a LogRecord, use the
    makeLogRecord function.
    """

    host: str  # undocumented
    port: int | None  # undocumented
    address: tuple[str, int] | str  # undocumented
    sock: socket | None  # undocumented
    closeOnError: bool  # undocumented
    retryTime: float | None  # undocumented
    retryStart: float  # undocumented
    retryFactor: float  # undocumented
    retryMax: float  # undocumented
    def __init__(self, host: str, port: int | None) -> None:
        """
        Initializes the handler with a specific host address and port.

        When the attribute *closeOnError* is set to True - if a socket error
        occurs, the socket is silently closed and then reopened on the next
        logging call.
        """

    def makeSocket(self, timeout: float = 1) -> socket:  # timeout is undocumented
        """
        A factory method which allows subclasses to define the precise
        type of socket they want.
        """

    def makePickle(self, record: LogRecord) -> bytes:
        """
        Pickles the record in binary format with a length prefix, and
        returns it ready for transmission across the socket.
        """

    def send(self, s: ReadableBuffer) -> None:
        """
        Send a pickled string to the socket.

        This function allows for partial sends which can happen when the
        network is busy.
        """

    def createSocket(self) -> None:
        """
        Try to create a socket, using an exponential backoff with
        a max retry time. Thanks to Robert Olson for the original patch
        (SF #815911) which has been slightly refactored.
        """

class DatagramHandler(SocketHandler):
    """
    A handler class which writes logging records, in pickle format, to
    a datagram socket.  The pickle which is sent is that of the LogRecord's
    attribute dictionary (__dict__), so that the receiver does not need to
    have the logging module installed in order to process the logging event.

    To unpickle the record at the receiving end into a LogRecord, use the
    makeLogRecord function.

    """

    def makeSocket(self) -> socket:  # type: ignore[override]
        """
        The factory method of SocketHandler is here overridden to create
        a UDP socket (SOCK_DGRAM).
        """

class SysLogHandler(Handler):
    """
    A handler class which sends formatted logging records to a syslog
    server. Based on Sam Rushing's syslog module:
    http://www.nightmare.com/squirl/python-ext/misc/syslog.py
    Contributed by Nicolas Untz (after which minor refactoring changes
    have been made).
    """

    LOG_EMERG: int
    LOG_ALERT: int
    LOG_CRIT: int
    LOG_ERR: int
    LOG_WARNING: int
    LOG_NOTICE: int
    LOG_INFO: int
    LOG_DEBUG: int

    LOG_KERN: int
    LOG_USER: int
    LOG_MAIL: int
    LOG_DAEMON: int
    LOG_AUTH: int
    LOG_SYSLOG: int
    LOG_LPR: int
    LOG_NEWS: int
    LOG_UUCP: int
    LOG_CRON: int
    LOG_AUTHPRIV: int
    LOG_FTP: int
    LOG_NTP: int
    LOG_SECURITY: int
    LOG_CONSOLE: int
    LOG_SOLCRON: int
    LOG_LOCAL0: int
    LOG_LOCAL1: int
    LOG_LOCAL2: int
    LOG_LOCAL3: int
    LOG_LOCAL4: int
    LOG_LOCAL5: int
    LOG_LOCAL6: int
    LOG_LOCAL7: int
    address: tuple[str, int] | str  # undocumented
    unixsocket: bool  # undocumented
    socktype: SocketKind  # undocumented
    ident: str  # undocumented
    append_nul: bool  # undocumented
    facility: int  # undocumented
    priority_names: ClassVar[dict[str, int]]  # undocumented
    facility_names: ClassVar[dict[str, int]]  # undocumented
    priority_map: ClassVar[dict[str, str]]  # undocumented
    if sys.version_info >= (3, 14):
        timeout: float | None
        def __init__(
            self,
            address: tuple[str, int] | str = ("localhost", 514),
            facility: str | int = 1,
            socktype: SocketKind | None = None,
            timeout: float | None = None,
        ) -> None:
            """
            Initialize a handler.

            If address is specified as a string, a UNIX socket is used. To log to a
            local syslogd, "SysLogHandler(address="/dev/log")" can be used.
            If facility is not specified, LOG_USER is used. If socktype is
            specified as socket.SOCK_DGRAM or socket.SOCK_STREAM, that specific
            socket type will be used. For Unix sockets, you can also specify a
            socktype of None, in which case socket.SOCK_DGRAM will be used, falling
            back to socket.SOCK_STREAM.
            """
    else:
        def __init__(
            self, address: tuple[str, int] | str = ("localhost", 514), facility: str | int = 1, socktype: SocketKind | None = None
        ) -> None:
            """
            Initialize a handler.

            If address is specified as a string, a UNIX socket is used. To log to a
            local syslogd, "SysLogHandler(address="/dev/log")" can be used.
            If facility is not specified, LOG_USER is used. If socktype is
            specified as socket.SOCK_DGRAM or socket.SOCK_STREAM, that specific
            socket type will be used. For Unix sockets, you can also specify a
            socktype of None, in which case socket.SOCK_DGRAM will be used, falling
            back to socket.SOCK_STREAM.
            """
    if sys.version_info >= (3, 11):
        def createSocket(self) -> None:
            """
            Try to create a socket and, if it's not a datagram socket, connect it
            to the other end. This method is called during handler initialization,
            but it's not regarded as an error if the other end isn't listening yet
            --- the method will be called again when emitting an event,
            if there is no socket at that point.
            """

    def encodePriority(self, facility: int | str, priority: int | str) -> int:
        """
        Encode the facility and priority. You can pass in strings or
        integers - if strings are passed, the facility_names and
        priority_names mapping dictionaries are used to convert them to
        integers.
        """

    def mapPriority(self, levelName: str) -> str:
        """
        Map a logging level name to a key in the priority_names map.
        This is useful in two scenarios: when custom levels are being
        used, and in the case where you can't do a straightforward
        mapping by lowercasing the logging level name because of locale-
        specific issues (see SF #1524081).
        """

class NTEventLogHandler(Handler):
    """
    A handler class which sends events to the NT Event Log. Adds a
    registry entry for the specified application name. If no dllname is
    provided, win32service.pyd (which contains some basic message
    placeholders) is used. Note that use of these placeholders will make
    your event logs big, as the entire message source is held in the log.
    If you want slimmer logs, you have to pass in the name of your own DLL
    which contains the message definitions you want to use in the event log.
    """

    def __init__(self, appname: str, dllname: str | None = None, logtype: str = "Application") -> None: ...
    def getEventCategory(self, record: LogRecord) -> int:
        """
        Return the event category for the record.

        Override this if you want to specify your own categories. This version
        returns 0.
        """
    # TODO: correct return value?
    def getEventType(self, record: LogRecord) -> int:
        """
        Return the event type for the record.

        Override this if you want to specify your own types. This version does
        a mapping using the handler's typemap attribute, which is set up in
        __init__() to a dictionary which contains mappings for DEBUG, INFO,
        WARNING, ERROR and CRITICAL. If you are using your own levels you will
        either need to override this method or place a suitable dictionary in
        the handler's typemap attribute.
        """

    def getMessageID(self, record: LogRecord) -> int:
        """
        Return the message ID for the event record. If you are using your
        own messages, you could do this by having the msg passed to the
        logger being an ID rather than a formatting string. Then, in here,
        you could use a dictionary lookup to get the message ID. This
        version returns 1, which is the base message ID in win32service.pyd.
        """

class SMTPHandler(Handler):
    """
    A handler class which sends an SMTP email for each logging event.
    """

    mailhost: str  # undocumented
    mailport: int | None  # undocumented
    username: str | None  # undocumented
    # password only exists as an attribute if passed credentials is a tuple or list
    password: str  # undocumented
    fromaddr: str  # undocumented
    toaddrs: list[str]  # undocumented
    subject: str  # undocumented
    secure: tuple[()] | tuple[str] | tuple[str, str] | None  # undocumented
    timeout: float  # undocumented
    def __init__(
        self,
        mailhost: str | tuple[str, int],
        fromaddr: str,
        toaddrs: str | list[str],
        subject: str,
        credentials: tuple[str, str] | None = None,
        secure: tuple[()] | tuple[str] | tuple[str, str] | None = None,
        timeout: float = 5.0,
    ) -> None:
        """
        Initialize the handler.

        Initialize the instance with the from and to addresses and subject
        line of the email. To specify a non-standard SMTP port, use the
        (host, port) tuple format for the mailhost argument. To specify
        authentication credentials, supply a (username, password) tuple
        for the credentials argument. To specify the use of a secure
        protocol (TLS), pass in a tuple for the secure argument. This will
        only be used when authentication credentials are supplied. The tuple
        will be either an empty tuple, or a single-value tuple with the name
        of a keyfile, or a 2-value tuple with the names of the keyfile and
        certificate file. (This tuple is passed to the
        `ssl.SSLContext.load_cert_chain` method).
        A timeout in seconds can be specified for the SMTP connection (the
        default is one second).
        """

    def getSubject(self, record: LogRecord) -> str:
        """
        Determine the subject for the email.

        If you want to specify a subject line which is record-dependent,
        override this method.
        """

class BufferingHandler(Handler):
    """
    A handler class which buffers logging records in memory. Whenever each
    record is added to the buffer, a check is made to see if the buffer should
    be flushed. If it should, then flush() is expected to do what's needed.
    """

    capacity: int  # undocumented
    buffer: list[LogRecord]  # undocumented
    def __init__(self, capacity: int) -> None:
        """
        Initialize the handler with the buffer size.
        """

    def shouldFlush(self, record: LogRecord) -> bool:
        """
        Should the handler flush its buffer?

        Returns true if the buffer is up to capacity. This method can be
        overridden to implement custom flushing strategies.
        """

class MemoryHandler(BufferingHandler):
    """
    A handler class which buffers logging records in memory, periodically
    flushing them to a target handler. Flushing occurs whenever the buffer
    is full, or when an event of a certain severity or greater is seen.
    """

    flushLevel: int  # undocumented
    target: Handler | None  # undocumented
    flushOnClose: bool  # undocumented
    def __init__(self, capacity: int, flushLevel: int = 40, target: Handler | None = None, flushOnClose: bool = True) -> None:
        """
        Initialize the handler with the buffer size, the level at which
        flushing should occur and an optional target.

        Note that without a target being set either here or via setTarget(),
        a MemoryHandler is no use to anyone!

        The ``flushOnClose`` argument is ``True`` for backward compatibility
        reasons - the old behaviour is that when the handler is closed, the
        buffer is flushed, even if the flush level hasn't been exceeded nor the
        capacity exceeded. To prevent this, set ``flushOnClose`` to ``False``.
        """

    def setTarget(self, target: Handler | None) -> None:
        """
        Set the target handler for this handler.
        """

class HTTPHandler(Handler):
    """
    A class which sends records to a web server, using either GET or
    POST semantics.
    """

    host: str  # undocumented
    url: str  # undocumented
    method: str  # undocumented
    secure: bool  # undocumented
    credentials: tuple[str, str] | None  # undocumented
    context: ssl.SSLContext | None  # undocumented
    def __init__(
        self,
        host: str,
        url: str,
        method: str = "GET",
        secure: bool = False,
        credentials: tuple[str, str] | None = None,
        context: ssl.SSLContext | None = None,
    ) -> None:
        """
        Initialize the instance with the host, the request URL, and the method
        ("GET" or "POST")
        """

    def mapLogRecord(self, record: LogRecord) -> dict[str, Any]:
        """
        Default implementation of mapping the log record into a dict
        that is sent as the CGI data. Overwrite in your class.
        Contributed by Franz Glasner.
        """

    def getConnection(self, host: str, secure: bool) -> http.client.HTTPConnection:  # undocumented
        """
        get a HTTP[S]Connection.

        Override when a custom connection is required, for example if
        there is a proxy.
        """

@type_check_only
class _QueueLike(Protocol[_T]):
    def get(self) -> _T: ...
    def put_nowait(self, item: _T, /) -> None: ...

class QueueHandler(Handler):
    """
    This handler sends events to a queue. Typically, it would be used together
    with a multiprocessing Queue to centralise logging to file in one process
    (in a multi-process application), so as to avoid file write contention
    between processes.

    This code is new in Python 3.2, but this class can be copy pasted into
    user code for use with earlier Python versions.
    """

    queue: _QueueLike[Any]
    def __init__(self, queue: _QueueLike[Any]) -> None:
        """
        Initialise an instance, using the passed queue.
        """

    def prepare(self, record: LogRecord) -> Any:
        """
        Prepare a record for queuing. The object returned by this method is
        enqueued.

        The base implementation formats the record to merge the message and
        arguments, and removes unpickleable items from the record in-place.
        Specifically, it overwrites the record's `msg` and
        `message` attributes with the merged message (obtained by
        calling the handler's `format` method), and sets the `args`,
        `exc_info` and `exc_text` attributes to None.

        You might want to override this method if you want to convert
        the record to a dict or JSON string, or send a modified copy
        of the record while leaving the original intact.
        """

    def enqueue(self, record: LogRecord) -> None:
        """
        Enqueue a record.

        The base implementation uses put_nowait. You may want to override
        this method if you want to use blocking, timeouts or custom queue
        implementations.
        """
    if sys.version_info >= (3, 12):
        listener: QueueListener | None

class QueueListener:
    """
    This class implements an internal threaded listener which watches for
    LogRecords being added to a queue, removes them and passes them to a
    list of handlers for processing.
    """

    handlers: tuple[Handler, ...]  # undocumented
    respect_handler_level: bool  # undocumented
    queue: _QueueLike[Any]  # undocumented
    _thread: Thread | None  # undocumented
    def __init__(self, queue: _QueueLike[Any], *handlers: Handler, respect_handler_level: bool = False) -> None:
        """
        Initialise an instance with the specified queue and
        handlers.
        """

    def dequeue(self, block: bool) -> LogRecord:
        """
        Dequeue a record and return it, optionally blocking.

        The base implementation uses get. You may want to override this method
        if you want to use timeouts or work with custom queue implementations.
        """

    def prepare(self, record: LogRecord) -> Any:
        """
        Prepare a record for handling.

        This method just returns the passed-in record. You may want to
        override this method if you need to do any custom marshalling or
        manipulation of the record before passing it to the handlers.
        """

    def start(self) -> None:
        """
        Start the listener.

        This starts up a background thread to monitor the queue for
        LogRecords to process.
        """

    def stop(self) -> None:
        """
        Stop the listener.

        This asks the thread to terminate, and then waits for it to do so.
        Note that if you don't call this before your application exits, there
        may be some records still left on the queue, which won't be processed.
        """

    def enqueue_sentinel(self) -> None:
        """
        This is used to enqueue the sentinel record.

        The base implementation uses put_nowait. You may want to override this
        method if you want to use timeouts or work with custom queue
        implementations.
        """

    def handle(self, record: LogRecord) -> None:
        """
        Handle a record.

        This just loops through the handlers offering them the record
        to handle.
        """
    if sys.version_info >= (3, 14):
        def __enter__(self) -> Self:
            """
            For use as a context manager. Starts the listener.
            """

        def __exit__(
            self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None
        ) -> None:
            """
            For use as a context manager. Stops the listener.
            """
