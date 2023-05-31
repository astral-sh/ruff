import collections

j: collections.namedtuple  # OK

from collections import namedtuple

j: namedtuple  # OK
