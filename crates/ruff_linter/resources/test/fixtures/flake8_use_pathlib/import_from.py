from os import chmod, mkdir, makedirs, rename, replace, rmdir, sep
from os import remove, unlink, getcwd, readlink, stat
from os.path import abspath, exists, expanduser, isdir, isfile, islink
from os.path import isabs, join, basename, dirname, samefile, splitext

p = "/foo"
q = "bar"

a = abspath(p)
aa = chmod(p)
aaa = mkdir(p)
makedirs(p)
rename(p)
replace(p)
rmdir(p)
remove(p)
unlink(p)
getcwd(p)
b = exists(p)
bb = expanduser(p)
bbb = isdir(p)
bbbb = isfile(p)
bbbbb = islink(p)
readlink(p)
stat(p)
isabs(p)
join(p, q)
sep.join((p, q))
sep.join([p, q])
basename(p)
dirname(p)
samefile(p)
splitext(p)
with open(p) as fp:
    fp.read()
open(p).close()


# https://github.com/astral-sh/ruff/issues/15442
def _():
    from builtins import open

    with open(p) as _: ...  # Error


def _():
    from builtin import open

    with open(p) as _: ...  # No error

file = "file_1.py"

rename(file, "file_2.py")

rename(
    # commment 1
    file, # comment 2
    "file_2.py"
    ,
    # comment 3
)

rename(file, "file_2.py", src_dir_fd=None, dst_dir_fd=None)

rename(file, "file_2.py", src_dir_fd=1)
