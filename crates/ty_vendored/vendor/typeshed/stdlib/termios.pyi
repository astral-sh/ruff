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
from typing import Any
from typing_extensions import TypeAlias

# Must be a list of length 7, containing 6 ints and a list of NCCS 1-character bytes or ints.
_Attr: TypeAlias = list[int | list[bytes | int]] | list[int | list[bytes]] | list[int | list[int]]
# Same as _Attr for return types; we use Any to avoid a union.
_AttrReturn: TypeAlias = list[Any]

if sys.platform != "win32":
    B0: int
    B110: int
    B115200: int
    B1200: int
    B134: int
    B150: int
    B1800: int
    B19200: int
    B200: int
    B230400: int
    B2400: int
    B300: int
    B38400: int
    B4800: int
    B50: int
    B57600: int
    B600: int
    B75: int
    B9600: int
    BRKINT: int
    BS0: int
    BS1: int
    BSDLY: int
    CDSUSP: int
    CEOF: int
    CEOL: int
    CEOT: int
    CERASE: int
    CFLUSH: int
    CINTR: int
    CKILL: int
    CLNEXT: int
    CLOCAL: int
    CQUIT: int
    CR0: int
    CR1: int
    CR2: int
    CR3: int
    CRDLY: int
    CREAD: int
    CRPRNT: int
    CRTSCTS: int
    CS5: int
    CS6: int
    CS7: int
    CS8: int
    CSIZE: int
    CSTART: int
    CSTOP: int
    CSTOPB: int
    CSUSP: int
    CWERASE: int
    ECHO: int
    ECHOCTL: int
    ECHOE: int
    ECHOK: int
    ECHOKE: int
    ECHONL: int
    ECHOPRT: int
    EXTA: int
    EXTB: int
    FF0: int
    FF1: int
    FFDLY: int
    FIOASYNC: int
    FIOCLEX: int
    FIONBIO: int
    FIONCLEX: int
    FIONREAD: int
    FLUSHO: int
    HUPCL: int
    ICANON: int
    ICRNL: int
    IEXTEN: int
    IGNBRK: int
    IGNCR: int
    IGNPAR: int
    IMAXBEL: int
    INLCR: int
    INPCK: int
    ISIG: int
    ISTRIP: int
    IXANY: int
    IXOFF: int
    IXON: int
    NCCS: int
    NL0: int
    NL1: int
    NLDLY: int
    NOFLSH: int
    OCRNL: int
    OFDEL: int
    OFILL: int
    ONLCR: int
    ONLRET: int
    ONOCR: int
    OPOST: int
    PARENB: int
    PARMRK: int
    PARODD: int
    PENDIN: int
    TAB0: int
    TAB1: int
    TAB2: int
    TAB3: int
    TABDLY: int
    TCIFLUSH: int
    TCIOFF: int
    TCIOFLUSH: int
    TCION: int
    TCOFLUSH: int
    TCOOFF: int
    TCOON: int
    TCSADRAIN: int
    TCSAFLUSH: int
    TCSANOW: int
    TIOCCONS: int
    TIOCEXCL: int
    TIOCGETD: int
    TIOCGPGRP: int
    TIOCGWINSZ: int
    TIOCM_CAR: int
    TIOCM_CD: int
    TIOCM_CTS: int
    TIOCM_DSR: int
    TIOCM_DTR: int
    TIOCM_LE: int
    TIOCM_RI: int
    TIOCM_RNG: int
    TIOCM_RTS: int
    TIOCM_SR: int
    TIOCM_ST: int
    TIOCMBIC: int
    TIOCMBIS: int
    TIOCMGET: int
    TIOCMSET: int
    TIOCNOTTY: int
    TIOCNXCL: int
    TIOCOUTQ: int
    TIOCPKT_DATA: int
    TIOCPKT_DOSTOP: int
    TIOCPKT_FLUSHREAD: int
    TIOCPKT_FLUSHWRITE: int
    TIOCPKT_NOSTOP: int
    TIOCPKT_START: int
    TIOCPKT_STOP: int
    TIOCPKT: int
    TIOCSCTTY: int
    TIOCSETD: int
    TIOCSPGRP: int
    TIOCSTI: int
    TIOCSWINSZ: int
    TOSTOP: int
    VDISCARD: int
    VEOF: int
    VEOL: int
    VEOL2: int
    VERASE: int
    VINTR: int
    VKILL: int
    VLNEXT: int
    VMIN: int
    VQUIT: int
    VREPRINT: int
    VSTART: int
    VSTOP: int
    VSUSP: int
    VT0: int
    VT1: int
    VTDLY: int
    VTIME: int
    VWERASE: int

    if sys.version_info >= (3, 13):
        EXTPROC: int
        IUTF8: int

    if sys.platform == "darwin" and sys.version_info >= (3, 13):
        ALTWERASE: int
        B14400: int
        B28800: int
        B7200: int
        B76800: int
        CCAR_OFLOW: int
        CCTS_OFLOW: int
        CDSR_OFLOW: int
        CDTR_IFLOW: int
        CIGNORE: int
        CRTS_IFLOW: int
        MDMBUF: int
        NL2: int
        NL3: int
        NOKERNINFO: int
        ONOEOT: int
        OXTABS: int
        VDSUSP: int
        VSTATUS: int

    if sys.platform == "darwin" and sys.version_info >= (3, 11):
        TIOCGSIZE: int
        TIOCSSIZE: int

    if sys.platform == "linux":
        B1152000: int
        B576000: int
        CBAUD: int
        CBAUDEX: int
        CIBAUD: int
        IOCSIZE_MASK: int
        IOCSIZE_SHIFT: int
        IUCLC: int
        N_MOUSE: int
        N_PPP: int
        N_SLIP: int
        N_STRIP: int
        N_TTY: int
        NCC: int
        OLCUC: int
        TCFLSH: int
        TCGETA: int
        TCGETS: int
        TCSBRK: int
        TCSBRKP: int
        TCSETA: int
        TCSETAF: int
        TCSETAW: int
        TCSETS: int
        TCSETSF: int
        TCSETSW: int
        TCXONC: int
        TIOCGICOUNT: int
        TIOCGLCKTRMIOS: int
        TIOCGSERIAL: int
        TIOCGSOFTCAR: int
        TIOCINQ: int
        TIOCLINUX: int
        TIOCMIWAIT: int
        TIOCTTYGSTRUCT: int
        TIOCSER_TEMT: int
        TIOCSERCONFIG: int
        TIOCSERGETLSR: int
        TIOCSERGETMULTI: int
        TIOCSERGSTRUCT: int
        TIOCSERGWILD: int
        TIOCSERSETMULTI: int
        TIOCSERSWILD: int
        TIOCSLCKTRMIOS: int
        TIOCSSERIAL: int
        TIOCSSOFTCAR: int
        VSWTC: int
        VSWTCH: int
        XCASE: int
        XTABS: int

    if sys.platform != "darwin":
        B1000000: int
        B1500000: int
        B2000000: int
        B2500000: int
        B3000000: int
        B3500000: int
        B4000000: int
        B460800: int
        B500000: int
        B921600: int

    if sys.platform != "linux":
        TCSASOFT: int

    if sys.platform != "darwin" and sys.platform != "linux":
        # not available on FreeBSD either.
        CDEL: int
        CEOL2: int
        CESC: int
        CNUL: int
        COMMON: int
        CSWTCH: int
        IBSHIFT: int
        INIT_C_CC: int
        NSWTCH: int

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
