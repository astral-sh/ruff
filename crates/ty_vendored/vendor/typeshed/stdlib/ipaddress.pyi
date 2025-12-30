"""A fast, lightweight IPv4/IPv6 manipulation library in Python.

This library is used to create/poke/manipulate IPv4 and IPv6 addresses
and networks.

"""

import sys
from collections.abc import Iterable, Iterator
from typing import Any, Final, Generic, Literal, TypeVar, overload
from typing_extensions import Self, TypeAlias

# Undocumented length constants
IPV4LENGTH: Final = 32
IPV6LENGTH: Final = 128

_A = TypeVar("_A", IPv4Address, IPv6Address)
_N = TypeVar("_N", IPv4Network, IPv6Network)

_RawIPAddress: TypeAlias = int | str | bytes | IPv4Address | IPv6Address
_RawNetworkPart: TypeAlias = IPv4Network | IPv6Network | IPv4Interface | IPv6Interface

def ip_address(address: _RawIPAddress) -> IPv4Address | IPv6Address:
    """Take an IP string/int and return an object of the correct type.

    Args:
        address: A string or integer, the IP address.  Either IPv4 or
          IPv6 addresses may be supplied; integers less than 2**32 will
          be considered to be IPv4 by default.

    Returns:
        An IPv4Address or IPv6Address object.

    Raises:
        ValueError: if the *address* passed isn't either a v4 or a v6
          address

    """

def ip_network(
    address: _RawIPAddress | _RawNetworkPart | tuple[_RawIPAddress] | tuple[_RawIPAddress, int], strict: bool = True
) -> IPv4Network | IPv6Network:
    """Take an IP string/int and return an object of the correct type.

    Args:
        address: A string or integer, the IP network.  Either IPv4 or
          IPv6 networks may be supplied; integers less than 2**32 will
          be considered to be IPv4 by default.

    Returns:
        An IPv4Network or IPv6Network object.

    Raises:
        ValueError: if the string passed isn't either a v4 or a v6
          address. Or if the network has host bits set.

    """

def ip_interface(
    address: _RawIPAddress | _RawNetworkPart | tuple[_RawIPAddress] | tuple[_RawIPAddress, int],
) -> IPv4Interface | IPv6Interface:
    """Take an IP string/int and return an object of the correct type.

    Args:
        address: A string or integer, the IP address.  Either IPv4 or
          IPv6 addresses may be supplied; integers less than 2**32 will
          be considered to be IPv4 by default.

    Returns:
        An IPv4Interface or IPv6Interface object.

    Raises:
        ValueError: if the string passed isn't either a v4 or a v6
          address.

    Notes:
        The IPv?Interface classes describe an Address on a particular
        Network, so they're basically a combination of both the Address
        and Network classes.

    """

class _IPAddressBase:
    """The mother class."""

    __slots__ = ()
    @property
    def compressed(self) -> str:
        """Return the shorthand version of the IP address as a string."""

    @property
    def exploded(self) -> str:
        """Return the longhand version of the IP address as a string."""

    @property
    def reverse_pointer(self) -> str:
        """The name of the reverse DNS pointer for the IP address, e.g.:
        >>> ipaddress.ip_address("127.0.0.1").reverse_pointer
        '1.0.0.127.in-addr.arpa'
        >>> ipaddress.ip_address("2001:db8::1").reverse_pointer
        '1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa'

        """
    if sys.version_info < (3, 14):
        @property
        def version(self) -> int: ...

class _BaseAddress(_IPAddressBase):
    """A generic IP object.

    This IP class contains the version independent methods which are
    used by single IP addresses.
    """

    __slots__ = ()
    def __add__(self, other: int) -> Self: ...
    def __hash__(self) -> int: ...
    def __int__(self) -> int: ...
    def __sub__(self, other: int) -> Self: ...
    def __format__(self, fmt: str) -> str:
        """Returns an IP address as a formatted string.

        Supported presentation types are:
        's': returns the IP address as a string (default)
        'b': converts to binary and returns a zero-padded string
        'X' or 'x': converts to upper- or lower-case hex and returns a zero-padded string
        'n': the same as 'b' for IPv4 and 'x' for IPv6

        For binary and hex presentation types, the alternate form specifier
        '#' and the grouping option '_' are supported.
        """

    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: Self) -> bool: ...
    if sys.version_info >= (3, 11):
        def __ge__(self, other: Self) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __gt__(self, other: Self) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __le__(self, other: Self) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""
    else:
        def __ge__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __gt__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __le__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""

