import lib6
import lib2
import lib5
import lib1
import lib3
import lib4

import foo
import z
from foo import bar
from lib1 import foo
from lib2 import foo
from lib1.lib2 import foo
from foo.lib1.bar import baz
from lib4 import lib1
from lib5 import lib2
from lib4 import lib2
from lib5 import lib1

import lib3.lib4
import lib3.lib4.lib5
from lib3.lib4 import foo
from lib3.lib4.lib5 import foo
