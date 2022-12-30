# module members cannot be imported with that syntax
import typing.TypedDict

# we don't track reassignments
import typing, other
typing = other
typing.TypedDict()

# yet another false positive
def foo(typing):
    typing.TypedDict()