class _BaseNetwork(_IPAddressBase, Generic[_A]):
    """A generic IP network object.

    This IP class contains the version independent methods which are
    used by networks.
    """

    network_address: _A
    netmask: _A
    def __contains__(self, other: Any) -> bool: ...
    def __getitem__(self, n: int) -> _A: ...
    def __iter__(self) -> Iterator[_A]: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __lt__(self, other: Self) -> bool: ...
    if sys.version_info >= (3, 11):
        def __ge__(self, other: Self) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __gt__(self, other: Self) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __le__(self, other: Self) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""
    else:
        def __ge__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __gt__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __le__(self, other: Self, NotImplemented: Any = ...) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""

    def address_exclude(self, other: Self) -> Iterator[Self]:
        """Remove an address from a larger block.

        For example:

            addr1 = ip_network('192.0.2.0/28')
            addr2 = ip_network('192.0.2.1/32')
            list(addr1.address_exclude(addr2)) =
                [IPv4Network('192.0.2.0/32'), IPv4Network('192.0.2.2/31'),
                 IPv4Network('192.0.2.4/30'), IPv4Network('192.0.2.8/29')]

        or IPv6:

            addr1 = ip_network('2001:db8::1/32')
            addr2 = ip_network('2001:db8::1/128')
            list(addr1.address_exclude(addr2)) =
                [ip_network('2001:db8::1/128'),
                 ip_network('2001:db8::2/127'),
                 ip_network('2001:db8::4/126'),
                 ip_network('2001:db8::8/125'),
                 ...
                 ip_network('2001:db8:8000::/33')]

        Args:
            other: An IPv4Network or IPv6Network object of the same type.

        Returns:
            An iterator of the IPv(4|6)Network objects which is self
            minus other.

        Raises:
            TypeError: If self and other are of differing address
              versions, or if other is not a network object.
            ValueError: If other is not completely contained by self.

        """

    @property
    def broadcast_address(self) -> _A: ...
    def compare_networks(self, other: Self) -> int:
        """Compare two IP objects.

        This is only concerned about the comparison of the integer
        representation of the network addresses.  This means that the
        host bits aren't considered at all in this method.  If you want
        to compare host bits, you can easily enough do a
        'HostA._ip < HostB._ip'

        Args:
            other: An IP object.

        Returns:
            If the IP versions of self and other are the same, returns:

            -1 if self < other:
              eg: IPv4Network('192.0.2.0/25') < IPv4Network('192.0.2.128/25')
              IPv6Network('2001:db8::1000/124') <
                  IPv6Network('2001:db8::2000/124')
            0 if self == other
              eg: IPv4Network('192.0.2.0/24') == IPv4Network('192.0.2.0/24')
              IPv6Network('2001:db8::1000/124') ==
                  IPv6Network('2001:db8::1000/124')
            1 if self > other
              eg: IPv4Network('192.0.2.128/25') > IPv4Network('192.0.2.0/25')
                  IPv6Network('2001:db8::2000/124') >
                      IPv6Network('2001:db8::1000/124')

          Raises:
              TypeError if the IP versions are different.

        """

    def hosts(self) -> Iterator[_A]:
        """Generate Iterator over usable hosts in a network.

        This is like __iter__ except it doesn't return the network
        or broadcast addresses.

        """

    @property
    def is_global(self) -> bool:
        """Test if this address is allocated for public networks.

        Returns:
            A boolean, True if the address is not reserved per
            iana-ipv4-special-registry or iana-ipv6-special-registry.

        """

    @property
    def is_link_local(self) -> bool:
        """Test if the address is reserved for link-local.

        Returns:
            A boolean, True if the address is reserved per RFC 4291.

        """

    @property
    def is_loopback(self) -> bool:
        """Test if the address is a loopback address.

        Returns:
            A boolean, True if the address is a loopback address as defined in
            RFC 2373 2.5.3.

        """

    @property
    def is_multicast(self) -> bool:
        """Test if the address is reserved for multicast use.

        Returns:
            A boolean, True if the address is a multicast address.
            See RFC 2373 2.7 for details.

        """

    @property
    def is_private(self) -> bool:
        """Test if this network belongs to a private range.

        Returns:
            A boolean, True if the network is reserved per
            iana-ipv4-special-registry or iana-ipv6-special-registry.

        """

    @property
    def is_reserved(self) -> bool:
        """Test if the address is otherwise IETF reserved.

        Returns:
            A boolean, True if the address is within one of the
            reserved IPv6 Network ranges.

        """

    @property
    def is_unspecified(self) -> bool:
        """Test if the address is unspecified.

        Returns:
            A boolean, True if this is the unspecified address as defined in
            RFC 2373 2.5.2.

        """

    @property
    def num_addresses(self) -> int:
        """Number of hosts in the current subnet."""

    def overlaps(self, other: _BaseNetwork[IPv4Address] | _BaseNetwork[IPv6Address]) -> bool:
        """Tell if self is partly contained in other."""

    @property
    def prefixlen(self) -> int: ...
    def subnet_of(self, other: Self) -> bool:
        """Return True if this network is a subnet of other."""

    def supernet_of(self, other: Self) -> bool:
        """Return True if this network is a supernet of other."""

    def subnets(self, prefixlen_diff: int = 1, new_prefix: int | None = None) -> Iterator[Self]:
        """The subnets which join to make the current subnet.

        In the case that self contains only one IP
        (self._prefixlen == 32 for IPv4 or self._prefixlen == 128
        for IPv6), yield an iterator with just ourself.

        Args:
            prefixlen_diff: An integer, the amount the prefix length
              should be increased by. This should not be set if
              new_prefix is also set.
            new_prefix: The desired new prefix length. This must be a
              larger number (smaller prefix) than the existing prefix.
              This should not be set if prefixlen_diff is also set.

        Returns:
            An iterator of IPv(4|6) objects.

        Raises:
            ValueError: The prefixlen_diff is too small or too large.
                OR
            prefixlen_diff and new_prefix are both set or new_prefix
              is a smaller number than the current prefix (smaller
              number means a larger network)

        """

    def supernet(self, prefixlen_diff: int = 1, new_prefix: int | None = None) -> Self:
        """The supernet containing the current network.

        Args:
            prefixlen_diff: An integer, the amount the prefix length of
              the network should be decreased by.  For example, given a
              /24 network and a prefixlen_diff of 3, a supernet with a
              /21 netmask is returned.

        Returns:
            An IPv4 network object.

        Raises:
            ValueError: If self.prefixlen - prefixlen_diff < 0. I.e., you have
              a negative prefix length.
                OR
            If prefixlen_diff and new_prefix are both set or new_prefix is a
              larger number than the current prefix (larger number means a
              smaller network)

        """

    @property
    def with_hostmask(self) -> str: ...
    @property
    def with_netmask(self) -> str: ...
    @property
    def with_prefixlen(self) -> str: ...
    @property
    def hostmask(self) -> _A: ...

