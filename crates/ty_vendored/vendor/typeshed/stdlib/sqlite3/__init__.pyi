"""
The sqlite3 extension module provides a DB-API 2.0 (PEP 249) compliant
interface to the SQLite library, and requires SQLite 3.15.2 or newer.

To use the module, start by creating a database Connection object:

    import sqlite3
    cx = sqlite3.connect("test.db")  # test.db will be created or opened

The special path name ":memory:" can be provided to connect to a transient
in-memory database:

    cx = sqlite3.connect(":memory:")  # connect to a database in RAM

Once a connection has been established, create a Cursor object and call
its execute() method to perform SQL queries:

    cu = cx.cursor()

    # create a table
    cu.execute("create table lang(name, first_appeared)")

    # insert values into a table
    cu.execute("insert into lang values (?, ?)", ("C", 1972))

    # execute a query and iterate over the result
    for row in cu.execute("select * from lang"):
        print(row)

    cx.close()

The sqlite3 module is written by Gerhard Häring <gh@ghaering.de>.
"""

import sys
from _typeshed import MaybeNone, ReadableBuffer, StrOrBytesPath, SupportsLenAndGetItem, Unused
from collections.abc import Callable, Generator, Iterable, Iterator, Mapping, Sequence
from sqlite3.dbapi2 import (
    PARSE_COLNAMES as PARSE_COLNAMES,
    PARSE_DECLTYPES as PARSE_DECLTYPES,
    SQLITE_ALTER_TABLE as SQLITE_ALTER_TABLE,
    SQLITE_ANALYZE as SQLITE_ANALYZE,
    SQLITE_ATTACH as SQLITE_ATTACH,
    SQLITE_CREATE_INDEX as SQLITE_CREATE_INDEX,
    SQLITE_CREATE_TABLE as SQLITE_CREATE_TABLE,
    SQLITE_CREATE_TEMP_INDEX as SQLITE_CREATE_TEMP_INDEX,
    SQLITE_CREATE_TEMP_TABLE as SQLITE_CREATE_TEMP_TABLE,
    SQLITE_CREATE_TEMP_TRIGGER as SQLITE_CREATE_TEMP_TRIGGER,
    SQLITE_CREATE_TEMP_VIEW as SQLITE_CREATE_TEMP_VIEW,
    SQLITE_CREATE_TRIGGER as SQLITE_CREATE_TRIGGER,
    SQLITE_CREATE_VIEW as SQLITE_CREATE_VIEW,
    SQLITE_CREATE_VTABLE as SQLITE_CREATE_VTABLE,
    SQLITE_DELETE as SQLITE_DELETE,
    SQLITE_DENY as SQLITE_DENY,
    SQLITE_DETACH as SQLITE_DETACH,
    SQLITE_DONE as SQLITE_DONE,
    SQLITE_DROP_INDEX as SQLITE_DROP_INDEX,
    SQLITE_DROP_TABLE as SQLITE_DROP_TABLE,
    SQLITE_DROP_TEMP_INDEX as SQLITE_DROP_TEMP_INDEX,
    SQLITE_DROP_TEMP_TABLE as SQLITE_DROP_TEMP_TABLE,
    SQLITE_DROP_TEMP_TRIGGER as SQLITE_DROP_TEMP_TRIGGER,
    SQLITE_DROP_TEMP_VIEW as SQLITE_DROP_TEMP_VIEW,
    SQLITE_DROP_TRIGGER as SQLITE_DROP_TRIGGER,
    SQLITE_DROP_VIEW as SQLITE_DROP_VIEW,
    SQLITE_DROP_VTABLE as SQLITE_DROP_VTABLE,
    SQLITE_FUNCTION as SQLITE_FUNCTION,
    SQLITE_IGNORE as SQLITE_IGNORE,
    SQLITE_INSERT as SQLITE_INSERT,
    SQLITE_OK as SQLITE_OK,
    SQLITE_PRAGMA as SQLITE_PRAGMA,
    SQLITE_READ as SQLITE_READ,
    SQLITE_RECURSIVE as SQLITE_RECURSIVE,
    SQLITE_REINDEX as SQLITE_REINDEX,
    SQLITE_SAVEPOINT as SQLITE_SAVEPOINT,
    SQLITE_SELECT as SQLITE_SELECT,
    SQLITE_TRANSACTION as SQLITE_TRANSACTION,
    SQLITE_UPDATE as SQLITE_UPDATE,
    Binary as Binary,
    Date as Date,
    DateFromTicks as DateFromTicks,
    Time as Time,
    TimeFromTicks as TimeFromTicks,
    TimestampFromTicks as TimestampFromTicks,
    adapt as adapt,
    adapters as adapters,
    apilevel as apilevel,
    complete_statement as complete_statement,
    connect as connect,
    converters as converters,
    enable_callback_tracebacks as enable_callback_tracebacks,
    paramstyle as paramstyle,
    register_adapter as register_adapter,
    register_converter as register_converter,
    sqlite_version as sqlite_version,
    sqlite_version_info as sqlite_version_info,
    threadsafety as threadsafety,
)
from types import TracebackType
from typing import Any, Literal, Protocol, SupportsIndex, TypeVar, final, overload, type_check_only
from typing_extensions import Self, TypeAlias, disjoint_base

