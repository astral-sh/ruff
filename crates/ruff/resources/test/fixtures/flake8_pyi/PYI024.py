import collections

person: collections.namedtuple  # Y024 Use "typing.NamedTuple" instead of "collections.namedtuple"

from collections import namedtuple

person: namedtuple  # Y024 Use "typing.NamedTuple" instead of "collections.namedtuple"

person = namedtuple(
    "Person", ["name", "age"]
)  # Y024 Use "typing.NamedTuple" instead of "collections.namedtuple"
