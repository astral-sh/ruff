import sys, math
from os import path, uname
from json import detect_encoding
from json import dump
from json import dumps as json_dumps
from json import load
from json import loads as json_loads
from logging.handlers import StreamHandler, FileHandler

# comment 1
from third_party import lib1, lib2, \
     lib3, lib7, lib5, lib6
# comment 2
from third_party import lib4

from foo import bar  # comment 3
from foo2 import bar2  # comment 4
from foo3 import bar3, baz3  # comment 5

# comment 6
from bar import (
     a, # comment 7
     b, # comment 8
)

# comment 9
from baz import *  # comment 10
