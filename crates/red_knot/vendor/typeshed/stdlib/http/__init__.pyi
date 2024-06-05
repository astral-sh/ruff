import sys
from enum import IntEnum

if sys.version_info >= (3, 11):
    from enum import StrEnum

if sys.version_info >= (3, 11):
    __all__ = ["HTTPStatus", "HTTPMethod"]
else:
    __all__ = ["HTTPStatus"]

class HTTPStatus(IntEnum):
    @property
    def phrase(self) -> str: ...
    @property
    def description(self) -> str: ...
    CONTINUE = 100
    SWITCHING_PROTOCOLS = 101
    PROCESSING = 102
    OK = 200
    CREATED = 201
    ACCEPTED = 202
    NON_AUTHORITATIVE_INFORMATION = 203
    NO_CONTENT = 204
    RESET_CONTENT = 205
    PARTIAL_CONTENT = 206
    MULTI_STATUS = 207
    ALREADY_REPORTED = 208
    IM_USED = 226
    MULTIPLE_CHOICES = 300
    MOVED_PERMANENTLY = 301
    FOUND = 302
    SEE_OTHER = 303
    NOT_MODIFIED = 304
    USE_PROXY = 305
    TEMPORARY_REDIRECT = 307
    PERMANENT_REDIRECT = 308
    BAD_REQUEST = 400
    UNAUTHORIZED = 401
    PAYMENT_REQUIRED = 402
    FORBIDDEN = 403
    NOT_FOUND = 404
    METHOD_NOT_ALLOWED = 405
    NOT_ACCEPTABLE = 406
    PROXY_AUTHENTICATION_REQUIRED = 407
    REQUEST_TIMEOUT = 408
    CONFLICT = 409
    GONE = 410
    LENGTH_REQUIRED = 411
    PRECONDITION_FAILED = 412
    if sys.version_info >= (3, 13):
        CONTENT_TOO_LARGE = 413
    REQUEST_ENTITY_TOO_LARGE = 413
    if sys.version_info >= (3, 13):
        URI_TOO_LONG = 414
    REQUEST_URI_TOO_LONG = 414
    UNSUPPORTED_MEDIA_TYPE = 415
    if sys.version_info >= (3, 13):
        RANGE_NOT_SATISFIABLE = 416
    REQUESTED_RANGE_NOT_SATISFIABLE = 416
    EXPECTATION_FAILED = 417
    if sys.version_info >= (3, 13):
        UNPROCESSABLE_CONTENT = 422
    UNPROCESSABLE_ENTITY = 422
    LOCKED = 423
    FAILED_DEPENDENCY = 424
    UPGRADE_REQUIRED = 426
    PRECONDITION_REQUIRED = 428
    TOO_MANY_REQUESTS = 429
    REQUEST_HEADER_FIELDS_TOO_LARGE = 431
    INTERNAL_SERVER_ERROR = 500
    NOT_IMPLEMENTED = 501
    BAD_GATEWAY = 502
    SERVICE_UNAVAILABLE = 503
    GATEWAY_TIMEOUT = 504
    HTTP_VERSION_NOT_SUPPORTED = 505
    VARIANT_ALSO_NEGOTIATES = 506
    INSUFFICIENT_STORAGE = 507
    LOOP_DETECTED = 508
    NOT_EXTENDED = 510
    NETWORK_AUTHENTICATION_REQUIRED = 511
    MISDIRECTED_REQUEST = 421
    UNAVAILABLE_FOR_LEGAL_REASONS = 451
    if sys.version_info >= (3, 9):
        EARLY_HINTS = 103
        IM_A_TEAPOT = 418
        TOO_EARLY = 425
    if sys.version_info >= (3, 12):
        @property
        def is_informational(self) -> bool: ...
        @property
        def is_success(self) -> bool: ...
        @property
        def is_redirection(self) -> bool: ...
        @property
        def is_client_error(self) -> bool: ...
        @property
        def is_server_error(self) -> bool: ...

if sys.version_info >= (3, 11):
    class HTTPMethod(StrEnum):
        @property
        def description(self) -> str: ...
        CONNECT = "CONNECT"
        DELETE = "DELETE"
        GET = "GET"
        HEAD = "HEAD"
        OPTIONS = "OPTIONS"
        PATCH = "PATCH"
        POST = "POST"
        PUT = "PUT"
        TRACE = "TRACE"
