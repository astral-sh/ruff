import os
import os.path
from os.path import commonprefix
from os import path

# Errors
os.path.commonprefix(["/usr/lib", "/usr/local/lib"])
commonprefix(["/usr/lib", "/usr/local/lib"])
path.commonprefix(["/usr/lib", "/usr/local/lib"])

# OK
os.path.commonpath(["/usr/lib", "/usr/local/lib"])

# Not a call — bare reference is fine
x = os.path.commonprefix


# User-defined function — no error
def commonprefix(paths):
    return paths[0]


commonprefix(["/usr/lib", "/usr/local/lib"])
