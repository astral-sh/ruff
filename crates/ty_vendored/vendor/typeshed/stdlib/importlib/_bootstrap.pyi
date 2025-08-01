"""Core implementation of import.

This module is NOT meant to be directly imported! It has been designed such
that it can be bootstrapped into Python as the implementation of import. As
such it requires the injection of specific modules and attributes in order to
work. One should use importlib as the public-facing version of this module.

"""

from _frozen_importlib import *
from _frozen_importlib import __import__ as __import__, _init_module_attrs as _init_module_attrs
