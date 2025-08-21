import sys
from _typeshed import structseq
from typing import Final, final

if sys.platform != "win32":
    RLIMIT_AS: int
    RLIMIT_CORE: int
    RLIMIT_CPU: int
    RLIMIT_DATA: int
    RLIMIT_FSIZE: int
    RLIMIT_MEMLOCK: int
    RLIMIT_NOFILE: int
    RLIMIT_NPROC: int
    RLIMIT_RSS: int
    RLIMIT_STACK: int
    RLIM_INFINITY: int
    RUSAGE_CHILDREN: int
    RUSAGE_SELF: int
    if sys.platform == "linux":
        RLIMIT_MSGQUEUE: int
        RLIMIT_NICE: int
        RLIMIT_OFILE: int
        RLIMIT_RTPRIO: int
        RLIMIT_RTTIME: int
        RLIMIT_SIGPENDING: int
        RUSAGE_THREAD: int

    @final
    class struct_rusage(
        structseq[float], tuple[float, float, int, int, int, int, int, int, int, int, int, int, int, int, int, int]
    ):
        """struct_rusage: Result from getrusage.

        This object may be accessed either as a tuple of
            (utime,stime,maxrss,ixrss,idrss,isrss,minflt,majflt,
            nswap,inblock,oublock,msgsnd,msgrcv,nsignals,nvcsw,nivcsw)
        or via the attributes ru_utime, ru_stime, ru_maxrss, and so on.
        """

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
        def ru_utime(self) -> float:
            """user time used"""

        @property
        def ru_stime(self) -> float:
            """system time used"""

        @property
        def ru_maxrss(self) -> int:
            """max. resident set size"""

        @property
        def ru_ixrss(self) -> int:
            """shared memory size"""

        @property
        def ru_idrss(self) -> int:
            """unshared data size"""

        @property
        def ru_isrss(self) -> int:
            """unshared stack size"""

        @property
        def ru_minflt(self) -> int:
            """page faults not requiring I/O"""

        @property
        def ru_majflt(self) -> int:
            """page faults requiring I/O"""

        @property
        def ru_nswap(self) -> int:
            """number of swap outs"""

        @property
        def ru_inblock(self) -> int:
            """block input operations"""

        @property
        def ru_oublock(self) -> int:
            """block output operations"""

        @property
        def ru_msgsnd(self) -> int:
            """IPC messages sent"""

        @property
        def ru_msgrcv(self) -> int:
            """IPC messages received"""

        @property
        def ru_nsignals(self) -> int:
            """signals received"""

        @property
        def ru_nvcsw(self) -> int:
            """voluntary context switches"""

        @property
        def ru_nivcsw(self) -> int:
            """involuntary context switches"""

    def getpagesize() -> int: ...
    def getrlimit(resource: int, /) -> tuple[int, int]: ...
    def getrusage(who: int, /) -> struct_rusage: ...
    def setrlimit(resource: int, limits: tuple[int, int], /) -> None: ...
    if sys.platform == "linux":
        if sys.version_info >= (3, 12):
            def prlimit(pid: int, resource: int, limits: tuple[int, int] | None = None, /) -> tuple[int, int]: ...
        else:
            def prlimit(pid: int, resource: int, limits: tuple[int, int] = ..., /) -> tuple[int, int]:
                """prlimit(pid, resource, [limits])"""
    error = OSError