if sys.version_info < (3, 14):
    from sqlite3.dbapi2 import version_info as version_info

if sys.version_info >= (3, 12):
    from sqlite3.dbapi2 import (
        LEGACY_TRANSACTION_CONTROL as LEGACY_TRANSACTION_CONTROL,
        SQLITE_DBCONFIG_DEFENSIVE as SQLITE_DBCONFIG_DEFENSIVE,
        SQLITE_DBCONFIG_DQS_DDL as SQLITE_DBCONFIG_DQS_DDL,
        SQLITE_DBCONFIG_DQS_DML as SQLITE_DBCONFIG_DQS_DML,
        SQLITE_DBCONFIG_ENABLE_FKEY as SQLITE_DBCONFIG_ENABLE_FKEY,
        SQLITE_DBCONFIG_ENABLE_FTS3_TOKENIZER as SQLITE_DBCONFIG_ENABLE_FTS3_TOKENIZER,
        SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION as SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
        SQLITE_DBCONFIG_ENABLE_QPSG as SQLITE_DBCONFIG_ENABLE_QPSG,
        SQLITE_DBCONFIG_ENABLE_TRIGGER as SQLITE_DBCONFIG_ENABLE_TRIGGER,
        SQLITE_DBCONFIG_ENABLE_VIEW as SQLITE_DBCONFIG_ENABLE_VIEW,
        SQLITE_DBCONFIG_LEGACY_ALTER_TABLE as SQLITE_DBCONFIG_LEGACY_ALTER_TABLE,
        SQLITE_DBCONFIG_LEGACY_FILE_FORMAT as SQLITE_DBCONFIG_LEGACY_FILE_FORMAT,
        SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE as SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
        SQLITE_DBCONFIG_RESET_DATABASE as SQLITE_DBCONFIG_RESET_DATABASE,
        SQLITE_DBCONFIG_TRIGGER_EQP as SQLITE_DBCONFIG_TRIGGER_EQP,
        SQLITE_DBCONFIG_TRUSTED_SCHEMA as SQLITE_DBCONFIG_TRUSTED_SCHEMA,
        SQLITE_DBCONFIG_WRITABLE_SCHEMA as SQLITE_DBCONFIG_WRITABLE_SCHEMA,
    )

