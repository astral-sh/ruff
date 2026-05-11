from os import chmod as xchmod, mkdir as xmkdir, sep as s
from os import makedirs as xmakedirs, rename as xrename, replace as xreplace
from os import rmdir as xrmdir, remove as xremove, unlink as xunlink
from os import getcwd as xgetcwd, readlink as xreadlink, stat as xstat
from os.path import abspath as xabspath, exists as xexists
from os.path import expanduser as xexpanduser, isdir as xisdir
from os.path import isfile as xisfile, islink as xislink, isabs as xisabs
from os.path import join as xjoin, basename as xbasename, dirname as xdirname
from os.path import samefile as xsamefile, splitext as xsplitext

p = "/foo"
q = "bar"

a = xabspath(p)
aa = xchmod(p)
aaa = xmkdir(p)
xmakedirs(p)
xrename(p)
xreplace(p)
xrmdir(p)
xremove(p)
xunlink(p)
xgetcwd(p)
b = xexists(p)
bb = xexpanduser(p)
bbb = xisdir(p)
bbbb = xisfile(p)
bbbbb = xislink(p)
xreadlink(p)
xstat(p)
xisabs(p)
xjoin(p, q)
s.join((p, q))
s.join([p, q])
xbasename(p)
xdirname(p)
xsamefile(p)
xsplitext(p)

fd = 1


def get_fd() -> int:
    return fd


xexists(1)
xexists(fd)
xexists(get_fd())
xexists(path=1)
xisdir(1)
xisdir(fd)
xisdir(get_fd())
xisdir(s=1)
xisfile(1)
xisfile(fd)
xisfile(get_fd())
xisfile(path=1)
xsamefile(1, p)
xsamefile(fd, p)
xsamefile(get_fd(), p)
xsamefile(f1=1, f2=p)
