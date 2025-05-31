# pylint: disable=unused-import, missing-docstring, invalid-name, reimported, import-error, wrong-import-order, no-name-in-module, shadowed-import
# Functional tests for import aliasing
# 1. useless-import-alias

import collections as collections
from collections import OrderedDict as OrderedDict
from . import foo as foo
from .foo import bar as bar
