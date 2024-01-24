#: E401
import os, sys

#: Okay
import os
import sys

from subprocess import Popen, PIPE

from myclass import MyClass
from foo.bar.yourclass import YourClass

import myclass
import foo.bar.yourclass
#: Okay
__all__ = ['abc']

import foo
#: Okay
__version__ = "42"

import foo
#: Okay
__author__ = "Simon Gomizelj"

import foo
#: Okay
try:
    import foo
except ImportError:
    pass
else:
    print('imported foo')
finally:
    print('made attempt to import foo')

import bar
#: Okay
with warnings.catch_warnings():
    warnings.filterwarnings("ignore", DeprecationWarning)
    import foo

import bar
#: Okay
if False:
    import foo
elif not True:
    import bar
else:
    import mwahaha

import bar
#: E402
VERSION = '1.2.3'

import foo
#: E402
import foo

a = 1

import bar

#: E401
import re as regex, string  # also with a comment!
import re as regex, string; x = 1

x = 1; import re as regex, string


def blah():
    import datetime as dt, copy

    def nested_and_tested():
        import builtins, textwrap as tw

        x = 1; import re as regex, string
        import re as regex, string; x = 1

    if True: import re as regex, string
