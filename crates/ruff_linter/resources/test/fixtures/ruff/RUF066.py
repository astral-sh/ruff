# Test cases for RUF066 - DirectMemberImport

# Should trigger violations
from os import path  # RUF066
from pathlib import Path  # RUF066
from collections import defaultdict  # RUF066
from json import loads, dumps  # RUF066 (twice)

# Should NOT trigger - typing imports are exempt (per Google Style Guide 2.2.4.1)
from typing import Literal, Protocol, TypeVar, Union
from typing_extensions import NotRequired, Required
from collections.abc import Callable, Iterator
from six.moves import urllib  # Python 2/3 compatibility

# Should NOT trigger - module imports (what we want)
import os
import pathlib
import json

# Should NOT trigger - relative imports
from . import something
from .. import another_thing
from .module import Class

# Should NOT trigger - wildcard imports (handled by other rules)
from os import *
