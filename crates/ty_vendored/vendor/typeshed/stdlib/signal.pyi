import sys
from _typeshed import structseq
from collections.abc import Callable, Iterable
from enum import IntEnum
from types import FrameType
from typing import Any, Final, final
from typing_extensions import Never, TypeAlias

NSIG: int

class Signals(IntEnum):
    """An enumeration."""

    SIGABRT = 6
    SIGFPE = 8
    SIGILL = 4
    SIGINT = 2
    SIGSEGV = 11
    SIGTERM = 15

    if sys.platform == "win32":
        SIGBREAK = 21
        CTRL_C_EVENT = 0
        CTRL_BREAK_EVENT = 1
    else:
        SIGALRM = 14
        SIGBUS = 7
        SIGCHLD = 17
        SIGCONT = 18
        SIGHUP = 1
        SIGIO = 29
        SIGIOT = 6
        SIGKILL = 9
        SIGPIPE = 13
        SIGPROF = 27
        SIGQUIT = 3
        SIGSTOP = 19
        SIGSYS = 31
        SIGTRAP = 5
        SIGTSTP = 20
        SIGTTIN = 21
        SIGTTOU = 22
        SIGURG = 23
        SIGUSR1 = 10
        SIGUSR2 = 12
        SIGVTALRM = 26
        SIGWINCH = 28
        SIGXCPU = 24
        SIGXFSZ = 25
        if sys.platform != "linux":
            SIGEMT = 7
            SIGINFO = 29
        if sys.platform != "darwin":
            SIGCLD = 17
            SIGPOLL = 29
            SIGPWR = 30
            SIGRTMAX = 64
            SIGRTMIN = 34
            if sys.version_info >= (3, 11):
                SIGSTKFLT = 16

class Handlers(IntEnum):
    """An enumeration."""

    SIG_DFL = 0
    SIG_IGN = 1

SIG_DFL: Final = Handlers.SIG_DFL
SIG_IGN: Final = Handlers.SIG_IGN

_SIGNUM: TypeAlias = int | Signals
_HANDLER: TypeAlias = Callable[[int, FrameType | None], Any] | int | Handlers | None

def default_int_handler(signalnum: int, frame: FrameType | None, /) -> Never:
    """The default handler for SIGINT installed by Python.

    It raises KeyboardInterrupt.
    """

if sys.version_info >= (3, 10):  # arguments changed in 3.10.2
    def getsignal(signalnum: _SIGNUM) -> _HANDLER:
        """Return the current action for the given signal.

        The return value can be:
          SIG_IGN -- if the signal is being ignored
          SIG_DFL -- if the default action for the signal is in effect
          None    -- if an unknown handler is in effect
          anything else -- the callable Python object used as a handler
        """

    def signal(signalnum: _SIGNUM, handler: _HANDLER) -> _HANDLER:
        """Set the action for the given signal.

        The action can be SIG_DFL, SIG_IGN, or a callable Python object.
        The previous action is returned.  See getsignal() for possible return values.

        *** IMPORTANT NOTICE ***
        A signal handler function is called with two arguments:
        the first is the signal number, the second is the interrupted stack frame.
        """

else:
    def getsignal(signalnum: _SIGNUM, /) -> _HANDLER:
        """Return the current action for the given signal.

        The return value can be:
          SIG_IGN -- if the signal is being ignored
          SIG_DFL -- if the default action for the signal is in effect
          None    -- if an unknown handler is in effect
          anything else -- the callable Python object used as a handler
        """

    def signal(signalnum: _SIGNUM, handler: _HANDLER, /) -> _HANDLER:
        """Set the action for the given signal.

        The action can be SIG_DFL, SIG_IGN, or a callable Python object.
        The previous action is returned.  See getsignal() for possible return values.

        *** IMPORTANT NOTICE ***
        A signal handler function is called with two arguments:
        the first is the signal number, the second is the interrupted stack frame.
        """

