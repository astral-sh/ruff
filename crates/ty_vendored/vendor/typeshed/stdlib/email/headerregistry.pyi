"""Representing and manipulating email headers via custom objects.

This module provides an implementation of the HeaderRegistry API.
The implementation is designed to flexibly follow RFC5322 rules.
"""

import types
from collections.abc import Iterable, Mapping
from datetime import datetime as _datetime
from email._header_value_parser import (
    AddressList,
    ContentDisposition,
    ContentTransferEncoding,
    ContentType,
    MessageID,
    MIMEVersion,
    TokenList,
    UnstructuredTokenList,
)
from email.errors import MessageDefect
from email.policy import Policy
from typing import Any, ClassVar, Literal, Protocol, type_check_only
from typing_extensions import Self

class BaseHeader(str):
    """Base class for message headers.

    Implements generic behavior and provides tools for subclasses.

    A subclass must define a classmethod named 'parse' that takes an unfolded
    value string and a dictionary as its arguments.  The dictionary will
    contain one key, 'defects', initialized to an empty list.  After the call
    the dictionary must contain two additional keys: parse_tree, set to the
    parse tree obtained from parsing the header, and 'decoded', set to the
    string value of the idealized representation of the data from the value.
    (That is, encoded words are decoded, and values that have canonical
    representations are so represented.)

    The defects key is intended to collect parsing defects, which the message
    parser will subsequently dispose of as appropriate.  The parser should not,
    insofar as practical, raise any errors.  Defects should be added to the
    list instead.  The standard header parsers register defects for RFC
    compliance issues, for obsolete RFC syntax, and for unrecoverable parsing
    errors.

    The parse method may add additional keys to the dictionary.  In this case
    the subclass must define an 'init' method, which will be passed the
    dictionary as its keyword arguments.  The method should use (usually by
    setting them as the value of similarly named attributes) and remove all the
    extra keys added by its parse method, and then use super to call its parent
    class with the remaining arguments and keywords.

    The subclass should also make sure that a 'max_count' attribute is defined
    that is either None or 1. XXX: need to better define this API.

    """

    # max_count is actually more of an abstract ClassVar (not defined on the base class, but expected to be defined in subclasses)
    max_count: ClassVar[Literal[1] | None]
    @property
    def name(self) -> str: ...
    @property
    def defects(self) -> tuple[MessageDefect, ...]: ...
    def __new__(cls, name: str, value: Any) -> Self: ...
    def init(self, name: str, *, parse_tree: TokenList, defects: Iterable[MessageDefect]) -> None: ...
    def fold(self, *, policy: Policy) -> str:
        """Fold header according to policy.

        The parsed representation of the header is folded according to
        RFC5322 rules, as modified by the policy.  If the parse tree
        contains surrogateescaped bytes, the bytes are CTE encoded using
        the charset 'unknown-8bit".

        Any non-ASCII characters in the parse tree are CTE encoded using
        charset utf-8. XXX: make this a policy setting.

        The returned value is an ASCII-only string possibly containing linesep
        characters, and ending with a linesep character.  The string includes
        the header name and the ': ' separator.

        """

class UnstructuredHeader:
    max_count: ClassVar[Literal[1] | None]
    @staticmethod
    def value_parser(value: str) -> UnstructuredTokenList:
        """unstructured = (*([FWS] vchar) *WSP) / obs-unstruct
           obs-unstruct = *((*LF *CR *(obs-utext) *LF *CR)) / FWS)
           obs-utext = %d0 / obs-NO-WS-CTL / LF / CR

           obs-NO-WS-CTL is control characters except WSP/CR/LF.

        So, basically, we have printable runs, plus control characters or nulls in
        the obsolete syntax, separated by whitespace.  Since RFC 2047 uses the
        obsolete syntax in its specification, but requires whitespace on either
        side of the encoded words, I can see no reason to need to separate the
        non-printable-non-whitespace from the printable runs if they occur, so we
        parse this into xtext tokens separated by WSP tokens.

        Because an 'unstructured' value must by definition constitute the entire
        value, this 'get' routine does not return a remaining value, only the
        parsed TokenList.

        """

    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...

