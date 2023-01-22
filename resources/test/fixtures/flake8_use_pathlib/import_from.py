from os import chmod, mkdir, makedirs, rename, replace, rmdir
from os import remove, unlink, getcwd, readlink, stat
from os.path import abspath, exists, expanduser, isdir, isfile, islink
from os.path import isabs, join, basename, dirname, samefile, splitext

p = "/foo"

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
join(p)
basename(p)
dirname(p)
samefile(p)
splitext(p)
with open(p) as fp:
    fp.read()
open(p).close()
