#: E701:1:5
if a: a = False
#: E701:1:40
if not header or header[:6] != 'bytes=': return
#: E702:1:10
a = False; b = True
#: E702:1:17
import bdist_egg; bdist_egg.write_safety_flag(cmd.egg_info, safe)
#: E703:1:13
import shlex;
#: E702:1:9 E703:1:23
del a[:]; a.append(42);
#: E704:1:1
def f(x): return 2
#: E704:1:1
async def f(x): return 2
#: E704:1:1 E271:1:6
async  def f(x): return 2
#: E704:1:1 E226:1:19
def f(x): return 2*x
#: E704:2:5 E226:2:23
while all is round:
    def f(x): return 2*x
#: E704:1:8 E702:1:11 E703:1:14
if True: x; y;
#: E701:1:8
if True: lambda a: b
#: E701:1:10
if a := 1: pass
# E701:1:4 E701:2:18 E701:3:8
try: lambda foo: bar
except ValueError: pass
finally: pass
# E701:1:7
class C: pass
# E701:1:7
with C(): pass
# E701:1:14
async with C(): pass
#:
lambda a: b
#:
a: List[str] = []
#:
if a := 1:
    pass
#:
func = lambda x: x** 2 if cond else lambda x:x
#:
class C: ...
#:
def f(): ...
#: E701:1:8 E702:1:13
class C: ...; x = 1
#: E701:1:8 E702:1:13
class C: ...; ...
#: E701:2:12
match *0, 1, *2:
    case 0,: y = 0
#:
class Foo:
    match: Optional[Match] = None
#: E702:2:4
while 1:
  1;...
#: E703:2:1
0\
;
#: E701:2:3
a = \
  5;
#:
with x(y) as z: ...
