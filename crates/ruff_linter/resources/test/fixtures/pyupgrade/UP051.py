import zipfile

try:
    pass
except zipfile.BadZipfile:
    pass

try:
    pass
except (zipfile.BadZipfile,):
    pass

raise (
    zipfile.
    # text
        BadZipfile
)


# multiple errors in tuple
from .mmap import error
try:
    pass
except (zipfile.BadZipfile, error):
    pass


# These should not change

try:
    pass
except zipfile.BadZipFile:
    pass

from foo import error

try:
    pass
except (zipfile.BadZipFile, error):
    pass

try:
    pass
except:
    pass

try:
    pass
except zipfile.BadZipFile:
    pass


try:
    pass
except (zipfile.BadZipFile, KeyError):
    pass

from zipfile import BadZipFile as BZP
try:
    pass
except (BZP, KeyError):
    pass

raise (
    zipfile.
    # text
        BadZipFile
)
