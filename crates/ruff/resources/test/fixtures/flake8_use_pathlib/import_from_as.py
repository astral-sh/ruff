from os import chmod as xchmod, mkdir as xmkdir
from os import makedirs as xmakedirs, rename as xrename, replace as xreplace
from os import rmdir as xrmdir, remove as xremove, unlink as xunlink
from os import getcwd as xgetcwd, readlink as xreadlink, stat as xstat
from os.path import abspath as xabspath, exists as xexists
from os.path import expanduser as xexpanduser, isdir as xisdir
from os.path import isfile as xisfile, islink as xislink, isabs as xisabs
from os.path import join as xjoin, basename as xbasename, dirname as xdirname
from os.path import samefile as xsamefile, splitext as xsplitext

p = "/foo"

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
xjoin(p)
xbasename(p)
xdirname(p)
xsamefile(p)
xsplitext(p)
