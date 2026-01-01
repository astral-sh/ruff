import socket
from collections.abc import Callable, MutableSequence, Sequence
from re import Match, Pattern
from types import TracebackType
from typing import Any, Final
from typing_extensions import Self

__all__ = ["Telnet"]

DEBUGLEVEL: Final = 0
TELNET_PORT: Final = 23

IAC: Final = b"\xff"
DONT: Final = b"\xfe"
DO: Final = b"\xfd"
WONT: Final = b"\xfc"
WILL: Final = b"\xfb"
theNULL: Final = b"\x00"

SE: Final = b"\xf0"
NOP: Final = b"\xf1"
DM: Final = b"\xf2"
BRK: Final = b"\xf3"
IP: Final = b"\xf4"
AO: Final = b"\xf5"
AYT: Final = b"\xf6"
EC: Final = b"\xf7"
EL: Final = b"\xf8"
GA: Final = b"\xf9"
SB: Final = b"\xfa"

BINARY: Final = b"\x00"
ECHO: Final = b"\x01"
RCP: Final = b"\x02"
SGA: Final = b"\x03"
NAMS: Final = b"\x04"
STATUS: Final = b"\x05"
TM: Final = b"\x06"
RCTE: Final = b"\x07"
NAOL: Final = b"\x08"
NAOP: Final = b"\t"
NAOCRD: Final = b"\n"
NAOHTS: Final = b"\x0b"
NAOHTD: Final = b"\x0c"
NAOFFD: Final = b"\r"
NAOVTS: Final = b"\x0e"
NAOVTD: Final = b"\x0f"
NAOLFD: Final = b"\x10"
XASCII: Final = b"\x11"
LOGOUT: Final = b"\x12"
BM: Final = b"\x13"
DET: Final = b"\x14"
SUPDUP: Final = b"\x15"
SUPDUPOUTPUT: Final = b"\x16"
SNDLOC: Final = b"\x17"
TTYPE: Final = b"\x18"
EOR: Final = b"\x19"
TUID: Final = b"\x1a"
OUTMRK: Final = b"\x1b"
TTYLOC: Final = b"\x1c"
VT3270REGIME: Final = b"\x1d"
X3PAD: Final = b"\x1e"
NAWS: Final = b"\x1f"
TSPEED: Final = b" "
LFLOW: Final = b"!"
LINEMODE: Final = b'"'
XDISPLOC: Final = b"#"
OLD_ENVIRON: Final = b"$"
AUTHENTICATION: Final = b"%"
ENCRYPT: Final = b"&"
NEW_ENVIRON: Final = b"'"

TN3270E: Final = b"("
XAUTH: Final = b")"
CHARSET: Final = b"*"
RSP: Final = b"+"
COM_PORT_OPTION: Final = b","
SUPPRESS_LOCAL_ECHO: Final = b"-"
TLS: Final = b"."
KERMIT: Final = b"/"
SEND_URL: Final = b"0"
FORWARD_X: Final = b"1"
PRAGMA_LOGON: Final = b"\x8a"
SSPI_LOGON: Final = b"\x8b"
PRAGMA_HEARTBEAT: Final = b"\x8c"
EXOPL: Final = b"\xff"
NOOPT: Final = b"\x00"

class Telnet:
    host: str | None  # undocumented
    sock: socket.socket | None  # undocumented
    def __init__(self, host: str | None = None, port: int = 0, timeout: float = ...) -> None: ...
    def open(self, host: str, port: int = 0, timeout: float = ...) -> None: ...
    def msg(self, msg: str, *args: Any) -> None: ...
    def set_debuglevel(self, debuglevel: int) -> None: ...
    def close(self) -> None: ...
    def get_socket(self) -> socket.socket: ...
    def fileno(self) -> int: ...
    def write(self, buffer: bytes) -> None: ...
    def read_until(self, match: bytes, timeout: float | None = None) -> bytes: ...
    def read_all(self) -> bytes: ...
    def read_some(self) -> bytes: ...
    def read_very_eager(self) -> bytes: ...
    def read_eager(self) -> bytes: ...
    def read_lazy(self) -> bytes: ...
    def read_very_lazy(self) -> bytes: ...
    def read_sb_data(self) -> bytes: ...
    def set_option_negotiation_callback(self, callback: Callable[[socket.socket, bytes, bytes], object] | None) -> None: ...
    def process_rawq(self) -> None: ...
    def rawq_getchar(self) -> bytes: ...
    def fill_rawq(self) -> None: ...
    def sock_avail(self) -> bool: ...
    def interact(self) -> None: ...
    def mt_interact(self) -> None: ...
    def listener(self) -> None: ...
    def expect(
        self, list: MutableSequence[Pattern[bytes] | bytes] | Sequence[Pattern[bytes]], timeout: float | None = None
    ) -> tuple[int, Match[bytes] | None, bytes]: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __del__(self) -> None: ...
