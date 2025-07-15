import _typeshed
from typing import Any, NewType, TypeVar

_T = TypeVar("_T")

_CacheToken = NewType("_CacheToken", int)

def get_cache_token() -> _CacheToken:
    """Returns the current ABC cache token.

    The token is an opaque object (supporting equality testing) identifying the
    current version of the ABC cache for virtual subclasses. The token changes
    with every call to ``register()`` on any ABC.
    """

class ABCMeta(type):
    """Metaclass for defining Abstract Base Classes (ABCs).

    Use this metaclass to create an ABC.  An ABC can be subclassed
    directly, and then acts as a mix-in class.  You can also register
    unrelated concrete classes (even built-in classes) and unrelated
    ABCs as 'virtual subclasses' -- these and their descendants will
    be considered subclasses of the registering ABC by the built-in
    issubclass() function, but the registering ABC won't show up in
    their MRO (Method Resolution Order) nor will method
    implementations defined by the registering ABC be callable (not
    even via super()).
    """

    def __new__(
        mcls: type[_typeshed.Self], name: str, bases: tuple[type[Any], ...], namespace: dict[str, Any], /
    ) -> _typeshed.Self: ...
    def register(cls, subclass: type[_T]) -> type[_T]:
        """Register a virtual subclass of an ABC.

        Returns the subclass, to allow usage as a class decorator.
        """
