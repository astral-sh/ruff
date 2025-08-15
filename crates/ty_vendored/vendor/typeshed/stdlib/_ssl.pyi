import sys
from _typeshed import ReadableBuffer, StrOrBytesPath
from collections.abc import Callable
from ssl import (
    SSLCertVerificationError as SSLCertVerificationError,
    SSLContext,
    SSLEOFError as SSLEOFError,
    SSLError as SSLError,
    SSLObject,
    SSLSyscallError as SSLSyscallError,
    SSLWantReadError as SSLWantReadError,
    SSLWantWriteError as SSLWantWriteError,
    SSLZeroReturnError as SSLZeroReturnError,
)
from typing import Any, ClassVar, Final, Literal, TypedDict, final, overload, type_check_only
from typing_extensions import NotRequired, Self, TypeAlias, deprecated

_PasswordType: TypeAlias = Callable[[], str | bytes | bytearray] | str | bytes | bytearray
_PCTRTT: TypeAlias = tuple[tuple[str, str], ...]
_PCTRTTT: TypeAlias = tuple[_PCTRTT, ...]
_PeerCertRetDictType: TypeAlias = dict[str, str | _PCTRTTT | _PCTRTT]

@type_check_only
class _Cipher(TypedDict):
    aead: bool
    alg_bits: int
    auth: str
    description: str
    digest: str | None
    id: int
    kea: str
    name: str
    protocol: str
    strength_bits: int
    symmetric: str

@type_check_only
class _CertInfo(TypedDict):
    subject: tuple[tuple[tuple[str, str], ...], ...]
    issuer: tuple[tuple[tuple[str, str], ...], ...]
    version: int
    serialNumber: str
    notBefore: str
    notAfter: str
    subjectAltName: NotRequired[tuple[tuple[str, str], ...] | None]
    OCSP: NotRequired[tuple[str, ...] | None]
    caIssuers: NotRequired[tuple[str, ...] | None]
    crlDistributionPoints: NotRequired[tuple[str, ...] | None]

def RAND_add(string: str | ReadableBuffer, entropy: float, /) -> None: ...
def RAND_bytes(n: int, /) -> bytes: ...

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.6; removed in Python 3.12. Use `ssl.RAND_bytes()` instead.")
    def RAND_pseudo_bytes(n: int, /) -> tuple[bytes, bool]: ...

if sys.version_info < (3, 10):
    def RAND_egd(path: str) -> None: ...

def RAND_status() -> bool: ...
def get_default_verify_paths() -> tuple[str, str, str, str]: ...

if sys.platform == "win32":
    _EnumRetType: TypeAlias = list[tuple[bytes, str, set[str] | bool]]
    def enum_certificates(store_name: str) -> _EnumRetType: ...
    def enum_crls(store_name: str) -> _EnumRetType: ...

def txt2obj(txt: str, name: bool = False) -> tuple[int, str, str, str]: ...
def nid2obj(nid: int, /) -> tuple[int, str, str, str]: ...

class _SSLContext:
    check_hostname: bool
    keylog_filename: str | None
    maximum_version: int
    minimum_version: int
    num_tickets: int
    options: int
    post_handshake_auth: bool
    protocol: int
    if sys.version_info >= (3, 10):
        security_level: int
    sni_callback: Callable[[SSLObject, str, SSLContext], None | int] | None
    verify_flags: int
    verify_mode: int
    def __new__(cls, protocol: int, /) -> Self: ...
    def cert_store_stats(self) -> dict[str, int]: ...
    @overload
    def get_ca_certs(self, binary_form: Literal[False] = False) -> list[_PeerCertRetDictType]: ...
    @overload
    def get_ca_certs(self, binary_form: Literal[True]) -> list[bytes]: ...
    @overload
    def get_ca_certs(self, binary_form: bool = False) -> Any: ...
    def get_ciphers(self) -> list[_Cipher]: ...
    def load_cert_chain(
        self, certfile: StrOrBytesPath, keyfile: StrOrBytesPath | None = None, password: _PasswordType | None = None
    ) -> None: ...
    def load_dh_params(self, path: str, /) -> None: ...
    def load_verify_locations(
        self,
        cafile: StrOrBytesPath | None = None,
        capath: StrOrBytesPath | None = None,
        cadata: str | ReadableBuffer | None = None,
    ) -> None: ...
    def session_stats(self) -> dict[str, int]: ...
    def set_ciphers(self, cipherlist: str, /) -> None: ...
    def set_default_verify_paths(self) -> None: ...
    def set_ecdh_curve(self, name: str, /) -> None: ...
    if sys.version_info >= (3, 13):
        def set_psk_client_callback(self, callback: Callable[[str | None], tuple[str | None, bytes]] | None) -> None: ...
        def set_psk_server_callback(
            self, callback: Callable[[str | None], bytes] | None, identity_hint: str | None = None
        ) -> None: ...

