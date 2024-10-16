import sys
from _typeshed import ReadableBuffer, StrOrBytesPath
from collections.abc import Callable
from sqlite3 import (
    Connection as Connection,
    Cursor as Cursor,
    DatabaseError as DatabaseError,
    DataError as DataError,
    Error as Error,
    IntegrityError as IntegrityError,
    InterfaceError as InterfaceError,
    InternalError as InternalError,
    NotSupportedError as NotSupportedError,
    OperationalError as OperationalError,
    PrepareProtocol as PrepareProtocol,
    ProgrammingError as ProgrammingError,
    Row as Row,
    Warning as Warning,
)
from typing import Any, Final, Literal, TypeVar, overload
from typing_extensions import TypeAlias

if sys.version_info >= (3, 11):
    from sqlite3 import Blob as Blob

_T = TypeVar("_T")
_ConnectionT = TypeVar("_ConnectionT", bound=Connection)
_SqliteData: TypeAlias = str | ReadableBuffer | int | float | None
_Adapter: TypeAlias = Callable[[_T], _SqliteData]
_Converter: TypeAlias = Callable[[bytes], Any]

PARSE_COLNAMES: Final[int]
PARSE_DECLTYPES: Final[int]
SQLITE_ALTER_TABLE: Final[int]
SQLITE_ANALYZE: Final[int]
SQLITE_ATTACH: Final[int]
SQLITE_CREATE_INDEX: Final[int]
SQLITE_CREATE_TABLE: Final[int]
SQLITE_CREATE_TEMP_INDEX: Final[int]
SQLITE_CREATE_TEMP_TABLE: Final[int]
SQLITE_CREATE_TEMP_TRIGGER: Final[int]
SQLITE_CREATE_TEMP_VIEW: Final[int]
SQLITE_CREATE_TRIGGER: Final[int]
SQLITE_CREATE_VIEW: Final[int]
SQLITE_CREATE_VTABLE: Final[int]
SQLITE_DELETE: Final[int]
SQLITE_DENY: Final[int]
SQLITE_DETACH: Final[int]
SQLITE_DONE: Final[int]
SQLITE_DROP_INDEX: Final[int]
SQLITE_DROP_TABLE: Final[int]
SQLITE_DROP_TEMP_INDEX: Final[int]
SQLITE_DROP_TEMP_TABLE: Final[int]
SQLITE_DROP_TEMP_TRIGGER: Final[int]
SQLITE_DROP_TEMP_VIEW: Final[int]
SQLITE_DROP_TRIGGER: Final[int]
SQLITE_DROP_VIEW: Final[int]
SQLITE_DROP_VTABLE: Final[int]
SQLITE_FUNCTION: Final[int]
SQLITE_IGNORE: Final[int]
SQLITE_INSERT: Final[int]
SQLITE_OK: Final[int]
SQLITE_PRAGMA: Final[int]
SQLITE_READ: Final[int]
SQLITE_RECURSIVE: Final[int]
SQLITE_REINDEX: Final[int]
SQLITE_SAVEPOINT: Final[int]
SQLITE_SELECT: Final[int]
SQLITE_TRANSACTION: Final[int]
SQLITE_UPDATE: Final[int]
adapters: dict[tuple[type[Any], type[Any]], _Adapter[Any]]
converters: dict[str, _Converter]
sqlite_version: str

if sys.version_info < (3, 12):
    version: str

if sys.version_info >= (3, 12):
    LEGACY_TRANSACTION_CONTROL: Final[int]
    SQLITE_DBCONFIG_DEFENSIVE: Final[int]
    SQLITE_DBCONFIG_DQS_DDL: Final[int]
    SQLITE_DBCONFIG_DQS_DML: Final[int]
    SQLITE_DBCONFIG_ENABLE_FKEY: Final[int]
    SQLITE_DBCONFIG_ENABLE_FTS3_TOKENIZER: Final[int]
    SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION: Final[int]
    SQLITE_DBCONFIG_ENABLE_QPSG: Final[int]
    SQLITE_DBCONFIG_ENABLE_TRIGGER: Final[int]
    SQLITE_DBCONFIG_ENABLE_VIEW: Final[int]
    SQLITE_DBCONFIG_LEGACY_ALTER_TABLE: Final[int]
    SQLITE_DBCONFIG_LEGACY_FILE_FORMAT: Final[int]
    SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE: Final[int]
    SQLITE_DBCONFIG_RESET_DATABASE: Final[int]
    SQLITE_DBCONFIG_TRIGGER_EQP: Final[int]
    SQLITE_DBCONFIG_TRUSTED_SCHEMA: Final[int]
    SQLITE_DBCONFIG_WRITABLE_SCHEMA: Final[int]

