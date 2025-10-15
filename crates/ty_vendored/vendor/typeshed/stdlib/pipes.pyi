"""Conversion pipeline templates.

The problem:
------------

Suppose you have some data that you want to convert to another format,
such as from GIF image format to PPM image format.  Maybe the
conversion involves several steps (e.g. piping it through compress or
uuencode).  Some of the conversion steps may require that their input
is a disk file, others may be able to read standard input; similar for
their output.  The input to the entire conversion may also be read
from a disk file or from an open file, and similar for its output.

The module lets you construct a pipeline template by sticking one or
more conversion steps together.  It will take care of creating and
removing temporary files if they are necessary to hold intermediate
data.  You can then use the template to do conversions from many
different sources to many different destinations.  The temporary
file names used are different each time the template is used.

The templates are objects so you can create templates for many
different conversion steps and store them in a dictionary, for
instance.


Directions:
-----------

To create a template:
    t = Template()

To add a conversion step to a template:
   t.append(command, kind)
where kind is a string of two characters: the first is '-' if the
command reads its standard input or 'f' if it requires a file; the
second likewise for the output. The command must be valid /bin/sh
syntax.  If input or output files are required, they are passed as
$IN and $OUT; otherwise, it must be  possible to use the command in
a pipeline.

To add a conversion step at the beginning:
   t.prepend(command, kind)

To convert a file to another file using a template:
  sts = t.copy(infile, outfile)
If infile or outfile are the empty string, standard input is read or
standard output is written, respectively.  The return value is the
exit status of the conversion pipeline.

To open a file for reading or writing through a conversion pipeline:
   fp = t.open(file, mode)
where mode is 'r' to read the file, or 'w' to write it -- just like
for the built-in function open() or for os.popen().

To create a new template object initialized to a given one:
   t2 = t.clone()
"""

import os

__all__ = ["Template"]

class Template:
    """Class representing a pipeline template."""

    def reset(self) -> None:
        """t.reset() restores a pipeline template to its initial state."""

    def clone(self) -> Template:
        """t.clone() returns a new pipeline template with identical
        initial state as the current one.
        """

    def debug(self, flag: bool) -> None:
        """t.debug(flag) turns debugging on or off."""

    def append(self, cmd: str, kind: str) -> None:
        """t.append(cmd, kind) adds a new step at the end."""

    def prepend(self, cmd: str, kind: str) -> None:
        """t.prepend(cmd, kind) adds a new step at the front."""

    def open(self, file: str, rw: str) -> os._wrap_close:
        """t.open(file, rw) returns a pipe or file object open for
        reading or writing; the file is the other end of the pipeline.
        """

    def copy(self, infile: str, outfile: str) -> int: ...

# Not documented, but widely used.
# Documented as shlex.quote since 3.3.
def quote(s: str) -> str:
    """Return a shell-escaped version of the string *s*."""