@final
class MemoryBIO:
    eof: bool
    pending: int
    def __new__(self) -> Self: ...
    def read(self, size: int = -1, /) -> bytes: ...
    def write(self, b: ReadableBuffer, /) -> int: ...
    def write_eof(self) -> None: ...

@final
class SSLSession:
    __hash__: ClassVar[None]  # type: ignore[assignment]
    @property
    def has_ticket(self) -> bool: ...
    @property
    def id(self) -> bytes: ...
    @property
    def ticket_lifetime_hint(self) -> int: ...
    @property
    def time(self) -> int: ...
    @property
    def timeout(self) -> int: ...

# _ssl.Certificate is weird: it can't be instantiated or subclassed.
# Instances can only be created via methods of the private _ssl._SSLSocket class,
# for which the relevant method signatures are:
#
# class _SSLSocket:
#     def get_unverified_chain(self) -> list[Certificate] | None: ...
#     def get_verified_chain(self) -> list[Certificate] | None: ...
#
# You can find a _ssl._SSLSocket object as the _sslobj attribute of a ssl.SSLSocket object

if sys.version_info >= (3, 10):
    @final
    class Certificate:
        def get_info(self) -> _CertInfo: ...
        @overload
        def public_bytes(self) -> str: ...
        @overload
        def public_bytes(self, format: Literal[1] = 1, /) -> str: ...  # ENCODING_PEM
        @overload
        def public_bytes(self, format: Literal[2], /) -> bytes: ...  # ENCODING_DER
        @overload
        def public_bytes(self, format: int, /) -> str | bytes: ...

if sys.version_info < (3, 12):
    err_codes_to_names: dict[tuple[int, int], str]
    err_names_to_codes: dict[str, tuple[int, int]]
    lib_codes_to_names: dict[int, str]

_DEFAULT_CIPHERS: Final[str]

# SSL error numbers
SSL_ERROR_ZERO_RETURN: Final = 6
SSL_ERROR_WANT_READ: Final = 2
SSL_ERROR_WANT_WRITE: Final = 3
SSL_ERROR_WANT_X509_LOOKUP: Final = 4
SSL_ERROR_SYSCALL: Final = 5
SSL_ERROR_SSL: Final = 1
SSL_ERROR_WANT_CONNECT: Final = 7
SSL_ERROR_EOF: Final = 8
SSL_ERROR_INVALID_ERROR_CODE: Final = 10

# verify modes
CERT_NONE: Final = 0
CERT_OPTIONAL: Final = 1
CERT_REQUIRED: Final = 2

# verify flags
VERIFY_DEFAULT: Final = 0
VERIFY_CRL_CHECK_LEAF: Final = 0x4
VERIFY_CRL_CHECK_CHAIN: Final = 0x8
VERIFY_X509_STRICT: Final = 0x20
VERIFY_X509_TRUSTED_FIRST: Final = 0x8000
if sys.version_info >= (3, 10):
    VERIFY_ALLOW_PROXY_CERTS: Final = 0x40
    VERIFY_X509_PARTIAL_CHAIN: Final = 0x80000

# alert descriptions
ALERT_DESCRIPTION_CLOSE_NOTIFY: Final = 0
ALERT_DESCRIPTION_UNEXPECTED_MESSAGE: Final = 10
ALERT_DESCRIPTION_BAD_RECORD_MAC: Final = 20
ALERT_DESCRIPTION_RECORD_OVERFLOW: Final = 22
ALERT_DESCRIPTION_DECOMPRESSION_FAILURE: Final = 30
ALERT_DESCRIPTION_HANDSHAKE_FAILURE: Final = 40
ALERT_DESCRIPTION_BAD_CERTIFICATE: Final = 42
ALERT_DESCRIPTION_UNSUPPORTED_CERTIFICATE: Final = 43
ALERT_DESCRIPTION_CERTIFICATE_REVOKED: Final = 44
ALERT_DESCRIPTION_CERTIFICATE_EXPIRED: Final = 45
ALERT_DESCRIPTION_CERTIFICATE_UNKNOWN: Final = 46
ALERT_DESCRIPTION_ILLEGAL_PARAMETER: Final = 47
ALERT_DESCRIPTION_UNKNOWN_CA: Final = 48
ALERT_DESCRIPTION_ACCESS_DENIED: Final = 49
ALERT_DESCRIPTION_DECODE_ERROR: Final = 50
ALERT_DESCRIPTION_DECRYPT_ERROR: Final = 51
ALERT_DESCRIPTION_PROTOCOL_VERSION: Final = 70
ALERT_DESCRIPTION_INSUFFICIENT_SECURITY: Final = 71
ALERT_DESCRIPTION_INTERNAL_ERROR: Final = 80
ALERT_DESCRIPTION_USER_CANCELLED: Final = 90
ALERT_DESCRIPTION_NO_RENEGOTIATION: Final = 100
ALERT_DESCRIPTION_UNSUPPORTED_EXTENSION: Final = 110
ALERT_DESCRIPTION_CERTIFICATE_UNOBTAINABLE: Final = 111
ALERT_DESCRIPTION_UNRECOGNIZED_NAME: Final = 112
ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: Final = 113
ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: Final = 114
ALERT_DESCRIPTION_UNKNOWN_PSK_IDENTITY: Final = 115

