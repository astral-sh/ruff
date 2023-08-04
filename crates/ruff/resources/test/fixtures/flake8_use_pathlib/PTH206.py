import os
from os import sep


file_name = "foo/bar"

# PTH206
"foo/bar/".split(os.sep)
"foo/bar/".split(sep=os.sep)
"foo/bar/".split(os.sep)[-1]
"foo/bar/".split(os.sep)[-2]
"foo/bar/".split(os.sep)[-2:]
"fizz/buzz".split(sep)
"fizz/buzz".split(sep)[-1]
os.path.splitext("path/to/hello_world.py")[0].split(os.sep)[-1]
file_name.split(os.sep)
(os.path.abspath(file_name)).split(os.sep)

# OK
"foo/bar/".split("/")
"foo/bar/".split(os.sep, 1)
"foo/bar/".split(1, sep=os.sep)
