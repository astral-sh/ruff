"""Interface to the Expat non-validating XML parser."""

from pyexpat import *

# This is actually implemented in the C module pyexpat, but considers itself to live here.
class ExpatError(Exception):
    code: int
    lineno: int
    offset: int