if sys.version_info >= (3, 11):
    SQLITE_ABORT: Final[int]
    SQLITE_ABORT_ROLLBACK: Final[int]
    SQLITE_AUTH: Final[int]
    SQLITE_AUTH_USER: Final[int]
    SQLITE_BUSY: Final[int]
    SQLITE_BUSY_RECOVERY: Final[int]
    SQLITE_BUSY_SNAPSHOT: Final[int]
    SQLITE_BUSY_TIMEOUT: Final[int]
    SQLITE_CANTOPEN: Final[int]
    SQLITE_CANTOPEN_CONVPATH: Final[int]
    SQLITE_CANTOPEN_DIRTYWAL: Final[int]
    SQLITE_CANTOPEN_FULLPATH: Final[int]
    SQLITE_CANTOPEN_ISDIR: Final[int]
    SQLITE_CANTOPEN_NOTEMPDIR: Final[int]
    SQLITE_CANTOPEN_SYMLINK: Final[int]
    SQLITE_CONSTRAINT: Final[int]
    SQLITE_CONSTRAINT_CHECK: Final[int]
    SQLITE_CONSTRAINT_COMMITHOOK: Final[int]
    SQLITE_CONSTRAINT_FOREIGNKEY: Final[int]
    SQLITE_CONSTRAINT_FUNCTION: Final[int]
    SQLITE_CONSTRAINT_NOTNULL: Final[int]
    SQLITE_CONSTRAINT_PINNED: Final[int]
    SQLITE_CONSTRAINT_PRIMARYKEY: Final[int]
    SQLITE_CONSTRAINT_ROWID: Final[int]
    SQLITE_CONSTRAINT_TRIGGER: Final[int]
    SQLITE_CONSTRAINT_UNIQUE: Final[int]
    SQLITE_CONSTRAINT_VTAB: Final[int]
    SQLITE_CORRUPT: Final[int]
    SQLITE_CORRUPT_INDEX: Final[int]
    SQLITE_CORRUPT_SEQUENCE: Final[int]
    SQLITE_CORRUPT_VTAB: Final[int]
    SQLITE_EMPTY: Final[int]
    SQLITE_ERROR: Final[int]
    SQLITE_ERROR_MISSING_COLLSEQ: Final[int]
    SQLITE_ERROR_RETRY: Final[int]
    SQLITE_ERROR_SNAPSHOT: Final[int]
    SQLITE_FORMAT: Final[int]
    SQLITE_FULL: Final[int]
    SQLITE_INTERNAL: Final[int]
    SQLITE_INTERRUPT: Final[int]
    SQLITE_IOERR: Final[int]
    SQLITE_IOERR_ACCESS: Final[int]
    SQLITE_IOERR_AUTH: Final[int]
    SQLITE_IOERR_BEGIN_ATOMIC: Final[int]
    SQLITE_IOERR_BLOCKED: Final[int]
    SQLITE_IOERR_CHECKRESERVEDLOCK: Final[int]
    SQLITE_IOERR_CLOSE: Final[int]
    SQLITE_IOERR_COMMIT_ATOMIC: Final[int]
    SQLITE_IOERR_CONVPATH: Final[int]
    SQLITE_IOERR_CORRUPTFS: Final[int]
    SQLITE_IOERR_DATA: Final[int]
    SQLITE_IOERR_DELETE: Final[int]
    SQLITE_IOERR_DELETE_NOENT: Final[int]
    SQLITE_IOERR_DIR_CLOSE: Final[int]
    SQLITE_IOERR_DIR_FSYNC: Final[int]
    SQLITE_IOERR_FSTAT: Final[int]
    SQLITE_IOERR_FSYNC: Final[int]
    SQLITE_IOERR_GETTEMPPATH: Final[int]
    SQLITE_IOERR_LOCK: Final[int]
    SQLITE_IOERR_MMAP: Final[int]
    SQLITE_IOERR_NOMEM: Final[int]
    SQLITE_IOERR_RDLOCK: Final[int]
    SQLITE_IOERR_READ: Final[int]
    SQLITE_IOERR_ROLLBACK_ATOMIC: Final[int]
    SQLITE_IOERR_SEEK: Final[int]
    SQLITE_IOERR_SHMLOCK: Final[int]
    SQLITE_IOERR_SHMMAP: Final[int]
    SQLITE_IOERR_SHMOPEN: Final[int]
    SQLITE_IOERR_SHMSIZE: Final[int]
    SQLITE_IOERR_SHORT_READ: Final[int]
    SQLITE_IOERR_TRUNCATE: Final[int]
    SQLITE_IOERR_UNLOCK: Final[int]
    SQLITE_IOERR_VNODE: Final[int]
    SQLITE_IOERR_WRITE: Final[int]
    SQLITE_LIMIT_ATTACHED: Final[int]
    SQLITE_LIMIT_COLUMN: Final[int]
    SQLITE_LIMIT_COMPOUND_SELECT: Final[int]
    SQLITE_LIMIT_EXPR_DEPTH: Final[int]
    SQLITE_LIMIT_FUNCTION_ARG: Final[int]
    SQLITE_LIMIT_LENGTH: Final[int]
    SQLITE_LIMIT_LIKE_PATTERN_LENGTH: Final[int]
    SQLITE_LIMIT_SQL_LENGTH: Final[int]
    SQLITE_LIMIT_TRIGGER_DEPTH: Final[int]
    SQLITE_LIMIT_VARIABLE_NUMBER: Final[int]
    SQLITE_LIMIT_VDBE_OP: Final[int]
    SQLITE_LIMIT_WORKER_THREADS: Final[int]
    SQLITE_LOCKED: Final[int]
    SQLITE_LOCKED_SHAREDCACHE: Final[int]
    SQLITE_LOCKED_VTAB: Final[int]
    SQLITE_MISMATCH: Final[int]
    SQLITE_MISUSE: Final[int]
    SQLITE_NOLFS: Final[int]
    SQLITE_NOMEM: Final[int]
    SQLITE_NOTADB: Final[int]
    SQLITE_NOTFOUND: Final[int]
    SQLITE_NOTICE: Final[int]
    SQLITE_NOTICE_RECOVER_ROLLBACK: Final[int]
    SQLITE_NOTICE_RECOVER_WAL: Final[int]
    SQLITE_OK_LOAD_PERMANENTLY: Final[int]
    SQLITE_OK_SYMLINK: Final[int]
    SQLITE_PERM: Final[int]
    SQLITE_PROTOCOL: Final[int]
    SQLITE_RANGE: Final[int]
    SQLITE_READONLY: Final[int]
    SQLITE_READONLY_CANTINIT: Final[int]
    SQLITE_READONLY_CANTLOCK: Final[int]
    SQLITE_READONLY_DBMOVED: Final[int]
    SQLITE_READONLY_DIRECTORY: Final[int]
    SQLITE_READONLY_RECOVERY: Final[int]
    SQLITE_READONLY_ROLLBACK: Final[int]
    SQLITE_ROW: Final[int]
    SQLITE_SCHEMA: Final[int]
    SQLITE_TOOBIG: Final[int]
    SQLITE_WARNING: Final[int]
    SQLITE_WARNING_AUTOINDEX: Final[int]
    threadsafety: Final[int]

