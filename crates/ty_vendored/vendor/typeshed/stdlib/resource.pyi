import sys
from _typeshed import structseq
from typing import Final, final

if sys.platform != "win32":
    # Depends on resource.h
    RLIMIT_AS: Final[int]
    RLIMIT_CORE: Final[int]
    RLIMIT_CPU: Final[int]
    RLIMIT_DATA: Final[int]
    RLIMIT_FSIZE: Final[int]
    RLIMIT_MEMLOCK: Final[int]
    RLIMIT_NOFILE: Final[int]
    RLIMIT_NPROC: Final[int]
    RLIMIT_RSS: Final[int]
    RLIMIT_STACK: Final[int]
    RLIM_INFINITY: Final[int]
    RUSAGE_CHILDREN: Final[int]
    RUSAGE_SELF: Final[int]
    if sys.platform == "linux":
        RLIMIT_MSGQUEUE: Final[int]
        RLIMIT_NICE: Final[int]
        RLIMIT_OFILE: Final[int]
        RLIMIT_RTPRIO: Final[int]
        RLIMIT_RTTIME: Final[int]
        RLIMIT_SIGPENDING: Final[int]
        RUSAGE_THREAD: Final[int]

    @final
    class struct_rusage(
        structseq[float], tuple[float, float, int, int, int, int, int, int, int, int, int, int, int, int, int, int]
    ):
        if sys.version_info >= (3, 10):
            __match_args__: Final = (
                "ru_utime",
                "ru_stime",
                "ru_maxrss",
                "ru_ixrss",
                "ru_idrss",
                "ru_isrss",
                "ru_minflt",
                "ru_majflt",
                "ru_nswap",
                "ru_inblock",
                "ru_oublock",
                "ru_msgsnd",
                "ru_msgrcv",
                "ru_nsignals",
                "ru_nvcsw",
                "ru_nivcsw",
            )

        @property
        def ru_utime(self) -> float: ...
        @property
        def ru_stime(self) -> float: ...
        @property
        def ru_maxrss(self) -> int: ...
        @property
        def ru_ixrss(self) -> int: ...
        @property
        def ru_idrss(self) -> int: ...
        @property
        def ru_isrss(self) -> int: ...
        @property
        def ru_minflt(self) -> int: ...
        @property
        def ru_majflt(self) -> int: ...
        @property
        def ru_nswap(self) -> int: ...
        @property
        def ru_inblock(self) -> int: ...
        @property
        def ru_oublock(self) -> int: ...
        @property
        def ru_msgsnd(self) -> int: ...
        @property
        def ru_msgrcv(self) -> int: ...
        @property
        def ru_nsignals(self) -> int: ...
        @property
        def ru_nvcsw(self) -> int: ...
        @property
        def ru_nivcsw(self) -> int: ...

    def getpagesize() -> int: ...
    def getrlimit(resource: int, /) -> tuple[int, int]: ...
    def getrusage(who: int, /) -> struct_rusage: ...
    def setrlimit(resource: int, limits: tuple[int, int], /) -> None: ...
    if sys.platform == "linux":
        if sys.version_info >= (3, 12):
            def prlimit(pid: int, resource: int, limits: tuple[int, int] | None = None, /) -> tuple[int, int]: ...
        else:
            def prlimit(pid: int, resource: int, limits: tuple[int, int] = ..., /) -> tuple[int, int]: ...
    error = OSError
