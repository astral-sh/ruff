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
    row_factory: Any
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

    def close(self) -> None: ...
    if sys.version_info >= (3, 11):
        def blobopen(self, table: str, column: str, row: int, /, *, readonly: bool = False, name: str = "main") -> Blob: ...

    def commit(self) -> None: ...
    def create_aggregate(self, name: str, n_arg: int, aggregate_class: Callable[[], _AggregateProtocol]) -> None: ...
    if sys.version_info >= (3, 11):
        # num_params determines how many params will be passed to the aggregate class. We provide an overload
        # for the case where num_params = 1, which is expected to be the common case.
        @overload
        def create_window_function(
            self, name: str, num_params: Literal[1], aggregate_class: Callable[[], _SingleParamWindowAggregateClass] | None, /
        ) -> None: ...
        # And for num_params = -1, which means the aggregate must accept any number of parameters.
        @overload
        def create_window_function(
            self, name: str, num_params: Literal[-1], aggregate_class: Callable[[], _AnyParamWindowAggregateClass] | None, /
        ) -> None: ...
        @overload
        def create_window_function(
            self, name: str, num_params: int, aggregate_class: Callable[[], _WindowAggregateClass] | None, /
        ) -> None: ...

    def create_collation(self, name: str, callback: Callable[[str, str], int | SupportsIndex] | None, /) -> None: ...
    def create_function(
        self, name: str, narg: int, func: Callable[..., _SqliteData] | None, *, deterministic: bool = False
    ) -> None: ...
    @overload
    def cursor(self, factory: None = None) -> Cursor: ...
    @overload
    def cursor(self, factory: Callable[[Connection], _CursorT]) -> _CursorT: ...
    def execute(self, sql: str, parameters: _Parameters = ..., /) -> Cursor: ...
    def executemany(self, sql: str, parameters: Iterable[_Parameters], /) -> Cursor: ...
    def executescript(self, sql_script: str, /) -> Cursor: ...
    def interrupt(self) -> None: ...
    if sys.version_info >= (3, 13):
        def iterdump(self, *, filter: str | None = None) -> Generator[str, None, None]: ...
    else:
        def iterdump(self) -> Generator[str, None, None]: ...

    def rollback(self) -> None: ...
    def set_authorizer(
        self, authorizer_callback: Callable[[int, str | None, str | None, str | None, str | None], int] | None
    ) -> None: ...
    def set_progress_handler(self, progress_handler: Callable[[], int | None] | None, n: int) -> None: ...
    def set_trace_callback(self, trace_callback: Callable[[str], object] | None) -> None: ...
    # enable_load_extension and load_extension is not available on python distributions compiled
    # without sqlite3 loadable extension support. see footnotes https://docs.python.org/3/library/sqlite3.html#f1
    def enable_load_extension(self, enable: bool, /) -> None: ...
    if sys.version_info >= (3, 12):
        def load_extension(self, name: str, /, *, entrypoint: str | None = None) -> None: ...
    else:
        def load_extension(self, name: str, /) -> None: ...

    def backup(
        self,
        target: Connection,
        *,
        pages: int = -1,
        progress: Callable[[int, int, int], object] | None = None,
        name: str = "main",
        sleep: float = 0.25,
    ) -> None: ...
    if sys.version_info >= (3, 11):
        def setlimit(self, category: int, limit: int, /) -> int: ...
        def getlimit(self, category: int, /) -> int: ...
        def serialize(self, *, name: str = "main") -> bytes: ...
        def deserialize(self, data: ReadableBuffer, /, *, name: str = "main") -> None: ...
    if sys.version_info >= (3, 12):
        def getconfig(self, op: int, /) -> bool: ...
        def setconfig(self, op: int, enable: bool = True, /) -> bool: ...

    def __call__(self, sql: str, /) -> _Statement: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None, /
    ) -> Literal[False]: ...

@disjoint_base
class Cursor:
    arraysize: int
    @property
    def connection(self) -> Connection: ...
    # May be None, but using `| MaybeNone` (`| Any`) instead to avoid slightly annoying false positives.
    @property
    def description(self) -> tuple[tuple[str, None, None, None, None, None, None], ...] | MaybeNone: ...
    @property
    def lastrowid(self) -> int | None: ...
    row_factory: Callable[[Cursor, Row], object] | None
    @property
    def rowcount(self) -> int: ...
    def __init__(self, cursor: Connection, /) -> None: ...
    def close(self) -> None: ...
    def execute(self, sql: str, parameters: _Parameters = (), /) -> Self: ...
    def executemany(self, sql: str, seq_of_parameters: Iterable[_Parameters], /) -> Self: ...
    def executescript(self, sql_script: str, /) -> Cursor: ...
    def fetchall(self) -> list[Any]: ...
    def fetchmany(self, size: int | None = 1) -> list[Any]: ...
    # Returns either a row (as created by the row_factory) or None, but
    # putting None in the return annotation causes annoying false positives.
    def fetchone(self) -> Any: ...
    def setinputsizes(self, sizes: Unused, /) -> None: ...  # does nothing
    def setoutputsize(self, size: Unused, column: Unused = None, /) -> None: ...  # does nothing
    def __iter__(self) -> Self: ...
    def __next__(self) -> Any: ...

@final
class PrepareProtocol:
    def __init__(self, *args: object, **kwargs: object) -> None: ...

@disjoint_base
class Row(Sequence[Any]):
    def __new__(cls, cursor: Cursor, data: tuple[Any, ...], /) -> Self: ...
    def keys(self) -> list[str]: ...
    @overload
    def __getitem__(self, key: int | str, /) -> Any: ...
    @overload
    def __getitem__(self, key: slice, /) -> tuple[Any, ...]: ...
    def __hash__(self) -> int: ...
    def __iter__(self) -> Iterator[Any]: ...
    def __len__(self) -> int: ...
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
        def close(self) -> None: ...
        def read(self, length: int = -1, /) -> bytes: ...
        def write(self, data: ReadableBuffer, /) -> None: ...
        def tell(self) -> int: ...
        # whence must be one of os.SEEK_SET, os.SEEK_CUR, os.SEEK_END
        def seek(self, offset: int, origin: int = 0, /) -> None: ...
        def __len__(self) -> int: ...
        def __enter__(self) -> Self: ...
        def __exit__(self, type: object, val: object, tb: object, /) -> Literal[False]: ...
        def __getitem__(self, key: SupportsIndex | slice, /) -> int: ...
        def __setitem__(self, key: SupportsIndex | slice, value: int, /) -> None: ...
