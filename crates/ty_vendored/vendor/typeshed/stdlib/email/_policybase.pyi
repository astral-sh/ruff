"""Policy framework for the email package.

Allows fine grained feature control of how the package parses and emits data.
"""

from abc import ABCMeta, abstractmethod
from email.errors import MessageDefect
from email.header import Header
from email.message import Message
from typing import Any, Generic, Protocol, TypeVar, type_check_only
from typing_extensions import Self

__all__ = ["Policy", "Compat32", "compat32"]

_MessageT = TypeVar("_MessageT", bound=Message[Any, Any], default=Message[str, str])
_MessageT_co = TypeVar("_MessageT_co", covariant=True, bound=Message[Any, Any], default=Message[str, str])

@type_check_only
class _MessageFactory(Protocol[_MessageT]):
    def __call__(self, policy: Policy[_MessageT]) -> _MessageT: ...

# Policy below is the only known direct subclass of _PolicyBase. We therefore
# assume that the __init__ arguments and attributes of _PolicyBase are
# the same as those of Policy.
class _PolicyBase(Generic[_MessageT_co]):
    """Policy Object basic framework.

    This class is useless unless subclassed.  A subclass should define
    class attributes with defaults for any values that are to be
    managed by the Policy object.  The constructor will then allow
    non-default values to be set for these attributes at instance
    creation time.  The instance will be callable, taking these same
    attributes keyword arguments, and returning a new instance
    identical to the called instance except for those values changed
    by the keyword arguments.  Instances may be added, yielding new
    instances with any non-default values from the right hand
    operand overriding those in the left hand operand.  That is,

        A + B == A(<non-default values of B>)

    The repr of an instance can be used to reconstruct the object
    if and only if the repr of the values can be used to reconstruct
    those values.

    """

    max_line_length: int | None
    linesep: str
    cte_type: str
    raise_on_defect: bool
    mangle_from_: bool
    message_factory: _MessageFactory[_MessageT_co] | None
    # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
    verify_generated_headers: bool

    def __init__(
        self,
        *,
        max_line_length: int | None = 78,
        linesep: str = "\n",
        cte_type: str = "8bit",
        raise_on_defect: bool = False,
        mangle_from_: bool = ...,  # default depends on sub-class
        message_factory: _MessageFactory[_MessageT_co] | None = None,
        # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = True,
    ) -> None:
        """Create new Policy, possibly overriding some defaults.

        See class docstring for a list of overridable attributes.

        """

    def clone(
        self,
        *,
        max_line_length: int | None = ...,
        linesep: str = ...,
        cte_type: str = ...,
        raise_on_defect: bool = ...,
        mangle_from_: bool = ...,
        message_factory: _MessageFactory[_MessageT_co] | None = ...,
        # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = ...,
    ) -> Self:
        """Return a new instance with specified attributes changed.

        The new instance has the same attribute values as the current object,
        except for the changes passed in as keyword arguments.

        """

    def __add__(self, other: Policy) -> Self:
        """Non-default values from right operand override those from left.

        The object returned is a new instance of the subclass.

        """

