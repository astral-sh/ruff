"""Convert a NT pathname to a file URL and vice versa.

This module only exists to provide OS-specific code
for urllib.requests, thus do not use directly.
"""

import sys
from typing_extensions import deprecated

if sys.version_info >= (3, 14):
    @deprecated("The `nturl2path` module is deprecated since Python 3.14.")
    def url2pathname(url: str) -> str:
        """OS-specific conversion from a relative URL of the 'file' scheme
        to a file system path; not recommended for general use.
        """

    @deprecated("The `nturl2path` module is deprecated since Python 3.14.")
    def pathname2url(p: str) -> str:
        """OS-specific conversion from a file system path to a relative URL
        of the 'file' scheme; not recommended for general use.
        """

else:
    def url2pathname(url: str) -> str:
        """OS-specific conversion from a relative URL of the 'file' scheme
        to a file system path; not recommended for general use.
        """

    def pathname2url(p: str) -> str:
        """OS-specific conversion from a file system path to a relative URL
        of the 'file' scheme; not recommended for general use.
        """
