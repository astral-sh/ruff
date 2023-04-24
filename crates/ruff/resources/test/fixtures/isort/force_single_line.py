import math
import sys
from json import detect_encoding
from json import dump
from json import dumps as json_dumps
from json import load
from json import loads as json_loads
from logging.handlers import FileHandler
from logging.handlers import StreamHandler
from os import path
from os import uname

# comment 6
from bar import a  # comment 7
from bar import b  # comment 8
from foo import bar  # comment 3
from foo2 import bar2  # comment 4
from foo3 import bar3  # comment 5
from foo3 import baz3  # comment 5

# comment 1
from third_party import lib1
from third_party import lib2
from third_party import lib3

# comment 2
from third_party import lib4
from third_party import lib5
from third_party import lib6
from third_party import lib7