class Policy(_PolicyBase[_MessageT_co], metaclass=ABCMeta):
    """Controls for how messages are interpreted and formatted.

    Most of the classes and many of the methods in the email package accept
    Policy objects as parameters.  A Policy object contains a set of values and
    functions that control how input is interpreted and how output is rendered.
    For example, the parameter 'raise_on_defect' controls whether or not an RFC
    violation results in an error being raised or not, while 'max_line_length'
    controls the maximum length of output lines when a Message is serialized.

    Any valid attribute may be overridden when a Policy is created by passing
    it as a keyword argument to the constructor.  Policy objects are immutable,
    but a new Policy object can be created with only certain values changed by
    calling the Policy instance with keyword arguments.  Policy objects can
    also be added, producing a new Policy object in which the non-default
    attributes set in the right hand operand overwrite those specified in the
    left operand.

    Settable attributes:

    raise_on_defect     -- If true, then defects should be raised as errors.
                           Default: False.

    linesep             -- string containing the value to use as separation
                           between output lines.  Default '\\n'.

    cte_type            -- Type of allowed content transfer encodings

                           7bit  -- ASCII only
                           8bit  -- Content-Transfer-Encoding: 8bit is allowed

                           Default: 8bit.  Also controls the disposition of
                           (RFC invalid) binary data in headers; see the
                           documentation of the binary_fold method.

    max_line_length     -- maximum length of lines, excluding 'linesep',
                           during serialization.  None or 0 means no line
                           wrapping is done.  Default is 78.

    mangle_from_        -- a flag that, when True escapes From_ lines in the
                           body of the message by putting a '>' in front of
                           them. This is used when the message is being
                           serialized by a generator. Default: False.

    message_factory     -- the class to use to create new message objects.
                           If the value is None, the default is Message.

    verify_generated_headers
                        -- if true, the generator verifies that each header
                           they are properly folded, so that a parser won't
                           treat it as multiple headers, start-of-body, or
                           part of another header.
                           This is a check against custom Header & fold()
                           implementations.
    """

    # Every Message object has a `defects` attribute, so the following
    # methods will work for any Message object.
    def handle_defect(self, obj: Message[Any, Any], defect: MessageDefect) -> None:
        """Based on policy, either raise defect or call register_defect.

            handle_defect(obj, defect)

        defect should be a Defect subclass, but in any case must be an
        Exception subclass.  obj is the object on which the defect should be
        registered if it is not raised.  If the raise_on_defect is True, the
        defect is raised as an error, otherwise the object and the defect are
        passed to register_defect.

        This method is intended to be called by parsers that discover defects.
        The email package parsers always call it with Defect instances.

        """

    def register_defect(self, obj: Message[Any, Any], defect: MessageDefect) -> None:
        """Record 'defect' on 'obj'.

        Called by handle_defect if raise_on_defect is False.  This method is
        part of the Policy API so that Policy subclasses can implement custom
        defect handling.  The default implementation calls the append method of
        the defects attribute of obj.  The objects used by the email package by
        default that get passed to this method will always have a defects
        attribute with an append method.

        """

    def header_max_count(self, name: str) -> int | None:
        """Return the maximum allowed number of headers named 'name'.

        Called when a header is added to a Message object.  If the returned
        value is not 0 or None, and there are already a number of headers with
        the name 'name' equal to the value returned, a ValueError is raised.

        Because the default behavior of Message's __setitem__ is to append the
        value to the list of headers, it is easy to create duplicate headers
        without realizing it.  This method allows certain headers to be limited
        in the number of instances of that header that may be added to a
        Message programmatically.  (The limit is not observed by the parser,
        which will faithfully produce as many headers as exist in the message
        being parsed.)

        The default implementation returns None for all header names.
        """

    @abstractmethod
    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]:
        """Given a list of linesep terminated strings constituting the lines of
        a single header, return the (name, value) tuple that should be stored
        in the model.  The input lines should retain their terminating linesep
        characters.  The lines passed in by the email package may contain
        surrogateescaped binary data.
        """

    @abstractmethod
    def header_store_parse(self, name: str, value: str) -> tuple[str, str]:
        """Given the header name and the value provided by the application
        program, return the (name, value) that should be stored in the model.
        """

    @abstractmethod
    def header_fetch_parse(self, name: str, value: str) -> str:
        """Given the header name and the value from the model, return the value
        to be returned to the application program that is requesting that
        header.  The value passed in by the email package may contain
        surrogateescaped binary data if the lines were parsed by a BytesParser.
        The returned value should not contain any surrogateescaped data.

        """

    @abstractmethod
    def fold(self, name: str, value: str) -> str:
        """Given the header name and the value from the model, return a string
        containing linesep characters that implement the folding of the header
        according to the policy controls.  The value passed in by the email
        package may contain surrogateescaped binary data if the lines were
        parsed by a BytesParser.  The returned value should not contain any
        surrogateescaped data.

        """

    @abstractmethod
    def fold_binary(self, name: str, value: str) -> bytes:
        """Given the header name and the value from the model, return binary
        data containing linesep characters that implement the folding of the
        header according to the policy controls.  The value passed in by the
        email package may contain surrogateescaped binary data.

        """

