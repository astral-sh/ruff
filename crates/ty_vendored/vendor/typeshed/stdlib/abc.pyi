"""Abstract Base Classes (ABCs) according to PEP 3119."""

import _typeshed
import sys
from _typeshed import SupportsWrite
from collections.abc import Callable
from typing import Any, Literal, TypeVar
from typing_extensions import Concatenate, ParamSpec, deprecated

_T = TypeVar("_T")
_R_co = TypeVar("_R_co", covariant=True)
_FuncT = TypeVar("_FuncT", bound=Callable[..., Any])
_P = ParamSpec("_P")

# These definitions have special processing in mypy
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

    __abstractmethods__: frozenset[str]
    if sys.version_info >= (3, 11):
        def __new__(
            mcls: type[_typeshed.Self], name: str, bases: tuple[type, ...], namespace: dict[str, Any], /, **kwargs: Any
        ) -> _typeshed.Self: ...
    else:
        def __new__(
            mcls: type[_typeshed.Self], name: str, bases: tuple[type, ...], namespace: dict[str, Any], **kwargs: Any
        ) -> _typeshed.Self: ...

    def __instancecheck__(cls: ABCMeta, instance: Any) -> bool:
        """Override for isinstance(instance, cls)."""

    def __subclasscheck__(cls: ABCMeta, subclass: type) -> bool:
        """Override for issubclass(subclass, cls)."""

    def _dump_registry(cls: ABCMeta, file: SupportsWrite[str] | None = None) -> None:
        """Debug helper to print the ABC registry."""

    def register(cls: ABCMeta, subclass: type[_T]) -> type[_T]:
        """Register a virtual subclass of an ABC.

        Returns the subclass, to allow usage as a class decorator.
        """

def abstractmethod(funcobj: _FuncT) -> _FuncT:
    """A decorator indicating abstract methods.

    Requires that the metaclass is ABCMeta or derived from it.  A
    class that has a metaclass derived from ABCMeta cannot be
    instantiated unless all of its abstract methods are overridden.
    The abstract methods can be called using any of the normal
    'super' call mechanisms.  abstractmethod() may be used to declare
    abstract methods for properties and descriptors.

    Usage:

        class C(metaclass=ABCMeta):
            @abstractmethod
            def my_abstract_method(self, arg1, arg2, argN):
                ...
    """

@deprecated("Deprecated since Python 3.3. Use `@classmethod` stacked on top of `@abstractmethod` instead.")
class abstractclassmethod(classmethod[_T, _P, _R_co]):
    """A decorator indicating abstract classmethods.

    Deprecated, use 'classmethod' with 'abstractmethod' instead:

        class C(ABC):
            @classmethod
            @abstractmethod
            def my_abstract_classmethod(cls, ...):
                ...

    """

    __isabstractmethod__: Literal[True]
    def __init__(self, callable: Callable[Concatenate[type[_T], _P], _R_co]) -> None: ...

@deprecated("Deprecated since Python 3.3. Use `@staticmethod` stacked on top of `@abstractmethod` instead.")
class abstractstaticmethod(staticmethod[_P, _R_co]):
    """A decorator indicating abstract staticmethods.

    Deprecated, use 'staticmethod' with 'abstractmethod' instead:

        class C(ABC):
            @staticmethod
            @abstractmethod
            def my_abstract_staticmethod(...):
                ...

    """

    __isabstractmethod__: Literal[True]
    def __init__(self, callable: Callable[_P, _R_co]) -> None: ...

@deprecated("Deprecated since Python 3.3. Use `@property` stacked on top of `@abstractmethod` instead.")
class abstractproperty(property):
    """A decorator indicating abstract properties.

    Deprecated, use 'property' with 'abstractmethod' instead:

        class C(ABC):
            @property
            @abstractmethod
            def my_abstract_property(self):
                ...

    """

    __isabstractmethod__: Literal[True]

class ABC(metaclass=ABCMeta):
    """Helper class that provides a standard way to create an ABC using
    inheritance.
    """

    __slots__ = ()

def get_cache_token() -> object:
    """Returns the current ABC cache token.

    The token is an opaque object (supporting equality testing) identifying the
    current version of the ABC cache for virtual subclasses. The token changes
    with every call to register() on any ABC.
    """

if sys.version_info >= (3, 10):
    def update_abstractmethods(cls: type[_T]) -> type[_T]:
        """Recalculate the set of abstract methods of an abstract class.

        If a class has had one of its abstract methods implemented after the
        class was created, the method will not be considered implemented until
        this function is called. Alternatively, if a new abstract method has been
        added to the class, it will only be considered an abstract method of the
        class after this function is called.

        This function should be called before any use is made of the class,
        usually in class decorators that add methods to the subject class.

        Returns cls, to allow usage as a class decorator.

        If cls is not an instance of ABCMeta, does nothing.
        """
