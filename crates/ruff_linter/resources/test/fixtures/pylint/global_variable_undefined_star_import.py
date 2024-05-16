# pylint: disable=missing-module-docstring, missing-function-docstring
# pylint: disable=redefined-builtin, unnecessary-lambda-assignment
# pylint: disable=global-statement, unused-wildcard-import, wildcard-import
from os import *


# OK
def global_star_import():
    global system
    system = lambda _: None
