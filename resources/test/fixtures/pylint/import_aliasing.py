# pylint: disable=unused-import, missing-docstring, invalid-name, reimported, import-error, wrong-import-order, no-name-in-module, shadowed-import
# Functional tests for import aliasing
# 1. useless-import-alias
# 2. consider-using-from-import

import collections as collections  # [useless-import-alias]
from collections import OrderedDict as OrderedDict  # [useless-import-alias]
from collections import OrderedDict as o_dict
import os.path as path  # [consider-using-from-import]
import os.path as p
import foo.bar.foobar as foobar  # [consider-using-from-import]
import os
import os as OS
from sys import version
from . import bar as bar  # [useless-import-alias]
from . import bar as Bar
from . import bar
from ..foo import bar as bar  # [useless-import-alias]
from ..foo.bar import foobar as foobar  # [useless-import-alias]
from ..foo.bar import foobar as anotherfoobar
from . import foo as foo, foo2 as bar2  # [useless-import-alias]
from . import foo as bar, foo2 as foo2  # [useless-import-alias]
from . import foo as bar, foo2 as bar2
from foo.bar import foobar as foobar  # [useless-import-alias]
from foo.bar import foobar as foo
from .foo.bar import f as foobar
from ............a import b  # [relative-beyond-top-level]
