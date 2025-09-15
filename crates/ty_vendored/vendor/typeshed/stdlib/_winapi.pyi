import sys
from _typeshed import ReadableBuffer
from collections.abc import Sequence
from typing import Any, Final, Literal, NoReturn, final, overload

if sys.platform == "win32":
    ABOVE_NORMAL_PRIORITY_CLASS: Final = 0x8000
    BELOW_NORMAL_PRIORITY_CLASS: Final = 0x4000

    CREATE_BREAKAWAY_FROM_JOB: Final = 0x1000000
    CREATE_DEFAULT_ERROR_MODE: Final = 0x4000000
    CREATE_NO_WINDOW: Final = 0x8000000
    CREATE_NEW_CONSOLE: Final = 0x10
    CREATE_NEW_PROCESS_GROUP: Final = 0x200

    DETACHED_PROCESS: Final = 8
    DUPLICATE_CLOSE_SOURCE: Final = 1
    DUPLICATE_SAME_ACCESS: Final = 2

    ERROR_ALREADY_EXISTS: Final = 183
    ERROR_BROKEN_PIPE: Final = 109
    ERROR_IO_PENDING: Final = 997
    ERROR_MORE_DATA: Final = 234
    ERROR_NETNAME_DELETED: Final = 64
    ERROR_NO_DATA: Final = 232
    ERROR_NO_SYSTEM_RESOURCES: Final = 1450
    ERROR_OPERATION_ABORTED: Final = 995
    ERROR_PIPE_BUSY: Final = 231
    ERROR_PIPE_CONNECTED: Final = 535
    ERROR_SEM_TIMEOUT: Final = 121

    FILE_FLAG_FIRST_PIPE_INSTANCE: Final = 0x80000
    FILE_FLAG_OVERLAPPED: Final = 0x40000000

    FILE_GENERIC_READ: Final = 1179785
    FILE_GENERIC_WRITE: Final = 1179926

    FILE_MAP_ALL_ACCESS: Final = 983071
    FILE_MAP_COPY: Final = 1
    FILE_MAP_EXECUTE: Final = 32
    FILE_MAP_READ: Final = 4
    FILE_MAP_WRITE: Final = 2

    FILE_TYPE_CHAR: Final = 2
    FILE_TYPE_DISK: Final = 1
    FILE_TYPE_PIPE: Final = 3
    FILE_TYPE_REMOTE: Final = 32768
    FILE_TYPE_UNKNOWN: Final = 0

    GENERIC_READ: Final = 0x80000000
    GENERIC_WRITE: Final = 0x40000000
    HIGH_PRIORITY_CLASS: Final = 0x80
    INFINITE: Final = 0xFFFFFFFF
    # Ignore the Flake8 error -- flake8-pyi assumes
    # most numbers this long will be implementation details,
    # but here we can see that it's a power of 2
    INVALID_HANDLE_VALUE: Final = 0xFFFFFFFFFFFFFFFF  # noqa: Y054
    IDLE_PRIORITY_CLASS: Final = 0x40
    NORMAL_PRIORITY_CLASS: Final = 0x20
    REALTIME_PRIORITY_CLASS: Final = 0x100
    NMPWAIT_WAIT_FOREVER: Final = 0xFFFFFFFF

    MEM_COMMIT: Final = 0x1000
    MEM_FREE: Final = 0x10000
    MEM_IMAGE: Final = 0x1000000
    MEM_MAPPED: Final = 0x40000
    MEM_PRIVATE: Final = 0x20000
    MEM_RESERVE: Final = 0x2000

    NULL: Final = 0
    OPEN_EXISTING: Final = 3

    PIPE_ACCESS_DUPLEX: Final = 3
    PIPE_ACCESS_INBOUND: Final = 1
    PIPE_READMODE_MESSAGE: Final = 2
    PIPE_TYPE_MESSAGE: Final = 4
    PIPE_UNLIMITED_INSTANCES: Final = 255
    PIPE_WAIT: Final = 0

    PAGE_EXECUTE: Final = 0x10
    PAGE_EXECUTE_READ: Final = 0x20
    PAGE_EXECUTE_READWRITE: Final = 0x40
    PAGE_EXECUTE_WRITECOPY: Final = 0x80
    PAGE_GUARD: Final = 0x100
    PAGE_NOACCESS: Final = 0x1
    PAGE_NOCACHE: Final = 0x200
    PAGE_READONLY: Final = 0x2
    PAGE_READWRITE: Final = 0x4
    PAGE_WRITECOMBINE: Final = 0x400
    PAGE_WRITECOPY: Final = 0x8

    PROCESS_ALL_ACCESS: Final = 0x1FFFFF
    PROCESS_DUP_HANDLE: Final = 0x40

    SEC_COMMIT: Final = 0x8000000
    SEC_IMAGE: Final = 0x1000000
    SEC_LARGE_PAGES: Final = 0x80000000
    SEC_NOCACHE: Final = 0x10000000
    SEC_RESERVE: Final = 0x4000000
    SEC_WRITECOMBINE: Final = 0x40000000

    if sys.version_info >= (3, 13):
        STARTF_FORCEOFFFEEDBACK: Final = 0x80
        STARTF_FORCEONFEEDBACK: Final = 0x40
        STARTF_PREVENTPINNING: Final = 0x2000
        STARTF_RUNFULLSCREEN: Final = 0x20
        STARTF_TITLEISAPPID: Final = 0x1000
        STARTF_TITLEISLINKNAME: Final = 0x800
        STARTF_UNTRUSTEDSOURCE: Final = 0x8000
        STARTF_USECOUNTCHARS: Final = 0x8
        STARTF_USEFILLATTRIBUTE: Final = 0x10
        STARTF_USEHOTKEY: Final = 0x200
        STARTF_USEPOSITION: Final = 0x4
        STARTF_USESIZE: Final = 0x2

    STARTF_USESHOWWINDOW: Final = 0x1
    STARTF_USESTDHANDLES: Final = 0x100

    STD_ERROR_HANDLE: Final = 0xFFFFFFF4
    STD_OUTPUT_HANDLE: Final = 0xFFFFFFF5
    STD_INPUT_HANDLE: Final = 0xFFFFFFF6

    STILL_ACTIVE: Final = 259
    SW_HIDE: Final = 0
    SYNCHRONIZE: Final = 0x100000
    WAIT_ABANDONED_0: Final = 128
    WAIT_OBJECT_0: Final = 0
    WAIT_TIMEOUT: Final = 258

    if sys.version_info >= (3, 10):
        LOCALE_NAME_INVARIANT: Final[str]
        LOCALE_NAME_MAX_LENGTH: Final[int]
        LOCALE_NAME_SYSTEM_DEFAULT: Final[str]
        LOCALE_NAME_USER_DEFAULT: Final[str | None]

        LCMAP_FULLWIDTH: Final[int]
        LCMAP_HALFWIDTH: Final[int]
        LCMAP_HIRAGANA: Final[int]
        LCMAP_KATAKANA: Final[int]
        LCMAP_LINGUISTIC_CASING: Final[int]
        LCMAP_LOWERCASE: Final[int]
        LCMAP_SIMPLIFIED_CHINESE: Final[int]
        LCMAP_TITLECASE: Final[int]
        LCMAP_TRADITIONAL_CHINESE: Final[int]
        LCMAP_UPPERCASE: Final[int]

    if sys.version_info >= (3, 12):
        COPYFILE2_CALLBACK_CHUNK_STARTED: Final = 1
        COPYFILE2_CALLBACK_CHUNK_FINISHED: Final = 2
        COPYFILE2_CALLBACK_STREAM_STARTED: Final = 3
        COPYFILE2_CALLBACK_STREAM_FINISHED: Final = 4
        COPYFILE2_CALLBACK_POLL_CONTINUE: Final = 5
        COPYFILE2_CALLBACK_ERROR: Final = 6

        COPYFILE2_PROGRESS_CONTINUE: Final = 0
        COPYFILE2_PROGRESS_CANCEL: Final = 1
        COPYFILE2_PROGRESS_STOP: Final = 2
        COPYFILE2_PROGRESS_QUIET: Final = 3
        COPYFILE2_PROGRESS_PAUSE: Final = 4

        COPY_FILE_FAIL_IF_EXISTS: Final = 0x1
        COPY_FILE_RESTARTABLE: Final = 0x2
        COPY_FILE_OPEN_SOURCE_FOR_WRITE: Final = 0x4
        COPY_FILE_ALLOW_DECRYPTED_DESTINATION: Final = 0x8
        COPY_FILE_COPY_SYMLINK: Final = 0x800
        COPY_FILE_NO_BUFFERING: Final = 0x1000
        COPY_FILE_REQUEST_SECURITY_PRIVILEGES: Final = 0x2000
        COPY_FILE_RESUME_FROM_PAUSE: Final = 0x4000
        COPY_FILE_NO_OFFLOAD: Final = 0x40000
        COPY_FILE_REQUEST_COMPRESSED_TRAFFIC: Final = 0x10000000

        ERROR_ACCESS_DENIED: Final = 5
        ERROR_PRIVILEGE_NOT_HELD: Final = 1314

    if sys.version_info >= (3, 14):
        COPY_FILE_DIRECTORY: Final = 0x00000080

    def CloseHandle(handle: int, /) -> None: ...
    @overload
    def ConnectNamedPipe(handle: int, overlapped: Literal[True]) -> Overlapped: ...
    @overload
    def ConnectNamedPipe(handle: int, overlapped: Literal[False] = False) -> None: ...
    @overload
    def ConnectNamedPipe(handle: int, overlapped: bool) -> Overlapped | None: ...
    def CreateFile(
        file_name: str,
        desired_access: int,
        share_mode: int,
        security_attributes: int,
        creation_disposition: int,
        flags_and_attributes: int,
        template_file: int,
        /,
    ) -> int: ...
    def CreateJunction(src_path: str, dst_path: str, /) -> None: ...
    def CreateNamedPipe(
        name: str,
        open_mode: int,
        pipe_mode: int,
        max_instances: int,
        out_buffer_size: int,
        in_buffer_size: int,
        default_timeout: int,
        security_attributes: int,
        /,
    ) -> int: ...
    def CreatePipe(pipe_attrs: Any, size: int, /) -> tuple[int, int]: ...
    def CreateProcess(
        application_name: str | None,
        command_line: str | None,
        proc_attrs: Any,
        thread_attrs: Any,
        inherit_handles: bool,
        creation_flags: int,
        env_mapping: dict[str, str],
        current_directory: str | None,
        startup_info: Any,
        /,
    ) -> tuple[int, int, int, int]: ...
    def DuplicateHandle(
        source_process_handle: int,
        source_handle: int,
        target_process_handle: int,
        desired_access: int,
        inherit_handle: bool,
        options: int = 0,
        /,
    ) -> int: ...
    def ExitProcess(ExitCode: int, /) -> NoReturn: ...
    def GetACP() -> int: ...
    def GetFileType(handle: int) -> int: ...
    def GetCurrentProcess() -> int: ...
    def GetExitCodeProcess(process: int, /) -> int: ...
    def GetLastError() -> int: ...
    def GetModuleFileName(module_handle: int, /) -> str: ...
    def GetStdHandle(std_handle: int, /) -> int: ...
    def GetVersion() -> int: ...
    def OpenProcess(desired_access: int, inherit_handle: bool, process_id: int, /) -> int: ...
    def PeekNamedPipe(handle: int, size: int = 0, /) -> tuple[int, int] | tuple[bytes, int, int]: ...
    if sys.version_info >= (3, 10):
        def LCMapStringEx(locale: str, flags: int, src: str) -> str: ...
        def UnmapViewOfFile(address: int, /) -> None: ...

    @overload
    def ReadFile(handle: int, size: int, overlapped: Literal[True]) -> tuple[Overlapped, int]: ...
    @overload
    def ReadFile(handle: int, size: int, overlapped: Literal[False] = False) -> tuple[bytes, int]: ...
    @overload
    def ReadFile(handle: int, size: int, overlapped: int | bool) -> tuple[Any, int]: ...
    def SetNamedPipeHandleState(
        named_pipe: int, mode: int | None, max_collection_count: int | None, collect_data_timeout: int | None, /
    ) -> None: ...
    def TerminateProcess(handle: int, exit_code: int, /) -> None: ...
    def WaitForMultipleObjects(handle_seq: Sequence[int], wait_flag: bool, milliseconds: int = 0xFFFFFFFF, /) -> int: ...
    def WaitForSingleObject(handle: int, milliseconds: int, /) -> int: ...
    def WaitNamedPipe(name: str, timeout: int, /) -> None: ...
    @overload
    def WriteFile(handle: int, buffer: ReadableBuffer, overlapped: Literal[True]) -> tuple[Overlapped, int]: ...
    @overload
    def WriteFile(handle: int, buffer: ReadableBuffer, overlapped: Literal[False] = False) -> tuple[int, int]: ...
    @overload
    def WriteFile(handle: int, buffer: ReadableBuffer, overlapped: int | bool) -> tuple[Any, int]: ...
    @final
    class Overlapped:
        event: int
        def GetOverlappedResult(self, wait: bool, /) -> tuple[int, int]: ...
        def cancel(self) -> None: ...
        def getbuffer(self) -> bytes | None: ...

    if sys.version_info >= (3, 13):
        def BatchedWaitForMultipleObjects(
            handle_seq: Sequence[int], wait_all: bool, milliseconds: int = 0xFFFFFFFF
        ) -> list[int]: ...
        def CreateEventW(security_attributes: int, manual_reset: bool, initial_state: bool, name: str | None) -> int: ...
        def CreateMutexW(security_attributes: int, initial_owner: bool, name: str) -> int: ...
        def GetLongPathName(path: str) -> str: ...
        def GetShortPathName(path: str) -> str: ...
        def OpenEventW(desired_access: int, inherit_handle: bool, name: str) -> int: ...
        def OpenMutexW(desired_access: int, inherit_handle: bool, name: str) -> int: ...
        def ReleaseMutex(mutex: int) -> None: ...
        def ResetEvent(event: int) -> None: ...
        def SetEvent(event: int) -> None: ...

    if sys.version_info >= (3, 12):
        def CopyFile2(existing_file_name: str, new_file_name: str, flags: int, progress_routine: int | None = None) -> int: ...
        def NeedCurrentDirectoryForExePath(exe_name: str, /) -> bool: ...
