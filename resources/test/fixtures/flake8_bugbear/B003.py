"""
Should emit:
B003 - on line 9
"""

import os
from os import environ

os.environ = {}
environ = {}  # that's fine, assigning a new meaning to the module-level name


class Object:
    os = None


o = Object()
o.os.environ = {}