if sys.version_info >= (3, 11):
    from sqlite3.dbapi2 import (
        SQLITE_ABORT as SQLITE_ABORT,
        SQLITE_ABORT_ROLLBACK as SQLITE_ABORT_ROLLBACK,
        SQLITE_AUTH as SQLITE_AUTH,
        SQLITE_AUTH_USER as SQLITE_AUTH_USER,
        SQLITE_BUSY as SQLITE_BUSY,
        SQLITE_BUSY_RECOVERY as SQLITE_BUSY_RECOVERY,
        SQLITE_BUSY_SNAPSHOT as SQLITE_BUSY_SNAPSHOT,
        SQLITE_BUSY_TIMEOUT as SQLITE_BUSY_TIMEOUT,
        SQLITE_CANTOPEN as SQLITE_CANTOPEN,
        SQLITE_CANTOPEN_CONVPATH as SQLITE_CANTOPEN_CONVPATH,
        SQLITE_CANTOPEN_DIRTYWAL as SQLITE_CANTOPEN_DIRTYWAL,
        SQLITE_CANTOPEN_FULLPATH as SQLITE_CANTOPEN_FULLPATH,
        SQLITE_CANTOPEN_ISDIR as SQLITE_CANTOPEN_ISDIR,
        SQLITE_CANTOPEN_NOTEMPDIR as SQLITE_CANTOPEN_NOTEMPDIR,
        SQLITE_CANTOPEN_SYMLINK as SQLITE_CANTOPEN_SYMLINK,
        SQLITE_CONSTRAINT as SQLITE_CONSTRAINT,
        SQLITE_CONSTRAINT_CHECK as SQLITE_CONSTRAINT_CHECK,
        SQLITE_CONSTRAINT_COMMITHOOK as SQLITE_CONSTRAINT_COMMITHOOK,
        SQLITE_CONSTRAINT_FOREIGNKEY as SQLITE_CONSTRAINT_FOREIGNKEY,
        SQLITE_CONSTRAINT_FUNCTION as SQLITE_CONSTRAINT_FUNCTION,
        SQLITE_CONSTRAINT_NOTNULL as SQLITE_CONSTRAINT_NOTNULL,
        SQLITE_CONSTRAINT_PINNED as SQLITE_CONSTRAINT_PINNED,
        SQLITE_CONSTRAINT_PRIMARYKEY as SQLITE_CONSTRAINT_PRIMARYKEY,
        SQLITE_CONSTRAINT_ROWID as SQLITE_CONSTRAINT_ROWID,
        SQLITE_CONSTRAINT_TRIGGER as SQLITE_CONSTRAINT_TRIGGER,
        SQLITE_CONSTRAINT_UNIQUE as SQLITE_CONSTRAINT_UNIQUE,
        SQLITE_CONSTRAINT_VTAB as SQLITE_CONSTRAINT_VTAB,
        SQLITE_CORRUPT as SQLITE_CORRUPT,
        SQLITE_CORRUPT_INDEX as SQLITE_CORRUPT_INDEX,
        SQLITE_CORRUPT_SEQUENCE as SQLITE_CORRUPT_SEQUENCE,
        SQLITE_CORRUPT_VTAB as SQLITE_CORRUPT_VTAB,
        SQLITE_EMPTY as SQLITE_EMPTY,
        SQLITE_ERROR as SQLITE_ERROR,
        SQLITE_ERROR_MISSING_COLLSEQ as SQLITE_ERROR_MISSING_COLLSEQ,
        SQLITE_ERROR_RETRY as SQLITE_ERROR_RETRY,
        SQLITE_ERROR_SNAPSHOT as SQLITE_ERROR_SNAPSHOT,
        SQLITE_FORMAT as SQLITE_FORMAT,
        SQLITE_FULL as SQLITE_FULL,
        SQLITE_INTERNAL as SQLITE_INTERNAL,
        SQLITE_INTERRUPT as SQLITE_INTERRUPT,
        SQLITE_IOERR as SQLITE_IOERR,
        SQLITE_IOERR_ACCESS as SQLITE_IOERR_ACCESS,
        SQLITE_IOERR_AUTH as SQLITE_IOERR_AUTH,
        SQLITE_IOERR_BEGIN_ATOMIC as SQLITE_IOERR_BEGIN_ATOMIC,
        SQLITE_IOERR_BLOCKED as SQLITE_IOERR_BLOCKED,
        SQLITE_IOERR_CHECKRESERVEDLOCK as SQLITE_IOERR_CHECKRESERVEDLOCK,
        SQLITE_IOERR_CLOSE as SQLITE_IOERR_CLOSE,
        SQLITE_IOERR_COMMIT_ATOMIC as SQLITE_IOERR_COMMIT_ATOMIC,
        SQLITE_IOERR_CONVPATH as SQLITE_IOERR_CONVPATH,
        SQLITE_IOERR_CORRUPTFS as SQLITE_IOERR_CORRUPTFS,
        SQLITE_IOERR_DATA as SQLITE_IOERR_DATA,
        SQLITE_IOERR_DELETE as SQLITE_IOERR_DELETE,
        SQLITE_IOERR_DELETE_NOENT as SQLITE_IOERR_DELETE_NOENT,
        SQLITE_IOERR_DIR_CLOSE as SQLITE_IOERR_DIR_CLOSE,
        SQLITE_IOERR_DIR_FSYNC as SQLITE_IOERR_DIR_FSYNC,
        SQLITE_IOERR_FSTAT as SQLITE_IOERR_FSTAT,
        SQLITE_IOERR_FSYNC as SQLITE_IOERR_FSYNC,
        SQLITE_IOERR_GETTEMPPATH as SQLITE_IOERR_GETTEMPPATH,
        SQLITE_IOERR_LOCK as SQLITE_IOERR_LOCK,
        SQLITE_IOERR_MMAP as SQLITE_IOERR_MMAP,
        SQLITE_IOERR_NOMEM as SQLITE_IOERR_NOMEM,
        SQLITE_IOERR_RDLOCK as SQLITE_IOERR_RDLOCK,
        SQLITE_IOERR_READ as SQLITE_IOERR_READ,
        SQLITE_IOERR_ROLLBACK_ATOMIC as SQLITE_IOERR_ROLLBACK_ATOMIC,
        SQLITE_IOERR_SEEK as SQLITE_IOERR_SEEK,
        SQLITE_IOERR_SHMLOCK as SQLITE_IOERR_SHMLOCK,
        SQLITE_IOERR_SHMMAP as SQLITE_IOERR_SHMMAP,
        SQLITE_IOERR_SHMOPEN as SQLITE_IOERR_SHMOPEN,
        SQLITE_IOERR_SHMSIZE as SQLITE_IOERR_SHMSIZE,
        SQLITE_IOERR_SHORT_READ as SQLITE_IOERR_SHORT_READ,
        SQLITE_IOERR_TRUNCATE as SQLITE_IOERR_TRUNCATE,
        SQLITE_IOERR_UNLOCK as SQLITE_IOERR_UNLOCK,
        SQLITE_IOERR_VNODE as SQLITE_IOERR_VNODE,
        SQLITE_IOERR_WRITE as SQLITE_IOERR_WRITE,
        SQLITE_LIMIT_ATTACHED as SQLITE_LIMIT_ATTACHED,
        SQLITE_LIMIT_COLUMN as SQLITE_LIMIT_COLUMN,
        SQLITE_LIMIT_COMPOUND_SELECT as SQLITE_LIMIT_COMPOUND_SELECT,
        SQLITE_LIMIT_EXPR_DEPTH as SQLITE_LIMIT_EXPR_DEPTH,
        SQLITE_LIMIT_FUNCTION_ARG as SQLITE_LIMIT_FUNCTION_ARG,
        SQLITE_LIMIT_LENGTH as SQLITE_LIMIT_LENGTH,
        SQLITE_LIMIT_LIKE_PATTERN_LENGTH as SQLITE_LIMIT_LIKE_PATTERN_LENGTH,
        SQLITE_LIMIT_SQL_LENGTH as SQLITE_LIMIT_SQL_LENGTH,
        SQLITE_LIMIT_TRIGGER_DEPTH as SQLITE_LIMIT_TRIGGER_DEPTH,
        SQLITE_LIMIT_VARIABLE_NUMBER as SQLITE_LIMIT_VARIABLE_NUMBER,
        SQLITE_LIMIT_VDBE_OP as SQLITE_LIMIT_VDBE_OP,
        SQLITE_LIMIT_WORKER_THREADS as SQLITE_LIMIT_WORKER_THREADS,
        SQLITE_LOCKED as SQLITE_LOCKED,
        SQLITE_LOCKED_SHAREDCACHE as SQLITE_LOCKED_SHAREDCACHE,
        SQLITE_LOCKED_VTAB as SQLITE_LOCKED_VTAB,
        SQLITE_MISMATCH as SQLITE_MISMATCH,
        SQLITE_MISUSE as SQLITE_MISUSE,
        SQLITE_NOLFS as SQLITE_NOLFS,
        SQLITE_NOMEM as SQLITE_NOMEM,
        SQLITE_NOTADB as SQLITE_NOTADB,
        SQLITE_NOTFOUND as SQLITE_NOTFOUND,
        SQLITE_NOTICE as SQLITE_NOTICE,
        SQLITE_NOTICE_RECOVER_ROLLBACK as SQLITE_NOTICE_RECOVER_ROLLBACK,
        SQLITE_NOTICE_RECOVER_WAL as SQLITE_NOTICE_RECOVER_WAL,
        SQLITE_OK_LOAD_PERMANENTLY as SQLITE_OK_LOAD_PERMANENTLY,
        SQLITE_OK_SYMLINK as SQLITE_OK_SYMLINK,
        SQLITE_PERM as SQLITE_PERM,
        SQLITE_PROTOCOL as SQLITE_PROTOCOL,
        SQLITE_RANGE as SQLITE_RANGE,
        SQLITE_READONLY as SQLITE_READONLY,
        SQLITE_READONLY_CANTINIT as SQLITE_READONLY_CANTINIT,
        SQLITE_READONLY_CANTLOCK as SQLITE_READONLY_CANTLOCK,
        SQLITE_READONLY_DBMOVED as SQLITE_READONLY_DBMOVED,
        SQLITE_READONLY_DIRECTORY as SQLITE_READONLY_DIRECTORY,
        SQLITE_READONLY_RECOVERY as SQLITE_READONLY_RECOVERY,
        SQLITE_READONLY_ROLLBACK as SQLITE_READONLY_ROLLBACK,
        SQLITE_ROW as SQLITE_ROW,
        SQLITE_SCHEMA as SQLITE_SCHEMA,
        SQLITE_TOOBIG as SQLITE_TOOBIG,
        SQLITE_WARNING as SQLITE_WARNING,
        SQLITE_WARNING_AUTOINDEX as SQLITE_WARNING_AUTOINDEX,
    )