# protocol versions
PROTOCOL_SSLv23: Final = 2
PROTOCOL_TLS: Final = 2
PROTOCOL_TLS_CLIENT: Final = 16
PROTOCOL_TLS_SERVER: Final = 17
PROTOCOL_TLSv1: Final = 3
PROTOCOL_TLSv1_1: Final = 4
PROTOCOL_TLSv1_2: Final = 5

# protocol options
OP_ALL: Final = 0x80000050
OP_NO_SSLv2: Final = 0x0
OP_NO_SSLv3: Final = 0x2000000
OP_NO_TLSv1: Final = 0x4000000
OP_NO_TLSv1_1: Final = 0x10000000
OP_NO_TLSv1_2: Final = 0x8000000
OP_NO_TLSv1_3: Final = 0x20000000
OP_CIPHER_SERVER_PREFERENCE: Final = 0x400000
OP_SINGLE_DH_USE: Final = 0x0
OP_NO_TICKET: Final = 0x4000
OP_SINGLE_ECDH_USE: Final = 0x0
OP_NO_COMPRESSION: Final = 0x20000
OP_ENABLE_MIDDLEBOX_COMPAT: Final = 0x100000
OP_NO_RENEGOTIATION: Final = 0x40000000
if sys.version_info >= (3, 11) or sys.platform == "linux":
    OP_IGNORE_UNEXPECTED_EOF: Final = 0x80
if sys.version_info >= (3, 12):
    OP_LEGACY_SERVER_CONNECT: Final = 0x4
    OP_ENABLE_KTLS: Final = 0x8

# host flags
HOSTFLAG_ALWAYS_CHECK_SUBJECT: Final = 0x1
HOSTFLAG_NEVER_CHECK_SUBJECT: Final = 0x20
HOSTFLAG_NO_WILDCARDS: Final = 0x2
HOSTFLAG_NO_PARTIAL_WILDCARDS: Final = 0x4
HOSTFLAG_MULTI_LABEL_WILDCARDS: Final = 0x8
HOSTFLAG_SINGLE_LABEL_SUBDOMAINS: Final = 0x10

if sys.version_info >= (3, 10):
    # certificate file types
    ENCODING_PEM: Final = 1
    ENCODING_DER: Final = 2

# protocol versions
PROTO_MINIMUM_SUPPORTED: Final = -2
PROTO_MAXIMUM_SUPPORTED: Final = -1
PROTO_SSLv3: Final[int]
PROTO_TLSv1: Final[int]
PROTO_TLSv1_1: Final[int]
PROTO_TLSv1_2: Final[int]
PROTO_TLSv1_3: Final[int]

# feature support
HAS_SNI: Final[bool]
HAS_TLS_UNIQUE: Final[bool]
HAS_ECDH: Final[bool]
HAS_NPN: Final[bool]
if sys.version_info >= (3, 13):
    HAS_PSK: Final[bool]
HAS_ALPN: Final[bool]
HAS_SSLv2: Final[bool]
HAS_SSLv3: Final[bool]
HAS_TLSv1: Final[bool]
HAS_TLSv1_1: Final[bool]
HAS_TLSv1_2: Final[bool]
HAS_TLSv1_3: Final[bool]
if sys.version_info >= (3, 14):
    HAS_PHA: Final[bool]

# version info
OPENSSL_VERSION_NUMBER: Final[int]
OPENSSL_VERSION_INFO: Final[tuple[int, int, int, int, int]]
OPENSSL_VERSION: Final[str]
_OPENSSL_API_VERSION: Final[tuple[int, int, int, int, int]]
