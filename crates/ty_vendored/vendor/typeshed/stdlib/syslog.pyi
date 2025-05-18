import sys
from typing import Final, overload

if sys.platform != "win32":
    LOG_ALERT: Final = 1
    LOG_AUTH: Final = 32
    LOG_AUTHPRIV: Final = 80
    LOG_CONS: Final = 2
    LOG_CRIT: Final = 2
    LOG_CRON: Final = 72
    LOG_DAEMON: Final = 24
    LOG_DEBUG: Final = 7
    LOG_EMERG: Final = 0
    LOG_ERR: Final = 3
    LOG_INFO: Final = 6
    LOG_KERN: Final = 0
    LOG_LOCAL0: Final = 128
    LOG_LOCAL1: Final = 136
    LOG_LOCAL2: Final = 144
    LOG_LOCAL3: Final = 152
    LOG_LOCAL4: Final = 160
    LOG_LOCAL5: Final = 168
    LOG_LOCAL6: Final = 176
    LOG_LOCAL7: Final = 184
    LOG_LPR: Final = 48
    LOG_MAIL: Final = 16
    LOG_NDELAY: Final = 8
    LOG_NEWS: Final = 56
    LOG_NOTICE: Final = 5
    LOG_NOWAIT: Final = 16
    LOG_ODELAY: Final = 4
    LOG_PERROR: Final = 32
    LOG_PID: Final = 1
    LOG_SYSLOG: Final = 40
    LOG_USER: Final = 8
    LOG_UUCP: Final = 64
    LOG_WARNING: Final = 4

    if sys.version_info >= (3, 13):
        LOG_FTP: Final = 88

        if sys.platform == "darwin":
            LOG_INSTALL: Final = 112
            LOG_LAUNCHD: Final = 192
            LOG_NETINFO: Final = 96
            LOG_RAS: Final = 120
            LOG_REMOTEAUTH: Final = 104

    def LOG_MASK(pri: int, /) -> int: ...
    def LOG_UPTO(pri: int, /) -> int: ...
    def closelog() -> None: ...
    def openlog(ident: str = ..., logoption: int = ..., facility: int = ...) -> None: ...
    def setlogmask(maskpri: int, /) -> int: ...
    @overload
    def syslog(priority: int, message: str) -> None: ...
    @overload
    def syslog(message: str) -> None: ...