if sys.version_info < (3, 12):
    from sqlite3.dbapi2 import enable_shared_cache as enable_shared_cache, version as version

if sys.version_info < (3, 10):
    from sqlite3.dbapi2 import OptimizedUnicode as OptimizedUnicode

_CursorT = TypeVar("_CursorT", bound=Cursor)
_SqliteData: TypeAlias = str | ReadableBuffer | int | float | None
# Data that is passed through adapters can be of any type accepted by an adapter.
_AdaptedInputData: TypeAlias = _SqliteData | Any
# The Mapping must really be a dict, but making it invariant is too annoying.
_Parameters: TypeAlias = SupportsLenAndGetItem[_AdaptedInputData] | Mapping[str, _AdaptedInputData]
# Controls the legacy transaction handling mode of sqlite3.
_IsolationLevel: TypeAlias = Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None
_RowFactoryOptions: TypeAlias = type[Row] | Callable[[Cursor, Row], object] | None

@type_check_only
class _AnyParamWindowAggregateClass(Protocol):
    def step(self, *args: Any) -> object: ...
    def inverse(self, *args: Any) -> object: ...
    def value(self) -> _SqliteData: ...
    def finalize(self) -> _SqliteData: ...

@type_check_only
class _WindowAggregateClass(Protocol):
    step: Callable[..., object]
    inverse: Callable[..., object]
    def value(self) -> _SqliteData: ...
    def finalize(self) -> _SqliteData: ...

