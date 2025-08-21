import enum
import sys
from typing import Final

LOG_THRESHOLD_FOR_CONNLOST_WRITES: Final = 5
ACCEPT_RETRY_DELAY: Final = 1
DEBUG_STACK_DEPTH: Final = 10
SSL_HANDSHAKE_TIMEOUT: float
SENDFILE_FALLBACK_READBUFFER_SIZE: Final = 262144
if sys.version_info >= (3, 11):
    SSL_SHUTDOWN_TIMEOUT: float
    FLOW_CONTROL_HIGH_WATER_SSL_READ: Final = 256
    FLOW_CONTROL_HIGH_WATER_SSL_WRITE: Final = 512
if sys.version_info >= (3, 12):
    THREAD_JOIN_TIMEOUT: Final = 300

class _SendfileMode(enum.Enum):
    UNSUPPORTED = 1
    TRY_NATIVE = 2
    FALLBACK = 3