class _BaseV4:
    """Base IPv4 object.

    The following methods are used by IPv4 objects in both single IP
    addresses and networks.

    """

    __slots__ = ()
    if sys.version_info >= (3, 14):
        version: Final = 4
        max_prefixlen: Final = 32
    else:
        @property
        def version(self) -> Literal[4]: ...
        @property
        def max_prefixlen(self) -> Literal[32]: ...

class IPv4Address(_BaseV4, _BaseAddress):
    """Represent and manipulate single IPv4 Addresses."""

    __slots__ = ("_ip", "__weakref__")
    def __init__(self, address: object) -> None:
        """
        Args:
            address: A string or integer representing the IP

              Additionally, an integer can be passed, so
              IPv4Address('192.0.2.1') == IPv4Address(3221225985).
              or, more generally
              IPv4Address(int(IPv4Address('192.0.2.1'))) ==
                IPv4Address('192.0.2.1')

        Raises:
            AddressValueError: If ipaddress isn't a valid IPv4 address.

        """

    @property
    def is_global(self) -> bool:
        """``True`` if the address is defined as globally reachable by
        iana-ipv4-special-registry_ (for IPv4) or iana-ipv6-special-registry_
        (for IPv6) with the following exception:

        For IPv4-mapped IPv6-addresses the ``is_private`` value is determined by the
        semantics of the underlying IPv4 addresses and the following condition holds
        (see :attr:`IPv6Address.ipv4_mapped`)::

            address.is_global == address.ipv4_mapped.is_global

        ``is_global`` has value opposite to :attr:`is_private`, except for the ``100.64.0.0/10``
        IPv4 range where they are both ``False``.
        """

    @property
    def is_link_local(self) -> bool:
        """Test if the address is reserved for link-local.

        Returns:
            A boolean, True if the address is link-local per RFC 3927.

        """

    @property
    def is_loopback(self) -> bool:
        """Test if the address is a loopback address.

        Returns:
            A boolean, True if the address is a loopback per RFC 3330.

        """

    @property
    def is_multicast(self) -> bool:
        """Test if the address is reserved for multicast use.

        Returns:
            A boolean, True if the address is multicast.
            See RFC 3171 for details.

        """

    @property
    def is_private(self) -> bool:
        """``True`` if the address is defined as not globally reachable by
        iana-ipv4-special-registry_ (for IPv4) or iana-ipv6-special-registry_
        (for IPv6) with the following exceptions:

        * ``is_private`` is ``False`` for ``100.64.0.0/10``
        * For IPv4-mapped IPv6-addresses the ``is_private`` value is determined by the
            semantics of the underlying IPv4 addresses and the following condition holds
            (see :attr:`IPv6Address.ipv4_mapped`)::

                address.is_private == address.ipv4_mapped.is_private

        ``is_private`` has value opposite to :attr:`is_global`, except for the ``100.64.0.0/10``
        IPv4 range where they are both ``False``.
        """

    @property
    def is_reserved(self) -> bool:
        """Test if the address is otherwise IETF reserved.

        Returns:
            A boolean, True if the address is within the
            reserved IPv4 Network range.

        """

    @property
    def is_unspecified(self) -> bool:
        """Test if the address is unspecified.

        Returns:
            A boolean, True if this is the unspecified address as defined in
            RFC 5735 3.

        """

    @property
    def packed(self) -> bytes:
        """The binary representation of this address."""
    if sys.version_info >= (3, 13):
        @property
        def ipv6_mapped(self) -> IPv6Address:
            """Return the IPv4-mapped IPv6 address.

            Returns:
                The IPv4-mapped IPv6 address per RFC 4291.

            """