class Compat32(Policy[_MessageT_co]):
    """Controls for how messages are interpreted and formatted.

    Most of the classes and many of the methods in the email package accept
    Policy objects as parameters.  A Policy object contains a set of values and
    functions that control how input is interpreted and how output is rendered.
    For example, the parameter 'raise_on_defect' controls whether or not an RFC
    violation results in an error being raised or not, while 'max_line_length'
    controls the maximum length of output lines when a Message is serialized.

    Any valid attribute may be overridden when a Policy is created by passing
    it as a keyword argument to the constructor.  Policy objects are immutable,
    but a new Policy object can be created with only certain values changed by
    calling the Policy instance with keyword arguments.  Policy objects can
    also be added, producing a new Policy object in which the non-default
    attributes set in the right hand operand overwrite those specified in the
    left operand.

    Settable attributes:

    raise_on_defect     -- If true, then defects should be raised as errors.
                           Default: False.

    linesep             -- string containing the value to use as separation
                           between output lines.  Default '\\n'.

    cte_type            -- Type of allowed content transfer encodings

                           7bit  -- ASCII only
                           8bit  -- Content-Transfer-Encoding: 8bit is allowed

                           Default: 8bit.  Also controls the disposition of
                           (RFC invalid) binary data in headers; see the
                           documentation of the binary_fold method.

    max_line_length     -- maximum length of lines, excluding 'linesep',
                           during serialization.  None or 0 means no line
                           wrapping is done.  Default is 78.

    mangle_from_        -- a flag that, when True escapes From_ lines in the
                           body of the message by putting a '>' in front of
                           them. This is used when the message is being
                           serialized by a generator. Default: False.

    message_factory     -- the class to use to create new message objects.
                           If the value is None, the default is Message.

    verify_generated_headers
                        -- if true, the generator verifies that each header
                           they are properly folded, so that a parser won't
                           treat it as multiple headers, start-of-body, or
                           part of another header.
                           This is a check against custom Header & fold()
                           implementations.
    This particular policy is the backward compatibility Policy.  It
    replicates the behavior of the email package version 5.1.
    """

    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]:
        """Given a list of linesep terminated strings constituting the lines of
        a single header, return the (name, value) tuple that should be stored
        in the model.  The input lines should retain their terminating linesep
        characters.  The lines passed in by the email package may contain
        surrogateescaped binary data.
        The name is parsed as everything up to the ':' and returned unmodified.
        The value is determined by stripping leading whitespace off the
        remainder of the first line joined with all subsequent lines, and
        stripping any trailing carriage return or linefeed characters.

        """

    def header_store_parse(self, name: str, value: str) -> tuple[str, str]:
        """Given the header name and the value provided by the application
        program, return the (name, value) that should be stored in the model.
        The name and value are returned unmodified.
        """

    def header_fetch_parse(self, name: str, value: str) -> str | Header:  # type: ignore[override]
        """Given the header name and the value from the model, return the value
        to be returned to the application program that is requesting that
        header.  The value passed in by the email package may contain
        surrogateescaped binary data if the lines were parsed by a BytesParser.
        The returned value should not contain any surrogateescaped data.

        If the value contains binary data, it is converted into a Header object
        using the unknown-8bit charset.  Otherwise it is returned unmodified.
        """

    def fold(self, name: str, value: str) -> str:
        """Given the header name and the value from the model, return a string
        containing linesep characters that implement the folding of the header
        according to the policy controls.  The value passed in by the email
        package may contain surrogateescaped binary data if the lines were
        parsed by a BytesParser.  The returned value should not contain any
        surrogateescaped data.

        Headers are folded using the Header folding algorithm, which preserves
        existing line breaks in the value, and wraps each resulting line to the
        max_line_length.  Non-ASCII binary data are CTE encoded using the
        unknown-8bit charset.

        """

    def fold_binary(self, name: str, value: str) -> bytes:
        """Given the header name and the value from the model, return binary
        data containing linesep characters that implement the folding of the
        header according to the policy controls.  The value passed in by the
        email package may contain surrogateescaped binary data.

        Headers are folded using the Header folding algorithm, which preserves
        existing line breaks in the value, and wraps each resulting line to the
        max_line_length.  If cte_type is 7bit, non-ascii binary data is CTE
        encoded using the unknown-8bit charset.  Otherwise the original source
        header is used, with its existing line breaks and/or binary data.

        """

compat32: Compat32[Message[str, str]]
