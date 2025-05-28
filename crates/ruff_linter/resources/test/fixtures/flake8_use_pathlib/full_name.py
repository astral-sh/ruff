import os
import os.path

p = "/foo"
q = "bar"

a = os.path.abspath(p)
aa = os.chmod(p)
aaa = os.mkdir(p)
os.makedirs(p)
os.rename(p)
os.replace(p)
os.rmdir(p)
os.remove(p)
os.unlink(p)
os.getcwd(p)
b = os.path.exists(p)
bb = os.path.expanduser(p)
bbb = os.path.isdir(p)
bbbb = os.path.isfile(p)
bbbbb = os.path.islink(p)
os.readlink(p)
os.stat(p)
os.path.isabs(p)
os.path.join(p, q)
os.sep.join([p, q])
os.sep.join((p, q))
os.path.basename(p)
os.path.dirname(p)
os.path.samefile(p)
os.path.splitext(p)
with open(p) as fp:
    fp.read()
open(p).close()
os.getcwdb(p)
os.path.join(p, *q)
os.sep.join(p, *q)

# https://github.com/astral-sh/ruff/issues/7620
def opener(path, flags):
    return os.open(path, flags, dir_fd=os.open('somedir', os.O_RDONLY))


open(p, closefd=False)
open(p, opener=opener)
open(p, mode='r', buffering=-1, encoding=None, errors=None, newline=None, closefd=True, opener=None)
open(p, 'r', - 1, None, None, None, True, None)
open(p, 'r', - 1, None, None, None, False, opener)

# Cannot be upgraded `pathlib.Open` does not support fds
# See https://github.com/astral-sh/ruff/issues/12871
open(1)
open(1, "w")
x = 2
open(x)
def foo(y: int):
    open(y)


# https://github.com/astral-sh/ruff/issues/17691
def f() -> int:
    return 1
open(f())

open(b"foo")
byte_str = b"bar"
open(byte_str)

def bytes_str_func() -> bytes:
    return b"foo"
open(bytes_str_func())

# https://github.com/astral-sh/ruff/issues/17693
os.stat(1)
os.stat(x)


def func() -> int:
    return 2
os.stat(func())


def bar(x: int):
    os.stat(x)

# https://github.com/astral-sh/ruff/issues/17694
os.rename("src", "dst", src_dir_fd=3, dst_dir_fd=4)
os.rename("src", "dst", src_dir_fd=3)
os.rename("src", "dst", dst_dir_fd=4)

# if `dir_fd` is set, suppress the diagnostic
os.readlink(p, dir_fd=1)
os.stat(p, dir_fd=2)
os.unlink(p, dir_fd=3)
os.remove(p, dir_fd=4)
os.rmdir(p, dir_fd=5)
os.mkdir(p, dir_fd=6)
os.chmod(p, dir_fd=7)
# `chmod` can also receive a file descriptor in the first argument
os.chmod(8)
os.chmod(x)

# if `src_dir_fd` or `dst_dir_fd` are set, suppress the diagnostic
os.replace("src", "dst", src_dir_fd=1, dst_dir_fd=2)
os.replace("src", "dst", src_dir_fd=1)
os.replace("src", "dst", dst_dir_fd=2)