@type_check_only
class _AggregateProtocol(Protocol):
    def step(self, value: int, /) -> object: ...
    def finalize(self) -> int: ...

@type_check_only
class _SingleParamWindowAggregateClass(Protocol):
    def step(self, param: Any, /) -> object: ...
    def inverse(self, param: Any, /) -> object: ...
    def value(self) -> _SqliteData: ...
    def finalize(self) -> _SqliteData: ...

# These classes are implemented in the C module _sqlite3. At runtime, they're imported
# from there into sqlite3.dbapi2 and from that module to here. However, they
# consider themselves to live in the sqlite3.* namespace, so we'll define them here.

class Error(Exception):
    if sys.version_info >= (3, 11):
        sqlite_errorcode: int
        sqlite_errorname: str

class DatabaseError(Error): ...
class DataError(DatabaseError): ...
class IntegrityError(DatabaseError): ...
class InterfaceError(Error): ...
class InternalError(DatabaseError): ...
class NotSupportedError(DatabaseError): ...
class OperationalError(DatabaseError): ...
class ProgrammingError(DatabaseError): ...
class Warning(Exception): ...

@disjoint_base
class Connection:
    """SQLite database connection object."""

    @property
    def DataError(self) -> type[DataError]: ...
    @property
    def DatabaseError(self) -> type[DatabaseError]: ...
    @property
    def Error(self) -> type[Error]: ...
    @property
    def IntegrityError(self) -> type[IntegrityError]: ...
    @property
    def InterfaceError(self) -> type[InterfaceError]: ...
    @property
    def InternalError(self) -> type[InternalError]: ...
    @property
    def NotSupportedError(self) -> type[NotSupportedError]: ...
    @property
    def OperationalError(self) -> type[OperationalError]: ...
    @property
    def ProgrammingError(self) -> type[ProgrammingError]: ...
    @property
    def Warning(self) -> type[Warning]: ...
    @property
    def in_transaction(self) -> bool: ...
    isolation_level: _IsolationLevel
    @property
    def total_changes(self) -> int: ...
    if sys.version_info >= (3, 12):
        @property
        def autocommit(self) -> int: ...
        @autocommit.setter
        def autocommit(self, val: int) -> None: ...
    row_factory: _RowFactoryOptions
    text_factory: Any
    if sys.version_info >= (3, 12):
        def __init__(
            self,
            database: StrOrBytesPath,
            timeout: float = 5.0,
            detect_types: int = 0,
            isolation_level: _IsolationLevel = "DEFERRED",
            check_same_thread: bool = True,
            factory: type[Connection] | None = ...,
            cached_statements: int = 128,
            uri: bool = False,
            autocommit: bool = ...,
        ) -> None: ...
    else:
        def __init__(
            self,
            database: StrOrBytesPath,
            timeout: float = 5.0,
            detect_types: int = 0,
            isolation_level: _IsolationLevel = "DEFERRED",
            check_same_thread: bool = True,
            factory: type[Connection] | None = ...,
            cached_statements: int = 128,
            uri: bool = False,
        ) -> None: ...

    def close(self) -> None:
        """Close the database connection.

        Any pending transaction is not committed implicitly.
        """
    if sys.version_info >= (3, 11):
        def blobopen(self, table: str, column: str, row: int, /, *, readonly: bool = False, name: str = "main") -> Blob:
            """Open and return a BLOB object.

            table
              Table name.
            column
              Column name.
            rowid
              Row id.
            readonly
              Open the BLOB without write permissions.
            name
              Database name.
            """

    def commit(self) -> None:
        """Commit any pending transaction to the database.

        If there is no open transaction, this method is a no-op.
        """

    def create_aggregate(self, name: str, n_arg: int, aggregate_class: Callable[[], _AggregateProtocol]) -> None:
        """Creates a new aggregate.

        Note: Passing keyword arguments 'name', 'n_arg' and 'aggregate_class'
        to _sqlite3.Connection.create_aggregate() is deprecated. Parameters
        'name', 'n_arg' and 'aggregate_class' will become positional-only in
        Python 3.15.
        """
    if sys.version_info >= (3, 11):
        # num_params determines how many params will be passed to the aggregate class. We provide an overload
        # for the case where num_params = 1, which is expected to be the common case.
        @overload
        def create_window_function(
            self, name: str, num_params: Literal[1], aggregate_class: Callable[[], _SingleParamWindowAggregateClass] | None, /
        ) -> None:
            """Creates or redefines an aggregate window function. Non-standard.

            name
              The name of the SQL aggregate window function to be created or
              redefined.
            num_params
              The number of arguments the step and inverse methods takes.
            aggregate_class
              A class with step(), finalize(), value(), and inverse() methods.
              Set to None to clear the window function.
            """
        # And for num_params = -1, which means the aggregate must accept any number of parameters.
        @overload
        def create_window_function(
            self, name: str, num_params: Literal[-1], aggregate_class: Callable[[], _AnyParamWindowAggregateClass] | None, /
        ) -> None: ...
        @overload
        def create_window_function(
            self, name: str, num_params: int, aggregate_class: Callable[[], _WindowAggregateClass] | None, /
        ) -> None: ...

    def create_collation(self, name: str, callback: Callable[[str, str], int | SupportsIndex] | None, /) -> None:
        """Creates a collation function."""

    def create_function(
        self, name: str, narg: int, func: Callable[..., _SqliteData] | None, *, deterministic: bool = False
    ) -> None:
        """Creates a new function.

        Note: Passing keyword arguments 'name', 'narg' and 'func' to
        _sqlite3.Connection.create_function() is deprecated. Parameters
        'name', 'narg' and 'func' will become positional-only in Python 3.15.
        """

    @overload
    def cursor(self, factory: None = None) -> Cursor:
        """Return a cursor for the connection."""

    @overload
    def cursor(self, factory: Callable[[Connection], _CursorT]) -> _CursorT: ...
    def execute(self, sql: str, parameters: _Parameters = ..., /) -> Cursor:
        """Executes an SQL statement."""

    def executemany(self, sql: str, parameters: Iterable[_Parameters], /) -> Cursor:
        """Repeatedly executes an SQL statement."""

    def executescript(self, sql_script: str, /) -> Cursor:
        """Executes multiple SQL statements at once."""

    def interrupt(self) -> None:
        """Abort any pending database operation."""
    if sys.version_info >= (3, 13):
        def iterdump(self, *, filter: str | None = None) -> Generator[str, None, None]:
            """Returns iterator to the dump of the database in an SQL text format.

            filter
              An optional LIKE pattern for database objects to dump
            """
    else:
        def iterdump(self) -> Generator[str, None, None]:
            """Returns iterator to the dump of the database in an SQL text format."""

    def rollback(self) -> None:
        """Roll back to the start of any pending transaction.

        If there is no open transaction, this method is a no-op.
        """

    def set_authorizer(
        self, authorizer_callback: Callable[[int, str | None, str | None, str | None, str | None], int] | None
    ) -> None:
        """Set authorizer callback.

        Note: Passing keyword argument 'authorizer_callback' to
        _sqlite3.Connection.set_authorizer() is deprecated. Parameter
        'authorizer_callback' will become positional-only in Python 3.15.
        """

    def set_progress_handler(self, progress_handler: Callable[[], int | None] | None, n: int) -> None:
        """Set progress handler callback.

          progress_handler
            A callable that takes no arguments.
            If the callable returns non-zero, the current query is terminated,
            and an exception is raised.
          n
            The number of SQLite virtual machine instructions that are
            executed between invocations of 'progress_handler'.

        If 'progress_handler' is None or 'n' is 0, the progress handler is disabled.

        Note: Passing keyword argument 'progress_handler' to
        _sqlite3.Connection.set_progress_handler() is deprecated. Parameter
        'progress_handler' will become positional-only in Python 3.15.
        """

    def set_trace_callback(self, trace_callback: Callable[[str], object] | None) -> None:
        """Set a trace callback called for each SQL statement (passed as unicode).

        Note: Passing keyword argument 'trace_callback' to
        _sqlite3.Connection.set_trace_callback() is deprecated. Parameter
        'trace_callback' will become positional-only in Python 3.15.
        """
    # enable_load_extension and load_extension is not available on python distributions compiled
    # without sqlite3 loadable extension support. see footnotes https://docs.python.org/3/library/sqlite3.html#f1
    def enable_load_extension(self, enable: bool, /) -> None:
        """Enable dynamic loading of SQLite extension modules."""
    if sys.version_info >= (3, 12):
        def load_extension(self, name: str, /, *, entrypoint: str | None = None) -> None:
            """Load SQLite extension module."""
    else:
        def load_extension(self, name: str, /) -> None:
            """Load SQLite extension module."""

    def backup(
        self,
        target: Connection,
        *,
        pages: int = -1,
        progress: Callable[[int, int, int], object] | None = None,
        name: str = "main",
        sleep: float = 0.25,
    ) -> None:
        """Makes a backup of the database."""
    if sys.version_info >= (3, 11):
        def setlimit(self, category: int, limit: int, /) -> int:
            """Set connection run-time limits.

              category
                The limit category to be set.
              limit
                The new limit. If the new limit is a negative number, the limit is
                unchanged.

            Attempts to increase a limit above its hard upper bound are silently truncated
            to the hard upper bound. Regardless of whether or not the limit was changed,
            the prior value of the limit is returned.
            """

        def getlimit(self, category: int, /) -> int:
            """Get connection run-time limits.

            category
              The limit category to be queried.
            """

        def serialize(self, *, name: str = "main") -> bytes:
            """Serialize a database into a byte string.

              name
                Which database to serialize.

            For an ordinary on-disk database file, the serialization is just a copy of the
            disk file. For an in-memory database or a "temp" database, the serialization is
            the same sequence of bytes which would be written to disk if that database
            were backed up to disk.
            """

        def deserialize(self, data: ReadableBuffer, /, *, name: str = "main") -> None:
            """Load a serialized database.

              data
                The serialized database content.
              name
                Which database to reopen with the deserialization.

            The deserialize interface causes the database connection to disconnect from the
            target database, and then reopen it as an in-memory database based on the given
            serialized data.

            The deserialize interface will fail with SQLITE_BUSY if the database is
            currently in a read transaction or is involved in a backup operation.
            """
    if sys.version_info >= (3, 12):
        def getconfig(self, op: int, /) -> bool:
            """Query a boolean connection configuration option.

            op
              The configuration verb; one of the sqlite3.SQLITE_DBCONFIG codes.
            """

        def setconfig(self, op: int, enable: bool = True, /) -> bool:
            """Set a boolean connection configuration option.

            op
              The configuration verb; one of the sqlite3.SQLITE_DBCONFIG codes.
            """

    def __call__(self, sql: str, /) -> _Statement:
        """Call self as a function."""

    def __enter__(self) -> Self:
        """Called when the connection is used as a context manager.

        Returns itself as a convenience to the caller.
        """

    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None, /
    ) -> Literal[False]:
        """Called when the connection is used as a context manager.

        If there was any exception, a rollback takes place; otherwise we commit.
        """