class UniqueUnstructuredHeader(UnstructuredHeader):
    max_count: ClassVar[Literal[1]]

class DateHeader:
    """Header whose value consists of a single timestamp.

    Provides an additional attribute, datetime, which is either an aware
    datetime using a timezone, or a naive datetime if the timezone
    in the input string is -0000.  Also accepts a datetime as input.
    The 'value' attribute is the normalized form of the timestamp,
    which means it is the output of format_datetime on the datetime.
    """

    max_count: ClassVar[Literal[1] | None]
    def init(self, name: str, *, parse_tree: TokenList, defects: Iterable[MessageDefect], datetime: _datetime) -> None: ...
    @property
    def datetime(self) -> _datetime | None: ...
    @staticmethod
    def value_parser(value: str) -> UnstructuredTokenList:
        """unstructured = (*([FWS] vchar) *WSP) / obs-unstruct
           obs-unstruct = *((*LF *CR *(obs-utext) *LF *CR)) / FWS)
           obs-utext = %d0 / obs-NO-WS-CTL / LF / CR

           obs-NO-WS-CTL is control characters except WSP/CR/LF.

        So, basically, we have printable runs, plus control characters or nulls in
        the obsolete syntax, separated by whitespace.  Since RFC 2047 uses the
        obsolete syntax in its specification, but requires whitespace on either
        side of the encoded words, I can see no reason to need to separate the
        non-printable-non-whitespace from the printable runs if they occur, so we
        parse this into xtext tokens separated by WSP tokens.

        Because an 'unstructured' value must by definition constitute the entire
        value, this 'get' routine does not return a remaining value, only the
        parsed TokenList.

        """

    @classmethod
    def parse(cls, value: str | _datetime, kwds: dict[str, Any]) -> None: ...

class UniqueDateHeader(DateHeader):
    max_count: ClassVar[Literal[1]]

class AddressHeader:
    max_count: ClassVar[Literal[1] | None]
    def init(self, name: str, *, parse_tree: TokenList, defects: Iterable[MessageDefect], groups: Iterable[Group]) -> None: ...
    @property
    def groups(self) -> tuple[Group, ...]: ...
    @property
    def addresses(self) -> tuple[Address, ...]: ...
    @staticmethod
    def value_parser(value: str) -> AddressList: ...
    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...

class UniqueAddressHeader(AddressHeader):
    max_count: ClassVar[Literal[1]]

class SingleAddressHeader(AddressHeader):
    @property
    def address(self) -> Address: ...

class UniqueSingleAddressHeader(SingleAddressHeader):
    max_count: ClassVar[Literal[1]]

class MIMEVersionHeader:
    max_count: ClassVar[Literal[1]]
    def init(
        self,
        name: str,
        *,
        parse_tree: TokenList,
        defects: Iterable[MessageDefect],
        version: str | None,
        major: int | None,
        minor: int | None,
    ) -> None: ...
    @property
    def version(self) -> str | None: ...
    @property
    def major(self) -> int | None: ...
    @property
    def minor(self) -> int | None: ...
    @staticmethod
    def value_parser(value: str) -> MIMEVersion:
        """mime-version = [CFWS] 1*digit [CFWS] "." [CFWS] 1*digit [CFWS]"""

    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...

class ParameterizedMIMEHeader:
    max_count: ClassVar[Literal[1]]
    def init(self, name: str, *, parse_tree: TokenList, defects: Iterable[MessageDefect], params: Mapping[str, Any]) -> None: ...
    @property
    def params(self) -> types.MappingProxyType[str, Any]: ...
    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...

class ContentTypeHeader(ParameterizedMIMEHeader):
    @property
    def content_type(self) -> str: ...
    @property
    def maintype(self) -> str: ...
    @property
    def subtype(self) -> str: ...
    @staticmethod
    def value_parser(value: str) -> ContentType:
        """maintype "/" subtype *( ";" parameter )

        The maintype and substype are tokens.  Theoretically they could
        be checked against the official IANA list + x-token, but we
        don't do that.
        """

class ContentDispositionHeader(ParameterizedMIMEHeader):
    # init is redefined but has the same signature as parent class, so is omitted from the stub
    @property
    def content_disposition(self) -> str | None: ...
    @staticmethod
    def value_parser(value: str) -> ContentDisposition:
        """disposition-type *( ";" parameter )"""

