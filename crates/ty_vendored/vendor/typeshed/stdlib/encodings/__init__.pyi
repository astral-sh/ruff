"""Standard "encodings" Package

    Standard Python encoding modules are stored in this package
    directory.

    Codec modules must have names corresponding to normalized encoding
    names as defined in the normalize_encoding() function below, e.g.
    'utf-8' must be implemented by the module 'utf_8.py'.

    Each codec module must export the following interface:

    * getregentry() -> codecs.CodecInfo object
    The getregentry() API must return a CodecInfo object with encoder, decoder,
    incrementalencoder, incrementaldecoder, streamwriter and streamreader
    attributes which adhere to the Python Codec Interface Standard.

    In addition, a module may optionally also define the following
    APIs which are then used by the package's codec search function:

    * getaliases() -> sequence of encoding name strings to use as aliases

    Alias names returned by getaliases() must be normalized encoding
    names as defined by normalize_encoding().

Written by Marc-Andre Lemburg (mal@lemburg.com).

(c) Copyright CNRI, All Rights Reserved. NO WARRANTY.

"""

import sys
from codecs import CodecInfo

class CodecRegistryError(LookupError, SystemError): ...

def normalize_encoding(encoding: str | bytes) -> str:
    """Normalize an encoding name.

    Normalization works as follows: all non-alphanumeric
    characters except the dot used for Python package names are
    collapsed and replaced with a single underscore, e.g. '  -;#'
    becomes '_'. Leading and trailing underscores are removed.

    Note that encoding names should be ASCII only.

    """

def search_function(encoding: str) -> CodecInfo | None: ...

if sys.version_info >= (3, 14) and sys.platform == "win32":
    def win32_code_page_search_function(encoding: str) -> CodecInfo | None: ...

# Needed for submodules
def __getattr__(name: str): ...  # incomplete module