@disjoint_base
class Cursor:
    """SQLite database cursor class."""

    arraysize: int
    @property
    def connection(self) -> Connection: ...
    # May be None, but using `| MaybeNone` (`| Any`) instead to avoid slightly annoying false positives.
    @property
    def description(self) -> tuple[tuple[str, None, None, None, None, None, None], ...] | MaybeNone: ...
    @property
    def lastrowid(self) -> int | None: ...
    row_factory: _RowFactoryOptions
    @property
    def rowcount(self) -> int: ...
    def __init__(self, cursor: Connection, /) -> None: ...
    def close(self) -> None:
        """Closes the cursor."""

    def execute(self, sql: str, parameters: _Parameters = (), /) -> Self:
        """Executes an SQL statement."""

    def executemany(self, sql: str, seq_of_parameters: Iterable[_Parameters], /) -> Self:
        """Repeatedly executes an SQL statement."""

    def executescript(self, sql_script: str, /) -> Cursor:
        """Executes multiple SQL statements at once."""

    def fetchall(self) -> list[Any]:
        """Fetches all rows from the resultset."""

    def fetchmany(self, size: int | None = 1) -> list[Any]:
        """Fetches several rows from the resultset.

        size
          The default value is set by the Cursor.arraysize attribute.
        """
    # Returns either a row (as created by the row_factory) or None, but
    # putting None in the return annotation causes annoying false positives.
    def fetchone(self) -> Any:
        """Fetches one row from the resultset."""

    def setinputsizes(self, sizes: Unused, /) -> None:  # does nothing
        """Required by DB-API. Does nothing in sqlite3."""

    def setoutputsize(self, size: Unused, column: Unused = None, /) -> None:  # does nothing
        """Required by DB-API. Does nothing in sqlite3."""

    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> Any:
        """Implement next(self)."""

