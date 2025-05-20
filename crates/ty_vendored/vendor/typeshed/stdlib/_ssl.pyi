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
from typing import Any, ClassVar, Literal, TypedDict, final, overload
from typing_extensions import NotRequired, Self, TypeAlias

_PasswordType: TypeAlias = Callable[[], str | bytes | bytearray] | str | bytes | bytearray
_PCTRTT: TypeAlias = tuple[tuple[str, str], ...]
_PCTRTTT: TypeAlias = tuple[_PCTRTT, ...]
_PeerCertRetDictType: TypeAlias = dict[str, str | _PCTRTTT | _PCTRTT]

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

_DEFAULT_CIPHERS: str

# SSL error numbers
SSL_ERROR_ZERO_RETURN: int
SSL_ERROR_WANT_READ: int
SSL_ERROR_WANT_WRITE: int
SSL_ERROR_WANT_X509_LOOKUP: int
SSL_ERROR_SYSCALL: int
SSL_ERROR_SSL: int
SSL_ERROR_WANT_CONNECT: int
SSL_ERROR_EOF: int
SSL_ERROR_INVALID_ERROR_CODE: int

# verify modes
CERT_NONE: int
CERT_OPTIONAL: int
CERT_REQUIRED: int

# verify flags
VERIFY_DEFAULT: int
VERIFY_CRL_CHECK_LEAF: int
VERIFY_CRL_CHECK_CHAIN: int
VERIFY_X509_STRICT: int
VERIFY_X509_TRUSTED_FIRST: int
if sys.version_info >= (3, 10):
    VERIFY_ALLOW_PROXY_CERTS: int
    VERIFY_X509_PARTIAL_CHAIN: int

# alert descriptions
ALERT_DESCRIPTION_CLOSE_NOTIFY: int
ALERT_DESCRIPTION_UNEXPECTED_MESSAGE: int
ALERT_DESCRIPTION_BAD_RECORD_MAC: int
ALERT_DESCRIPTION_RECORD_OVERFLOW: int
ALERT_DESCRIPTION_DECOMPRESSION_FAILURE: int
ALERT_DESCRIPTION_HANDSHAKE_FAILURE: int
ALERT_DESCRIPTION_BAD_CERTIFICATE: int
ALERT_DESCRIPTION_UNSUPPORTED_CERTIFICATE: int
ALERT_DESCRIPTION_CERTIFICATE_REVOKED: int
ALERT_DESCRIPTION_CERTIFICATE_EXPIRED: int
ALERT_DESCRIPTION_CERTIFICATE_UNKNOWN: int
ALERT_DESCRIPTION_ILLEGAL_PARAMETER: int
ALERT_DESCRIPTION_UNKNOWN_CA: int
ALERT_DESCRIPTION_ACCESS_DENIED: int
ALERT_DESCRIPTION_DECODE_ERROR: int
ALERT_DESCRIPTION_DECRYPT_ERROR: int
ALERT_DESCRIPTION_PROTOCOL_VERSION: int
ALERT_DESCRIPTION_INSUFFICIENT_SECURITY: int
ALERT_DESCRIPTION_INTERNAL_ERROR: int
ALERT_DESCRIPTION_USER_CANCELLED: int
ALERT_DESCRIPTION_NO_RENEGOTIATION: int
ALERT_DESCRIPTION_UNSUPPORTED_EXTENSION: int
ALERT_DESCRIPTION_CERTIFICATE_UNOBTAINABLE: int
ALERT_DESCRIPTION_UNRECOGNIZED_NAME: int
ALERT_DESCRIPTION_BAD_CERTIFICATE_STATUS_RESPONSE: int
ALERT_DESCRIPTION_BAD_CERTIFICATE_HASH_VALUE: int
ALERT_DESCRIPTION_UNKNOWN_PSK_IDENTITY: int

# protocol versions
PROTOCOL_SSLv23: int
PROTOCOL_TLS: int
PROTOCOL_TLS_CLIENT: int
PROTOCOL_TLS_SERVER: int
PROTOCOL_TLSv1: int
PROTOCOL_TLSv1_1: int
PROTOCOL_TLSv1_2: int

# protocol options
OP_ALL: int
OP_NO_SSLv2: int
OP_NO_SSLv3: int
OP_NO_TLSv1: int
OP_NO_TLSv1_1: int
OP_NO_TLSv1_2: int
OP_NO_TLSv1_3: int
OP_CIPHER_SERVER_PREFERENCE: int
OP_SINGLE_DH_USE: int
OP_NO_TICKET: int
OP_SINGLE_ECDH_USE: int
OP_NO_COMPRESSION: int
OP_ENABLE_MIDDLEBOX_COMPAT: int
OP_NO_RENEGOTIATION: int
if sys.version_info >= (3, 11) or sys.platform == "linux":
    OP_IGNORE_UNEXPECTED_EOF: int
if sys.version_info >= (3, 12):
    OP_LEGACY_SERVER_CONNECT: int
    OP_ENABLE_KTLS: int

# host flags
HOSTFLAG_ALWAYS_CHECK_SUBJECT: int
HOSTFLAG_NEVER_CHECK_SUBJECT: int
HOSTFLAG_NO_WILDCARDS: int
HOSTFLAG_NO_PARTIAL_WILDCARDS: int
HOSTFLAG_MULTI_LABEL_WILDCARDS: int
HOSTFLAG_SINGLE_LABEL_SUBDOMAINS: int

if sys.version_info >= (3, 10):
    # certificate file types
    # Typed as Literal so the overload on Certificate.public_bytes can work properly.
    ENCODING_PEM: Literal[1]
    ENCODING_DER: Literal[2]

# protocol versions
PROTO_MINIMUM_SUPPORTED: int
PROTO_MAXIMUM_SUPPORTED: int
PROTO_SSLv3: int
PROTO_TLSv1: int
PROTO_TLSv1_1: int
PROTO_TLSv1_2: int
PROTO_TLSv1_3: int

# feature support
HAS_SNI: bool
HAS_TLS_UNIQUE: bool
HAS_ECDH: bool
HAS_NPN: bool
if sys.version_info >= (3, 13):
    HAS_PSK: bool
HAS_ALPN: bool
HAS_SSLv2: bool
HAS_SSLv3: bool
HAS_TLSv1: bool
HAS_TLSv1_1: bool
HAS_TLSv1_2: bool
HAS_TLSv1_3: bool
if sys.version_info >= (3, 14):
    HAS_PHA: bool

# version info
OPENSSL_VERSION_NUMBER: int
OPENSSL_VERSION_INFO: tuple[int, int, int, int, int]
OPENSSL_VERSION: str
_OPENSSL_API_VERSION: tuple[int, int, int, int, int]