class ContentTransferEncodingHeader:
    max_count: ClassVar[Literal[1]]
    def init(self, name: str, *, parse_tree: TokenList, defects: Iterable[MessageDefect]) -> None: ...
    @property
    def cte(self) -> str: ...
    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...
    @staticmethod
    def value_parser(value: str) -> ContentTransferEncoding:
        """mechanism"""

class MessageIDHeader:
    max_count: ClassVar[Literal[1]]
    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any]) -> None: ...
    @staticmethod
    def value_parser(value: str) -> MessageID:
        """message-id      =   "Message-ID:" msg-id CRLF"""

@type_check_only
class _HeaderParser(Protocol):
    max_count: ClassVar[Literal[1] | None]
    @staticmethod
    def value_parser(value: str, /) -> TokenList: ...
    @classmethod
    def parse(cls, value: str, kwds: dict[str, Any], /) -> None: ...

class HeaderRegistry:
    """A header_factory and header registry."""

    registry: dict[str, type[_HeaderParser]]
    base_class: type[BaseHeader]
    default_class: type[_HeaderParser]
    def __init__(
        self, base_class: type[BaseHeader] = ..., default_class: type[_HeaderParser] = ..., use_default_map: bool = True
    ) -> None:
        """Create a header_factory that works with the Policy API.

        base_class is the class that will be the last class in the created
        header class's __bases__ list.  default_class is the class that will be
        used if "name" (see __call__) does not appear in the registry.
        use_default_map controls whether or not the default mapping of names to
        specialized classes is copied in to the registry when the factory is
        created.  The default is True.

        """

    def map_to_type(self, name: str, cls: type[BaseHeader]) -> None:
        """Register cls as the specialized class for handling "name" headers."""

    def __getitem__(self, name: str) -> type[BaseHeader]: ...
    def __call__(self, name: str, value: Any) -> BaseHeader:
        """Create a header instance for header 'name' from 'value'.

        Creates a header instance by creating a specialized class for parsing
        and representing the specified header by combining the factory
        base_class with a specialized class from the registry or the
        default_class, and passing the name and value to the constructed
        class's constructor.

        """

class Address:
    @property
    def display_name(self) -> str: ...
    @property
    def username(self) -> str: ...
    @property
    def domain(self) -> str: ...
    @property
    def addr_spec(self) -> str:
        """The addr_spec (username@domain) portion of the address, quoted
        according to RFC 5322 rules, but with no Content Transfer Encoding.
        """

    def __init__(
        self, display_name: str = "", username: str | None = "", domain: str | None = "", addr_spec: str | None = None
    ) -> None:
        """Create an object representing a full email address.

        An address can have a 'display_name', a 'username', and a 'domain'.  In
        addition to specifying the username and domain separately, they may be
        specified together by using the addr_spec keyword *instead of* the
        username and domain keywords.  If an addr_spec string is specified it
        must be properly quoted according to RFC 5322 rules; an error will be
        raised if it is not.

        An Address object has display_name, username, domain, and addr_spec
        attributes, all of which are read-only.  The addr_spec and the string
        value of the object are both quoted according to RFC5322 rules, but
        without any Content Transfer Encoding.

        """
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __eq__(self, other: object) -> bool: ...

class Group:
    @property
    def display_name(self) -> str | None: ...
    @property
    def addresses(self) -> tuple[Address, ...]: ...
    def __init__(self, display_name: str | None = None, addresses: Iterable[Address] | None = None) -> None:
        """Create an object representing an address group.

        An address group consists of a display_name followed by colon and a
        list of addresses (see Address) terminated by a semi-colon.  The Group
        is created by specifying a display_name and a possibly empty list of
        Address objects.  A Group can also be used to represent a single
        address that is not in a group, which is convenient when manipulating
        lists that are a combination of Groups and individual Addresses.  In
        this case the display_name should be set to None.  In particular, the
        string representation of a Group whose display_name is None is the same
        as the Address object, if there is one and only one Address object in
        the addresses list.

        """
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __eq__(self, other: object) -> bool: ...
