"""Registration facilities for DOM. This module should not be used
directly. Instead, the functions getDOMImplementation and
registerDOMImplementation should be imported from xml.dom.
"""

from _typeshed.xml import DOMImplementation
from collections.abc import Callable, Iterable

well_known_implementations: dict[str, str]
registered: dict[str, Callable[[], DOMImplementation]]

def registerDOMImplementation(name: str, factory: Callable[[], DOMImplementation]) -> None:
    """registerDOMImplementation(name, factory)

    Register the factory function with the name. The factory function
    should return an object which implements the DOMImplementation
    interface. The factory function can either return the same object,
    or a new one (e.g. if that implementation supports some
    customization).
    """

def getDOMImplementation(name: str | None = None, features: str | Iterable[tuple[str, str | None]] = ()) -> DOMImplementation:
    """getDOMImplementation(name = None, features = ()) -> DOM implementation.

    Return a suitable DOM implementation. The name is either
    well-known, the module name of a DOM implementation, or None. If
    it is not None, imports the corresponding module and returns
    DOMImplementation object if the import succeeds.

    If name is not given, consider the available implementations to
    find one with the required feature set. If no implementation can
    be found, raise an ImportError. The features list must be a sequence
    of (feature, version) pairs which are passed to hasFeature.
    """
