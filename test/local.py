import foo as bar
import foo as baz
import foo.f1 as g1
import foo.f1 as g2
import __future__.bar as foo
import __future__.baz
import __future__.bar as foo
import __future__.baz
import wop
from . import bar
from .boop import bar
from wop import wop as wop
from wop import bop as bop
import foo, bar
import local_from

# [isort]
# import __future__.bar as foo
# import __future__.baz
#
# import bar
# import wop
# from wop import wop
#
# import foo
# import foo as bar
# import foo as baz
# import foo.f1 as g1
# import foo.f1 as g2
#
# from . import bar
# from .boop import bar
