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
    _IsolationLevel,
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

PARSE_COLNAMES: Final = 2
PARSE_DECLTYPES: Final = 1
SQLITE_ALTER_TABLE: Final = 26
SQLITE_ANALYZE: Final = 28
SQLITE_ATTACH: Final = 24
SQLITE_CREATE_INDEX: Final = 1
SQLITE_CREATE_TABLE: Final = 2
SQLITE_CREATE_TEMP_INDEX: Final = 3
SQLITE_CREATE_TEMP_TABLE: Final = 4
SQLITE_CREATE_TEMP_TRIGGER: Final = 5
SQLITE_CREATE_TEMP_VIEW: Final = 6
SQLITE_CREATE_TRIGGER: Final = 7
SQLITE_CREATE_VIEW: Final = 8
SQLITE_CREATE_VTABLE: Final = 29
SQLITE_DELETE: Final = 9
SQLITE_DENY: Final = 1
SQLITE_DETACH: Final = 25
SQLITE_DONE: Final = 101
SQLITE_DROP_INDEX: Final = 10
SQLITE_DROP_TABLE: Final = 11
SQLITE_DROP_TEMP_INDEX: Final = 12
SQLITE_DROP_TEMP_TABLE: Final = 13
SQLITE_DROP_TEMP_TRIGGER: Final = 14
SQLITE_DROP_TEMP_VIEW: Final = 15
SQLITE_DROP_TRIGGER: Final = 16
SQLITE_DROP_VIEW: Final = 17
SQLITE_DROP_VTABLE: Final = 30
SQLITE_FUNCTION: Final = 31
SQLITE_IGNORE: Final = 2
SQLITE_INSERT: Final = 18
SQLITE_OK: Final = 0
SQLITE_PRAGMA: Final = 19
SQLITE_READ: Final = 20
SQLITE_RECURSIVE: Final = 33
SQLITE_REINDEX: Final = 27
SQLITE_SAVEPOINT: Final = 32
SQLITE_SELECT: Final = 21
SQLITE_TRANSACTION: Final = 22
SQLITE_UPDATE: Final = 23
adapters: dict[tuple[type[Any], type[Any]], _Adapter[Any]]
converters: dict[str, _Converter]
sqlite_version: str

if sys.version_info < (3, 12):
    version: str

if sys.version_info >= (3, 12):
    LEGACY_TRANSACTION_CONTROL: Final = -1
    SQLITE_DBCONFIG_DEFENSIVE: Final = 1010
    SQLITE_DBCONFIG_DQS_DDL: Final = 1014
    SQLITE_DBCONFIG_DQS_DML: Final = 1013
    SQLITE_DBCONFIG_ENABLE_FKEY: Final = 1002
    SQLITE_DBCONFIG_ENABLE_FTS3_TOKENIZER: Final = 1004
    SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION: Final = 1005
    SQLITE_DBCONFIG_ENABLE_QPSG: Final = 1007
    SQLITE_DBCONFIG_ENABLE_TRIGGER: Final = 1003
    SQLITE_DBCONFIG_ENABLE_VIEW: Final = 1015
    SQLITE_DBCONFIG_LEGACY_ALTER_TABLE: Final = 1012
    SQLITE_DBCONFIG_LEGACY_FILE_FORMAT: Final = 1016
    SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE: Final = 1006
    SQLITE_DBCONFIG_RESET_DATABASE: Final = 1009
    SQLITE_DBCONFIG_TRIGGER_EQP: Final = 1008
    SQLITE_DBCONFIG_TRUSTED_SCHEMA: Final = 1017
    SQLITE_DBCONFIG_WRITABLE_SCHEMA: Final = 1011

