"""This module provides an interface to the Posix calls for tty I/O control.
For a complete description of these calls, see the Posix or Unix manual
pages. It is only available for those Unix versions that support Posix
termios style tty I/O control.

All functions in this module take a file descriptor fd as their first
argument. This can be an integer file descriptor, such as returned by
sys.stdin.fileno(), or a file object, such as sys.stdin itself.
"""

import sys
from _typeshed import FileDescriptorLike
from typing import Any, Final
from typing_extensions import TypeAlias

# Must be a list of length 7, containing 6 ints and a list of NCCS 1-character bytes or ints.
_Attr: TypeAlias = list[int | list[bytes | int]] | list[int | list[bytes]] | list[int | list[int]]
# Same as _Attr for return types; we use Any to avoid a union.
_AttrReturn: TypeAlias = list[Any]

if sys.platform != "win32":
    # Values depends on the platform
    B0: Final[int]
    B110: Final[int]
    B115200: Final[int]
    B1200: Final[int]
    B134: Final[int]
    B150: Final[int]
    B1800: Final[int]
    B19200: Final[int]
    B200: Final[int]
    B230400: Final[int]
    B2400: Final[int]
    B300: Final[int]
    B38400: Final[int]
    B4800: Final[int]
    B50: Final[int]
    B57600: Final[int]
    B600: Final[int]
    B75: Final[int]
    B9600: Final[int]
    BRKINT: Final[int]
    BS0: Final[int]
    BS1: Final[int]
    BSDLY: Final[int]
    CDSUSP: Final[int]
    CEOF: Final[int]
    CEOL: Final[int]
    CEOT: Final[int]
    CERASE: Final[int]
    CFLUSH: Final[int]
    CINTR: Final[int]
    CKILL: Final[int]
    CLNEXT: Final[int]
    CLOCAL: Final[int]
    CQUIT: Final[int]
    CR0: Final[int]
    CR1: Final[int]
    CR2: Final[int]
    CR3: Final[int]
    CRDLY: Final[int]
    CREAD: Final[int]
    CRPRNT: Final[int]
    CRTSCTS: Final[int]
    CS5: Final[int]
    CS6: Final[int]
    CS7: Final[int]
    CS8: Final[int]
    CSIZE: Final[int]
    CSTART: Final[int]
    CSTOP: Final[int]
    CSTOPB: Final[int]
    CSUSP: Final[int]
    CWERASE: Final[int]
    ECHO: Final[int]
    ECHOCTL: Final[int]
    ECHOE: Final[int]
    ECHOK: Final[int]
    ECHOKE: Final[int]
    ECHONL: Final[int]
    ECHOPRT: Final[int]
    EXTA: Final[int]
    EXTB: Final[int]
    FF0: Final[int]
    FF1: Final[int]
    FFDLY: Final[int]
    FIOASYNC: Final[int]
    FIOCLEX: Final[int]
    FIONBIO: Final[int]
    FIONCLEX: Final[int]
    FIONREAD: Final[int]
    FLUSHO: Final[int]
    HUPCL: Final[int]
    ICANON: Final[int]
    ICRNL: Final[int]
    IEXTEN: Final[int]
    IGNBRK: Final[int]
    IGNCR: Final[int]
    IGNPAR: Final[int]
    IMAXBEL: Final[int]
    INLCR: Final[int]
    INPCK: Final[int]
    ISIG: Final[int]
    ISTRIP: Final[int]
    IXANY: Final[int]
    IXOFF: Final[int]
    IXON: Final[int]
    NCCS: Final[int]
    NL0: Final[int]
    NL1: Final[int]
    NLDLY: Final[int]
    NOFLSH: Final[int]
    OCRNL: Final[int]
    OFDEL: Final[int]
    OFILL: Final[int]
    ONLCR: Final[int]
    ONLRET: Final[int]
    ONOCR: Final[int]
    OPOST: Final[int]
    PARENB: Final[int]
    PARMRK: Final[int]
    PARODD: Final[int]
    PENDIN: Final[int]
    TAB0: Final[int]
    TAB1: Final[int]
    TAB2: Final[int]
    TAB3: Final[int]
    TABDLY: Final[int]
    TCIFLUSH: Final[int]
    TCIOFF: Final[int]
    TCIOFLUSH: Final[int]
    TCION: Final[int]
    TCOFLUSH: Final[int]
    TCOOFF: Final[int]
    TCOON: Final[int]
    TCSADRAIN: Final[int]
    TCSAFLUSH: Final[int]
    TCSANOW: Final[int]
    TIOCCONS: Final[int]
    TIOCEXCL: Final[int]
    TIOCGETD: Final[int]
    TIOCGPGRP: Final[int]
    TIOCGWINSZ: Final[int]
    TIOCM_CAR: Final[int]
    TIOCM_CD: Final[int]
    TIOCM_CTS: Final[int]
    TIOCM_DSR: Final[int]
    TIOCM_DTR: Final[int]
    TIOCM_LE: Final[int]
    TIOCM_RI: Final[int]
    TIOCM_RNG: Final[int]
    TIOCM_RTS: Final[int]
    TIOCM_SR: Final[int]
    TIOCM_ST: Final[int]
    TIOCMBIC: Final[int]
    TIOCMBIS: Final[int]
    TIOCMGET: Final[int]
    TIOCMSET: Final[int]
    TIOCNOTTY: Final[int]
    TIOCNXCL: Final[int]
    TIOCOUTQ: Final[int]
    TIOCPKT_DATA: Final[int]
    TIOCPKT_DOSTOP: Final[int]
    TIOCPKT_FLUSHREAD: Final[int]
    TIOCPKT_FLUSHWRITE: Final[int]
    TIOCPKT_NOSTOP: Final[int]
    TIOCPKT_START: Final[int]
    TIOCPKT_STOP: Final[int]
    TIOCPKT: Final[int]
    TIOCSCTTY: Final[int]
    TIOCSETD: Final[int]
    TIOCSPGRP: Final[int]
    TIOCSTI: Final[int]
    TIOCSWINSZ: Final[int]
    TOSTOP: Final[int]
    VDISCARD: Final[int]
    VEOF: Final[int]
    VEOL: Final[int]
    VEOL2: Final[int]
    VERASE: Final[int]
    VINTR: Final[int]
    VKILL: Final[int]
    VLNEXT: Final[int]
    VMIN: Final[int]
    VQUIT: Final[int]
    VREPRINT: Final[int]
    VSTART: Final[int]
    VSTOP: Final[int]
    VSUSP: Final[int]
    VT0: Final[int]
    VT1: Final[int]
    VTDLY: Final[int]
    VTIME: Final[int]
    VWERASE: Final[int]

    if sys.version_info >= (3, 13):
        EXTPROC: Final[int]
        IUTF8: Final[int]

    if sys.platform == "darwin" and sys.version_info >= (3, 13):
        ALTWERASE: Final[int]
        B14400: Final[int]
        B28800: Final[int]
        B7200: Final[int]
        B76800: Final[int]
        CCAR_OFLOW: Final[int]
        CCTS_OFLOW: Final[int]
        CDSR_OFLOW: Final[int]
        CDTR_IFLOW: Final[int]
        CIGNORE: Final[int]
        CRTS_IFLOW: Final[int]
        MDMBUF: Final[int]
        NL2: Final[int]
        NL3: Final[int]
        NOKERNINFO: Final[int]
        ONOEOT: Final[int]
        OXTABS: Final[int]
        VDSUSP: Final[int]
        VSTATUS: Final[int]

    if sys.platform == "darwin" and sys.version_info >= (3, 11):
        TIOCGSIZE: Final[int]
        TIOCSSIZE: Final[int]

    if sys.platform == "linux":
        B1152000: Final[int]
        B576000: Final[int]
        CBAUD: Final[int]
        CBAUDEX: Final[int]
        CIBAUD: Final[int]
        IOCSIZE_MASK: Final[int]
        IOCSIZE_SHIFT: Final[int]
        IUCLC: Final[int]
        N_MOUSE: Final[int]
        N_PPP: Final[int]
        N_SLIP: Final[int]
        N_STRIP: Final[int]
        N_TTY: Final[int]
        NCC: Final[int]
        OLCUC: Final[int]
        TCFLSH: Final[int]
        TCGETA: Final[int]
        TCGETS: Final[int]
        TCSBRK: Final[int]
        TCSBRKP: Final[int]
        TCSETA: Final[int]
        TCSETAF: Final[int]
        TCSETAW: Final[int]
        TCSETS: Final[int]
        TCSETSF: Final[int]
        TCSETSW: Final[int]
        TCXONC: Final[int]
        TIOCGICOUNT: Final[int]
        TIOCGLCKTRMIOS: Final[int]
        TIOCGSERIAL: Final[int]
        TIOCGSOFTCAR: Final[int]
        TIOCINQ: Final[int]
        TIOCLINUX: Final[int]
        TIOCMIWAIT: Final[int]
        TIOCTTYGSTRUCT: Final[int]
        TIOCSER_TEMT: Final[int]
        TIOCSERCONFIG: Final[int]
        TIOCSERGETLSR: Final[int]
        TIOCSERGETMULTI: Final[int]
        TIOCSERGSTRUCT: Final[int]
        TIOCSERGWILD: Final[int]
        TIOCSERSETMULTI: Final[int]
        TIOCSERSWILD: Final[int]
        TIOCSLCKTRMIOS: Final[int]
        TIOCSSERIAL: Final[int]
        TIOCSSOFTCAR: Final[int]
        VSWTC: Final[int]
        VSWTCH: Final[int]
        XCASE: Final[int]
        XTABS: Final[int]

    if sys.platform != "darwin":
        B1000000: Final[int]
        B1500000: Final[int]
        B2000000: Final[int]
        B2500000: Final[int]
        B3000000: Final[int]
        B3500000: Final[int]
        B4000000: Final[int]
        B460800: Final[int]
        B500000: Final[int]
        B921600: Final[int]

    if sys.platform != "linux":
        TCSASOFT: Final[int]

    if sys.platform != "darwin" and sys.platform != "linux":
        # not available on FreeBSD either.
        CDEL: Final[int]
        CEOL2: Final[int]
        CESC: Final[int]
        CNUL: Final[int]
        COMMON: Final[int]
        CSWTCH: Final[int]
        IBSHIFT: Final[int]
        INIT_C_CC: Final[int]
        NSWTCH: Final[int]

    def tcgetattr(fd: FileDescriptorLike, /) -> _AttrReturn:
        """Get the tty attributes for file descriptor fd.

        Returns a list [iflag, oflag, cflag, lflag, ispeed, ospeed, cc]
        where cc is a list of the tty special characters (each a string of
        length 1, except the items with indices VMIN and VTIME, which are
        integers when these fields are defined).  The interpretation of the
        flags and the speeds as well as the indexing in the cc array must be
        done using the symbolic constants defined in this module.
        """

    def tcsetattr(fd: FileDescriptorLike, when: int, attributes: _Attr, /) -> None:
        """Set the tty attributes for file descriptor fd.

        The attributes to be set are taken from the attributes argument, which
        is a list like the one returned by tcgetattr(). The when argument
        determines when the attributes are changed: termios.TCSANOW to
        change immediately, termios.TCSADRAIN to change after transmitting all
        queued output, or termios.TCSAFLUSH to change after transmitting all
        queued output and discarding all queued input.
        """

    def tcsendbreak(fd: FileDescriptorLike, duration: int, /) -> None:
        """Send a break on file descriptor fd.

        A zero duration sends a break for 0.25-0.5 seconds; a nonzero duration
        has a system dependent meaning.
        """

    def tcdrain(fd: FileDescriptorLike, /) -> None:
        """Wait until all output written to file descriptor fd has been transmitted."""

    def tcflush(fd: FileDescriptorLike, queue: int, /) -> None:
        """Discard queued data on file descriptor fd.

        The queue selector specifies which queue: termios.TCIFLUSH for the input
        queue, termios.TCOFLUSH for the output queue, or termios.TCIOFLUSH for
        both queues.
        """

    def tcflow(fd: FileDescriptorLike, action: int, /) -> None:
        """Suspend or resume input or output on file descriptor fd.

        The action argument can be termios.TCOOFF to suspend output,
        termios.TCOON to restart output, termios.TCIOFF to suspend input,
        or termios.TCION to restart input.
        """
    if sys.version_info >= (3, 11):
        def tcgetwinsize(fd: FileDescriptorLike, /) -> tuple[int, int]:
            """Get the tty winsize for file descriptor fd.

            Returns a tuple (ws_row, ws_col).
            """

        def tcsetwinsize(fd: FileDescriptorLike, winsize: tuple[int, int], /) -> None:
            """Set the tty winsize for file descriptor fd.

            The winsize to be set is taken from the winsize argument, which
            is a two-item tuple (ws_row, ws_col) like the one returned by tcgetwinsize().
            """

    class error(Exception): ...
