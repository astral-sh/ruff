# parse_options: {"target-version": "3.15"}
lazy import foo
lazy import foo as bar
lazy from bar import baz
lazy from sys import x as y
lazy = 1
import foo as lazy
from lazy import qux