@final
class PrepareProtocol:
    """PEP 246 style object adaption protocol type."""

    def __init__(self, *args: object, **kwargs: object) -> None: ...

@disjoint_base
class Row(Sequence[Any]):
    def __new__(cls, cursor: Cursor, data: tuple[Any, ...], /) -> Self: ...
    def keys(self) -> list[str]:
        """Returns the keys of the row."""

    @overload
    def __getitem__(self, key: int | str, /) -> Any:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> tuple[Any, ...]: ...
    def __hash__(self) -> int: ...
    def __iter__(self) -> Iterator[Any]:
        """Implement iter(self)."""

    def __len__(self) -> int:
        """Return len(self)."""
    # These return NotImplemented for anything that is not a Row.
    def __eq__(self, value: object, /) -> bool: ...
    def __ge__(self, value: object, /) -> bool: ...
    def __gt__(self, value: object, /) -> bool: ...
    def __le__(self, value: object, /) -> bool: ...
    def __lt__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...

# This class is not exposed. It calls itself sqlite3.Statement.
@final
@type_check_only
class _Statement: ...

if sys.version_info >= (3, 11):
    @final
    class Blob:
        def close(self) -> None:
            """Close the blob."""

        def read(self, length: int = -1, /) -> bytes:
            """Read data at the current offset position.

              length
                Read length in bytes.

            If the end of the blob is reached, the data up to end of file will be returned.
            When length is not specified, or is negative, Blob.read() will read until the
            end of the blob.
            """

        def write(self, data: ReadableBuffer, /) -> None:
            """Write data at the current offset.

            This function cannot change the blob length.  Writing beyond the end of the
            blob will result in an exception being raised.
            """

        def tell(self) -> int:
            """Return the current access position for the blob."""
        # whence must be one of os.SEEK_SET, os.SEEK_CUR, os.SEEK_END
        def seek(self, offset: int, origin: int = 0, /) -> None:
            """Set the current access position to offset.

            The origin argument defaults to os.SEEK_SET (absolute blob positioning).
            Other values for origin are os.SEEK_CUR (seek relative to the current position)
            and os.SEEK_END (seek relative to the blob's end).
            """

        def __len__(self) -> int:
            """Return len(self)."""

        def __enter__(self) -> Self:
            """Blob context manager enter."""

        def __exit__(self, type: object, val: object, tb: object, /) -> Literal[False]:
            """Blob context manager exit."""

        def __getitem__(self, key: SupportsIndex | slice, /) -> int:
            """Return self[key]."""

        def __setitem__(self, key: SupportsIndex | slice, value: int, /) -> None:
            """Set self[key] to value."""