# Can take or return anything depending on what's in the registry.
@overload
def adapt(obj: Any, proto: Any, /) -> Any: ...
@overload
def adapt(obj: Any, proto: Any, alt: _T, /) -> Any | _T: ...
def complete_statement(statement: str) -> bool: ...

if sys.version_info >= (3, 12):
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float = 5.0,
        detect_types: int = 0,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None = "DEFERRED",
        check_same_thread: bool = True,
        cached_statements: int = 128,
        uri: bool = False,
        *,
        autocommit: bool = ...,
    ) -> Connection: ...
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float,
        detect_types: int,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None,
        check_same_thread: bool,
        factory: type[_ConnectionT],
        cached_statements: int = 128,
        uri: bool = False,
        *,
        autocommit: bool = ...,
    ) -> _ConnectionT: ...
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float = 5.0,
        detect_types: int = 0,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None = "DEFERRED",
        check_same_thread: bool = True,
        *,
        factory: type[_ConnectionT],
        cached_statements: int = 128,
        uri: bool = False,
        autocommit: bool = ...,
    ) -> _ConnectionT: ...

else:
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float = 5.0,
        detect_types: int = 0,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None = "DEFERRED",
        check_same_thread: bool = True,
        cached_statements: int = 128,
        uri: bool = False,
    ) -> Connection: ...
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float,
        detect_types: int,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None,
        check_same_thread: bool,
        factory: type[_ConnectionT],
        cached_statements: int = 128,
        uri: bool = False,
    ) -> _ConnectionT: ...
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float = 5.0,
        detect_types: int = 0,
        isolation_level: Literal["DEFERRED", "EXCLUSIVE", "IMMEDIATE"] | None = "DEFERRED",
        check_same_thread: bool = True,
        *,
        factory: type[_ConnectionT],
        cached_statements: int = 128,
        uri: bool = False,
    ) -> _ConnectionT: ...

def enable_callback_tracebacks(enable: bool, /) -> None: ...

if sys.version_info < (3, 12):
    # takes a pos-or-keyword argument because there is a C wrapper
    def enable_shared_cache(do_enable: int) -> None: ...

if sys.version_info >= (3, 10):
    def register_adapter(type: type[_T], adapter: _Adapter[_T], /) -> None: ...
    def register_converter(typename: str, converter: _Converter, /) -> None: ...

else:
    def register_adapter(type: type[_T], caster: _Adapter[_T], /) -> None: ...
    def register_converter(name: str, converter: _Converter, /) -> None: ...

if sys.version_info < (3, 10):
    OptimizedUnicode = str
