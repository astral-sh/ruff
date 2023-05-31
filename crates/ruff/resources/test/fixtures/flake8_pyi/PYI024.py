import collections

person: collections.namedtuple  # OK

from collections import namedtuple

person: namedtuple  # OK

person = namedtuple("Person", ["name", "age"])  # OK