class IPv4Network(_BaseV4, _BaseNetwork[IPv4Address]):
    """This class represents and manipulates 32-bit IPv4 network + addresses..

    Attributes: [examples for IPv4Network('192.0.2.0/27')]
        .network_address: IPv4Address('192.0.2.0')
        .hostmask: IPv4Address('0.0.0.31')
        .broadcast_address: IPv4Address('192.0.2.32')
        .netmask: IPv4Address('255.255.255.224')
        .prefixlen: 27

    """

    def __init__(self, address: object, strict: bool = True) -> None:
        """Instantiate a new IPv4 network object.

        Args:
            address: A string or integer representing the IP [& network].
              '192.0.2.0/24'
              '192.0.2.0/255.255.255.0'
              '192.0.2.0/0.0.0.255'
              are all functionally the same in IPv4. Similarly,
              '192.0.2.1'
              '192.0.2.1/255.255.255.255'
              '192.0.2.1/32'
              are also functionally equivalent. That is to say, failing to
              provide a subnetmask will create an object with a mask of /32.

              If the mask (portion after the / in the argument) is given in
              dotted quad form, it is treated as a netmask if it starts with a
              non-zero field (e.g. /255.0.0.0 == /8) and as a hostmask if it
              starts with a zero field (e.g. 0.255.255.255 == /8), with the
              single exception of an all-zero mask which is treated as a
              netmask == /0. If no mask is given, a default of /32 is used.

              Additionally, an integer can be passed, so
              IPv4Network('192.0.2.1') == IPv4Network(3221225985)
              or, more generally
              IPv4Interface(int(IPv4Interface('192.0.2.1'))) ==
                IPv4Interface('192.0.2.1')

        Raises:
            AddressValueError: If ipaddress isn't a valid IPv4 address.
            NetmaskValueError: If the netmask isn't valid for
              an IPv4 address.
            ValueError: If strict is True and a network address is not
              supplied.
        """

