"""Routines to help recognizing sound files.

Function whathdr() recognizes various types of sound file headers.
It understands almost all headers that SOX can decode.

The return tuple contains the following items, in this order:
- file type (as SOX understands it)
- sampling rate (0 if unknown or hard to decode)
- number of channels (0 if unknown or hard to decode)
- number of frames in the file (-1 if unknown or hard to decode)
- number of bits/sample, or 'U' for U-LAW, or 'A' for A-LAW

If the file doesn't have a recognizable type, it returns None.
If the file can't be opened, OSError is raised.

To compute the total time, divide the number of frames by the
sampling rate (a frame contains a sample for each channel).

Function what() calls whathdr().  (It used to also use some
heuristics for raw data, but this doesn't work very well.)

Finally, the function test() is a simple main program that calls
what() for all files mentioned on the argument list.  For directory
arguments it calls what() for all files in that directory.  Default
argument is "." (testing all files in the current directory).  The
option -r tells it to recurse down directories found inside
explicitly given directories.
"""

from _typeshed import StrOrBytesPath
from typing import NamedTuple

__all__ = ["what", "whathdr"]

class SndHeaders(NamedTuple):
    """SndHeaders(filetype, framerate, nchannels, nframes, sampwidth)"""

    filetype: str
    framerate: int
    nchannels: int
    nframes: int
    sampwidth: int | str

def what(filename: StrOrBytesPath) -> SndHeaders | None:
    """Guess the type of a sound file."""

def whathdr(filename: StrOrBytesPath) -> SndHeaders | None:
    """Recognize sound headers."""