SIGABRT: Final = Signals.SIGABRT
SIGFPE: Final = Signals.SIGFPE
SIGILL: Final = Signals.SIGILL
SIGINT: Final = Signals.SIGINT
SIGSEGV: Final = Signals.SIGSEGV
SIGTERM: Final = Signals.SIGTERM

if sys.platform == "win32":
    SIGBREAK: Final = Signals.SIGBREAK
    CTRL_C_EVENT: Final = Signals.CTRL_C_EVENT
    CTRL_BREAK_EVENT: Final = Signals.CTRL_BREAK_EVENT
else:
    if sys.platform != "linux":
        SIGINFO: Final = Signals.SIGINFO
        SIGEMT: Final = Signals.SIGEMT
    SIGALRM: Final = Signals.SIGALRM
    SIGBUS: Final = Signals.SIGBUS
    SIGCHLD: Final = Signals.SIGCHLD
    SIGCONT: Final = Signals.SIGCONT
    SIGHUP: Final = Signals.SIGHUP
    SIGIO: Final = Signals.SIGIO
    SIGIOT: Final = Signals.SIGABRT  # alias
    SIGKILL: Final = Signals.SIGKILL
    SIGPIPE: Final = Signals.SIGPIPE
    SIGPROF: Final = Signals.SIGPROF
    SIGQUIT: Final = Signals.SIGQUIT
    SIGSTOP: Final = Signals.SIGSTOP
    SIGSYS: Final = Signals.SIGSYS
    SIGTRAP: Final = Signals.SIGTRAP
    SIGTSTP: Final = Signals.SIGTSTP
    SIGTTIN: Final = Signals.SIGTTIN
    SIGTTOU: Final = Signals.SIGTTOU
    SIGURG: Final = Signals.SIGURG
    SIGUSR1: Final = Signals.SIGUSR1
    SIGUSR2: Final = Signals.SIGUSR2
    SIGVTALRM: Final = Signals.SIGVTALRM
    SIGWINCH: Final = Signals.SIGWINCH
    SIGXCPU: Final = Signals.SIGXCPU
    SIGXFSZ: Final = Signals.SIGXFSZ

    class ItimerError(OSError): ...
    ITIMER_PROF: int
    ITIMER_REAL: int
    ITIMER_VIRTUAL: int

    class Sigmasks(IntEnum):
        """An enumeration."""

        SIG_BLOCK = 0
        SIG_UNBLOCK = 1
        SIG_SETMASK = 2

    SIG_BLOCK: Final = Sigmasks.SIG_BLOCK
    SIG_UNBLOCK: Final = Sigmasks.SIG_UNBLOCK
    SIG_SETMASK: Final = Sigmasks.SIG_SETMASK
    def alarm(seconds: int, /) -> int:
        """Arrange for SIGALRM to arrive after the given number of seconds."""

    def getitimer(which: int, /) -> tuple[float, float]:
        """Returns current value of given itimer."""

    def pause() -> None:
        """Wait until a signal arrives."""

    def pthread_kill(thread_id: int, signalnum: int, /) -> None:
        """Send a signal to a thread."""
    if sys.version_info >= (3, 10):  # arguments changed in 3.10.2
        def pthread_sigmask(how: int, mask: Iterable[int]) -> set[_SIGNUM]:
            """Fetch and/or change the signal mask of the calling thread."""
    else:
        def pthread_sigmask(how: int, mask: Iterable[int], /) -> set[_SIGNUM]:
            """Fetch and/or change the signal mask of the calling thread."""

    def setitimer(which: int, seconds: float, interval: float = 0.0, /) -> tuple[float, float]:
        """Sets given itimer (one of ITIMER_REAL, ITIMER_VIRTUAL or ITIMER_PROF).

        The timer will fire after value seconds and after that every interval seconds.
        The itimer can be cleared by setting seconds to zero.

        Returns old values as a tuple: (delay, interval).
        """

    def siginterrupt(signalnum: int, flag: bool, /) -> None:
        """Change system call restart behaviour.

        If flag is False, system calls will be restarted when interrupted by
        signal sig, else system calls will be interrupted.
        """

    def sigpending() -> Any:
        """Examine pending signals.

        Returns a set of signal numbers that are pending for delivery to
        the calling thread.
        """
    if sys.version_info >= (3, 10):  # argument changed in 3.10.2
        def sigwait(sigset: Iterable[int]) -> _SIGNUM:
            """Wait for a signal.

            Suspend execution of the calling thread until the delivery of one of the
            signals specified in the signal set sigset.  The function accepts the signal
            and returns the signal number.
            """
    else:
        def sigwait(sigset: Iterable[int], /) -> _SIGNUM:
            """Wait for a signal.

            Suspend execution of the calling thread until the delivery of one of the
            signals specified in the signal set sigset.  The function accepts the signal
            and returns the signal number.
            """
    if sys.platform != "darwin":
        SIGCLD: Final = Signals.SIGCHLD  # alias
        SIGPOLL: Final = Signals.SIGIO  # alias
        SIGPWR: Final = Signals.SIGPWR
        SIGRTMAX: Final = Signals.SIGRTMAX
        SIGRTMIN: Final = Signals.SIGRTMIN
        if sys.version_info >= (3, 11):
            SIGSTKFLT: Final = Signals.SIGSTKFLT

        @final
        class struct_siginfo(structseq[int], tuple[int, int, int, int, int, int, int]):
            """struct_siginfo: Result from sigwaitinfo or sigtimedwait.

            This object may be accessed either as a tuple of
            (si_signo, si_code, si_errno, si_pid, si_uid, si_status, si_band),
            or via the attributes si_signo, si_code, and so on.
            """

            if sys.version_info >= (3, 10):
                __match_args__: Final = ("si_signo", "si_code", "si_errno", "si_pid", "si_uid", "si_status", "si_band")

            @property
            def si_signo(self) -> int:
                """signal number"""

            @property
            def si_code(self) -> int:
                """signal code"""

            @property
            def si_errno(self) -> int:
                """errno associated with this signal"""

            @property
            def si_pid(self) -> int:
                """sending process ID"""

            @property
            def si_uid(self) -> int:
                """real user ID of sending process"""

            @property
            def si_status(self) -> int:
                """exit value or signal"""

            @property
            def si_band(self) -> int:
                """band event for SIGPOLL"""

        def sigtimedwait(sigset: Iterable[int], timeout: float, /) -> struct_siginfo | None:
            """Like sigwaitinfo(), but with a timeout.

            The timeout is specified in seconds, with floating-point numbers allowed.
            """

        def sigwaitinfo(sigset: Iterable[int], /) -> struct_siginfo:
            """Wait synchronously until one of the signals in *sigset* is delivered.

            Returns a struct_siginfo containing information about the signal.
            """

def strsignal(signalnum: _SIGNUM, /) -> str | None:
    """Return the system description of the given signal.

    Returns the description of signal *signalnum*, such as "Interrupt"
    for :const:`SIGINT`. Returns :const:`None` if *signalnum* has no
    description. Raises :exc:`ValueError` if *signalnum* is invalid.
    """

def valid_signals() -> set[Signals]:
    """Return a set of valid signal numbers on this platform.

    The signal numbers returned by this function can be safely passed to
    functions like `pthread_sigmask`.
    """

def raise_signal(signalnum: _SIGNUM, /) -> None:
    """Send a signal to the executing process."""

def set_wakeup_fd(fd: int, /, *, warn_on_full_buffer: bool = True) -> int:
    """Sets the fd to be written to (with the signal number) when a signal comes in.

    A library can use this to wakeup select or poll.
    The previous fd or -1 is returned.

    The fd must be non-blocking.
    """

if sys.platform == "linux":
    def pidfd_send_signal(pidfd: int, sig: int, siginfo: None = None, flags: int = 0, /) -> None:
        """Send a signal to a process referred to by a pid file descriptor."""