class IPv4Interface(IPv4Address):
    netmask: IPv4Address
    network: IPv4Network
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    @property
    def hostmask(self) -> IPv4Address: ...
    @property
    def ip(self) -> IPv4Address: ...
    @property
    def with_hostmask(self) -> str: ...
    @property
    def with_netmask(self) -> str: ...
    @property
    def with_prefixlen(self) -> str: ...

class _BaseV6:
    """Base IPv6 object.

    The following methods are used by IPv6 objects in both single IP
    addresses and networks.

    """

    __slots__ = ()
    if sys.version_info >= (3, 14):
        version: Final = 6
        max_prefixlen: Final = 128
    else:
        @property
        def version(self) -> Literal[6]: ...
        @property
        def max_prefixlen(self) -> Literal[128]: ...

class IPv6Address(_BaseV6, _BaseAddress):
    """Represent and manipulate single IPv6 Addresses."""

    __slots__ = ("_ip", "_scope_id", "__weakref__")
    def __init__(self, address: object) -> None:
        """Instantiate a new IPv6 address object.

        Args:
            address: A string or integer representing the IP

              Additionally, an integer can be passed, so
              IPv6Address('2001:db8::') ==
                IPv6Address(42540766411282592856903984951653826560)
              or, more generally
              IPv6Address(int(IPv6Address('2001:db8::'))) ==
                IPv6Address('2001:db8::')

        Raises:
            AddressValueError: If address isn't a valid IPv6 address.

        """

    @property
    def is_global(self) -> bool:
        """``True`` if the address is defined as globally reachable by
        iana-ipv4-special-registry_ (for IPv4) or iana-ipv6-special-registry_
        (for IPv6) with the following exception:

        For IPv4-mapped IPv6-addresses the ``is_private`` value is determined by the
        semantics of the underlying IPv4 addresses and the following condition holds
        (see :attr:`IPv6Address.ipv4_mapped`)::

            address.is_global == address.ipv4_mapped.is_global

        ``is_global`` has value opposite to :attr:`is_private`, except for the ``100.64.0.0/10``
        IPv4 range where they are both ``False``.
        """

    @property
    def is_link_local(self) -> bool:
        """Test if the address is reserved for link-local.

        Returns:
            A boolean, True if the address is reserved per RFC 4291.

        """

    @property
    def is_loopback(self) -> bool:
        """Test if the address is a loopback address.

        Returns:
            A boolean, True if the address is a loopback address as defined in
            RFC 2373 2.5.3.

        """

    @property
    def is_multicast(self) -> bool:
        """Test if the address is reserved for multicast use.

        Returns:
            A boolean, True if the address is a multicast address.
            See RFC 2373 2.7 for details.

        """

    @property
    def is_private(self) -> bool:
        """``True`` if the address is defined as not globally reachable by
        iana-ipv4-special-registry_ (for IPv4) or iana-ipv6-special-registry_
        (for IPv6) with the following exceptions:

        * ``is_private`` is ``False`` for ``100.64.0.0/10``
        * For IPv4-mapped IPv6-addresses the ``is_private`` value is determined by the
            semantics of the underlying IPv4 addresses and the following condition holds
            (see :attr:`IPv6Address.ipv4_mapped`)::

                address.is_private == address.ipv4_mapped.is_private

        ``is_private`` has value opposite to :attr:`is_global`, except for the ``100.64.0.0/10``
        IPv4 range where they are both ``False``.
        """

    @property
    def is_reserved(self) -> bool:
        """Test if the address is otherwise IETF reserved.

        Returns:
            A boolean, True if the address is within one of the
            reserved IPv6 Network ranges.

        """

    @property
    def is_unspecified(self) -> bool:
        """Test if the address is unspecified.

        Returns:
            A boolean, True if this is the unspecified address as defined in
            RFC 2373 2.5.2.

        """

    @property
    def packed(self) -> bytes:
        """The binary representation of this address."""

    @property
    def ipv4_mapped(self) -> IPv4Address | None:
        """Return the IPv4 mapped address.

        Returns:
            If the IPv6 address is a v4 mapped address, return the
            IPv4 mapped address. Return None otherwise.

        """

    @property
    def is_site_local(self) -> bool:
        """Test if the address is reserved for site-local.

        Note that the site-local address space has been deprecated by RFC 3879.
        Use is_private to test if this address is in the space of unique local
        addresses as defined by RFC 4193.

        Returns:
            A boolean, True if the address is reserved per RFC 3513 2.5.6.

        """

    @property
    def sixtofour(self) -> IPv4Address | None:
        """Return the IPv4 6to4 embedded address.

        Returns:
            The IPv4 6to4-embedded address if present or None if the
            address doesn't appear to contain a 6to4 embedded address.

        """

    @property
    def teredo(self) -> tuple[IPv4Address, IPv4Address] | None:
        """Tuple of embedded teredo IPs.

        Returns:
            Tuple of the (server, client) IPs or None if the address
            doesn't appear to be a teredo address (doesn't start with
            2001::/32)

        """

    @property
    def scope_id(self) -> str | None:
        """Identifier of a particular zone of the address's scope.

        See RFC 4007 for details.

        Returns:
            A string identifying the zone of the address if specified, else None.

        """

    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class IPv6Network(_BaseV6, _BaseNetwork[IPv6Address]):
    """This class represents and manipulates 128-bit IPv6 networks.

    Attributes: [examples for IPv6('2001:db8::1000/124')]
        .network_address: IPv6Address('2001:db8::1000')
        .hostmask: IPv6Address('::f')
        .broadcast_address: IPv6Address('2001:db8::100f')
        .netmask: IPv6Address('ffff:ffff:ffff:ffff:ffff:ffff:ffff:fff0')
        .prefixlen: 124

    """

    def __init__(self, address: object, strict: bool = True) -> None:
        """Instantiate a new IPv6 Network object.

        Args:
            address: A string or integer representing the IPv6 network or the
              IP and prefix/netmask.
              '2001:db8::/128'
              '2001:db8:0000:0000:0000:0000:0000:0000/128'
              '2001:db8::'
              are all functionally the same in IPv6.  That is to say,
              failing to provide a subnetmask will create an object with
              a mask of /128.

              Additionally, an integer can be passed, so
              IPv6Network('2001:db8::') ==
                IPv6Network(42540766411282592856903984951653826560)
              or, more generally
              IPv6Network(int(IPv6Network('2001:db8::'))) ==
                IPv6Network('2001:db8::')

            strict: A boolean. If true, ensure that we have been passed
              A true network address, eg, 2001:db8::1000/124 and not an
              IP address on a network, eg, 2001:db8::1/124.

        Raises:
            AddressValueError: If address isn't a valid IPv6 address.
            NetmaskValueError: If the netmask isn't valid for
              an IPv6 address.
            ValueError: If strict was True and a network address was not
              supplied.
        """

    @property
    def is_site_local(self) -> bool:
        """Test if the address is reserved for site-local.

        Note that the site-local address space has been deprecated by RFC 3879.
        Use is_private to test if this address is in the space of unique local
        addresses as defined by RFC 4193.

        Returns:
            A boolean, True if the address is reserved per RFC 3513 2.5.6.

        """

