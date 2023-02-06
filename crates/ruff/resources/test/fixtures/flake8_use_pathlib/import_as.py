import os as foo
import os.path as foo_p

p = "/foo"

a = foo_p.abspath(p)
aa = foo.chmod(p)
aaa = foo.mkdir(p)
foo.makedirs(p)
foo.rename(p)
foo.replace(p)
foo.rmdir(p)
foo.remove(p)
foo.unlink(p)
foo.getcwd(p)
b = foo_p.exists(p)
bb = foo_p.expanduser(p)
bbb = foo_p.isdir(p)
bbbb = foo_p.isfile(p)
bbbbb = foo_p.islink(p)
foo.readlink(p)
foo.stat(p)
foo_p.isabs(p)
foo_p.join(p)
foo_p.basename(p)
foo_p.dirname(p)
foo_p.samefile(p)
foo_p.splitext(p)