if sys.version_info >= (3, 11):
    SQLITE_ABORT: Final = 4
    SQLITE_ABORT_ROLLBACK: Final = 516
    SQLITE_AUTH: Final = 23
    SQLITE_AUTH_USER: Final = 279
    SQLITE_BUSY: Final = 5
    SQLITE_BUSY_RECOVERY: Final = 261
    SQLITE_BUSY_SNAPSHOT: Final = 517
    SQLITE_BUSY_TIMEOUT: Final = 773
    SQLITE_CANTOPEN: Final = 14
    SQLITE_CANTOPEN_CONVPATH: Final = 1038
    SQLITE_CANTOPEN_DIRTYWAL: Final = 1294
    SQLITE_CANTOPEN_FULLPATH: Final = 782
    SQLITE_CANTOPEN_ISDIR: Final = 526
    SQLITE_CANTOPEN_NOTEMPDIR: Final = 270
    SQLITE_CANTOPEN_SYMLINK: Final = 1550
    SQLITE_CONSTRAINT: Final = 19
    SQLITE_CONSTRAINT_CHECK: Final = 275
    SQLITE_CONSTRAINT_COMMITHOOK: Final = 531
    SQLITE_CONSTRAINT_FOREIGNKEY: Final = 787
    SQLITE_CONSTRAINT_FUNCTION: Final = 1043
    SQLITE_CONSTRAINT_NOTNULL: Final = 1299
    SQLITE_CONSTRAINT_PINNED: Final = 2835
    SQLITE_CONSTRAINT_PRIMARYKEY: Final = 1555
    SQLITE_CONSTRAINT_ROWID: Final = 2579
    SQLITE_CONSTRAINT_TRIGGER: Final = 1811
    SQLITE_CONSTRAINT_UNIQUE: Final = 2067
    SQLITE_CONSTRAINT_VTAB: Final = 2323
    SQLITE_CORRUPT: Final = 11
    SQLITE_CORRUPT_INDEX: Final = 779
    SQLITE_CORRUPT_SEQUENCE: Final = 523
    SQLITE_CORRUPT_VTAB: Final = 267
    SQLITE_EMPTY: Final = 16
    SQLITE_ERROR: Final = 1
    SQLITE_ERROR_MISSING_COLLSEQ: Final = 257
    SQLITE_ERROR_RETRY: Final = 513
    SQLITE_ERROR_SNAPSHOT: Final = 769
    SQLITE_FORMAT: Final = 24
    SQLITE_FULL: Final = 13
    SQLITE_INTERNAL: Final = 2
    SQLITE_INTERRUPT: Final = 9
    SQLITE_IOERR: Final = 10
    SQLITE_IOERR_ACCESS: Final = 3338
    SQLITE_IOERR_AUTH: Final = 7178
    SQLITE_IOERR_BEGIN_ATOMIC: Final = 7434
    SQLITE_IOERR_BLOCKED: Final = 2826
    SQLITE_IOERR_CHECKRESERVEDLOCK: Final = 3594
    SQLITE_IOERR_CLOSE: Final = 4106
    SQLITE_IOERR_COMMIT_ATOMIC: Final = 7690
    SQLITE_IOERR_CONVPATH: Final = 6666
    SQLITE_IOERR_CORRUPTFS: Final = 8458
    SQLITE_IOERR_DATA: Final = 8202
    SQLITE_IOERR_DELETE: Final = 2570
    SQLITE_IOERR_DELETE_NOENT: Final = 5898
    SQLITE_IOERR_DIR_CLOSE: Final = 4362
    SQLITE_IOERR_DIR_FSYNC: Final = 1290
    SQLITE_IOERR_FSTAT: Final = 1802
    SQLITE_IOERR_FSYNC: Final = 1034
    SQLITE_IOERR_GETTEMPPATH: Final = 6410
    SQLITE_IOERR_LOCK: Final = 3850
    SQLITE_IOERR_MMAP: Final = 6154
    SQLITE_IOERR_NOMEM: Final = 3082
    SQLITE_IOERR_RDLOCK: Final = 2314
    SQLITE_IOERR_READ: Final = 266
    SQLITE_IOERR_ROLLBACK_ATOMIC: Final = 7946
    SQLITE_IOERR_SEEK: Final = 5642
    SQLITE_IOERR_SHMLOCK: Final = 5130
    SQLITE_IOERR_SHMMAP: Final = 5386
    SQLITE_IOERR_SHMOPEN: Final = 4618
    SQLITE_IOERR_SHMSIZE: Final = 4874
    SQLITE_IOERR_SHORT_READ: Final = 522
    SQLITE_IOERR_TRUNCATE: Final = 1546
    SQLITE_IOERR_UNLOCK: Final = 2058
    SQLITE_IOERR_VNODE: Final = 6922
    SQLITE_IOERR_WRITE: Final = 778
    SQLITE_LIMIT_ATTACHED: Final = 7
    SQLITE_LIMIT_COLUMN: Final = 22
    SQLITE_LIMIT_COMPOUND_SELECT: Final = 4
    SQLITE_LIMIT_EXPR_DEPTH: Final = 3
    SQLITE_LIMIT_FUNCTION_ARG: Final = 6
    SQLITE_LIMIT_LENGTH: Final = 0
    SQLITE_LIMIT_LIKE_PATTERN_LENGTH: Final = 8
    SQLITE_LIMIT_SQL_LENGTH: Final = 1
    SQLITE_LIMIT_TRIGGER_DEPTH: Final = 10
    SQLITE_LIMIT_VARIABLE_NUMBER: Final = 9
    SQLITE_LIMIT_VDBE_OP: Final = 5
    SQLITE_LIMIT_WORKER_THREADS: Final = 11
    SQLITE_LOCKED: Final = 6
    SQLITE_LOCKED_SHAREDCACHE: Final = 262
    SQLITE_LOCKED_VTAB: Final = 518
    SQLITE_MISMATCH: Final = 20
    SQLITE_MISUSE: Final = 21
    SQLITE_NOLFS: Final = 22
    SQLITE_NOMEM: Final = 7
    SQLITE_NOTADB: Final = 26
    SQLITE_NOTFOUND: Final = 12
    SQLITE_NOTICE: Final = 27
    SQLITE_NOTICE_RECOVER_ROLLBACK: Final = 539
    SQLITE_NOTICE_RECOVER_WAL: Final = 283
    SQLITE_OK_LOAD_PERMANENTLY: Final = 256
    SQLITE_OK_SYMLINK: Final = 512
    SQLITE_PERM: Final = 3
    SQLITE_PROTOCOL: Final = 15
    SQLITE_RANGE: Final = 25
    SQLITE_READONLY: Final = 8
    SQLITE_READONLY_CANTINIT: Final = 1288
    SQLITE_READONLY_CANTLOCK: Final = 520
    SQLITE_READONLY_DBMOVED: Final = 1032
    SQLITE_READONLY_DIRECTORY: Final = 1544
    SQLITE_READONLY_RECOVERY: Final = 264
    SQLITE_READONLY_ROLLBACK: Final = 776
    SQLITE_ROW: Final = 100
    SQLITE_SCHEMA: Final = 17
    SQLITE_TOOBIG: Final = 18
    SQLITE_WARNING: Final = 28
    SQLITE_WARNING_AUTOINDEX: Final = 284
    threadsafety: Literal[0, 1, 3]

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
        isolation_level: _IsolationLevel = "DEFERRED",
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
        isolation_level: _IsolationLevel,
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
        isolation_level: _IsolationLevel = "DEFERRED",
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
        isolation_level: _IsolationLevel = "DEFERRED",
        check_same_thread: bool = True,
        cached_statements: int = 128,
        uri: bool = False,
    ) -> Connection: ...
    @overload
    def connect(
        database: StrOrBytesPath,
        timeout: float,
        detect_types: int,
        isolation_level: _IsolationLevel,
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
        isolation_level: _IsolationLevel = "DEFERRED",
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