class IPv6Interface(IPv6Address):
    netmask: IPv6Address
    network: IPv6Network
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    @property
    def hostmask(self) -> IPv6Address: ...
    @property
    def ip(self) -> IPv6Address: ...
    @property
    def with_hostmask(self) -> str: ...
    @property
    def with_netmask(self) -> str: ...
    @property
    def with_prefixlen(self) -> str: ...

def v4_int_to_packed(address: int) -> bytes:
    """Represent an address as 4 packed bytes in network (big-endian) order.

    Args:
        address: An integer representation of an IPv4 IP address.

    Returns:
        The integer address packed as 4 bytes in network (big-endian) order.

    Raises:
        ValueError: If the integer is negative or too large to be an
          IPv4 IP address.

    """

def v6_int_to_packed(address: int) -> bytes:
    """Represent an address as 16 packed bytes in network (big-endian) order.

    Args:
        address: An integer representation of an IPv6 IP address.

    Returns:
        The integer address packed as 16 bytes in network (big-endian) order.

    """

# Third overload is technically incorrect, but convenient when first and last are return values of ip_address()
@overload
def summarize_address_range(first: IPv4Address, last: IPv4Address) -> Iterator[IPv4Network]:
    """Summarize a network range given the first and last IP addresses.

    Example:
        >>> list(summarize_address_range(IPv4Address('192.0.2.0'),
        ...                              IPv4Address('192.0.2.130')))
        ...                                #doctest: +NORMALIZE_WHITESPACE
        [IPv4Network('192.0.2.0/25'), IPv4Network('192.0.2.128/31'),
         IPv4Network('192.0.2.130/32')]

    Args:
        first: the first IPv4Address or IPv6Address in the range.
        last: the last IPv4Address or IPv6Address in the range.

    Returns:
        An iterator of the summarized IPv(4|6) network objects.

    Raise:
        TypeError:
            If the first and last objects are not IP addresses.
            If the first and last objects are not the same version.
        ValueError:
            If the last object is not greater than the first.
            If the version of the first address is not 4 or 6.

    """

