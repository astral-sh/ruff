import weakref
from weakref import WeakKeyDictionary, WeakSet, WeakValueDictionary
import weakref as wr


def weakref_module(
    a=weakref.WeakKeyDictionary(),
    b=weakref.WeakValueDictionary(),
    c=weakref.WeakSet(),
):
    pass


def weakref_from_import(
    a=WeakKeyDictionary(),
    b=WeakValueDictionary(),
    c=WeakSet(),
):
    pass


def weakref_aliased(
    a=wr.WeakKeyDictionary(),
    b=wr.WeakValueDictionary(),
    c=wr.WeakSet(),
):
    pass


def weakref_parenthesized(
    a=(weakref.WeakKeyDictionary()),
    b=(WeakValueDictionary()),
    c=(wr.WeakSet()),
):
    pass


def weakref_subscripted(
    a=weakref.WeakKeyDictionary[int, str](),
    b=WeakValueDictionary[str, int](),
    c=wr.WeakSet[int](),
):
    pass
