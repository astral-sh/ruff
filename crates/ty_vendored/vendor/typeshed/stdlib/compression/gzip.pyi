"""Functions that read and write gzipped files.

The user of the file doesn't have to worry about the compression,
but random access is not allowed.
"""

from gzip import *