@overload
def summarize_address_range(first: IPv6Address, last: IPv6Address) -> Iterator[IPv6Network]: ...
@overload
def summarize_address_range(
    first: IPv4Address | IPv6Address, last: IPv4Address | IPv6Address
) -> Iterator[IPv4Network] | Iterator[IPv6Network]: ...
def collapse_addresses(addresses: Iterable[_N]) -> Iterator[_N]:
    """Collapse a list of IP objects.

    Example:
        collapse_addresses([IPv4Network('192.0.2.0/25'),
                            IPv4Network('192.0.2.128/25')]) ->
                           [IPv4Network('192.0.2.0/24')]

    Args:
        addresses: An iterable of IPv4Network or IPv6Network objects.

    Returns:
        An iterator of the collapsed IPv(4|6)Network objects.

    Raises:
        TypeError: If passed a list of mixed version objects.

    """

@overload
def get_mixed_type_key(obj: _A) -> tuple[int, _A]:
    """Return a key suitable for sorting between networks and addresses.

    Address and Network objects are not sortable by default; they're
    fundamentally different so the expression

        IPv4Address('192.0.2.0') <= IPv4Network('192.0.2.0/24')

    doesn't make any sense.  There are some times however, where you may wish
    to have ipaddress sort these for you anyway. If you need to do this, you
    can use this function as the key= argument to sorted().

    Args:
      obj: either a Network or Address object.
    Returns:
      appropriate key.

    """

@overload
def get_mixed_type_key(obj: IPv4Network) -> tuple[int, IPv4Address, IPv4Address]: ...
@overload
def get_mixed_type_key(obj: IPv6Network) -> tuple[int, IPv6Address, IPv6Address]: ...

class AddressValueError(ValueError):
    """A Value Error related to the address."""

class NetmaskValueError(ValueError):
    """A Value Error related to the netmask."""
