#: E221
a = 12 + 3
b = 4  + 5
#: E221 E221
x             = 1
y             = 2
long_variable = 3
#: E221 E221
x[0]          = 1
x[1]          = 2
long_variable = 3
#: E221 E221
x = f(x)          + 1
y = long_variable + 2
z = x[0]          + 3
#: E221:3:14
text = """
    bar
    foo %s"""  % rofl
#: Okay
x = 1
y = 2
long_variable = 3
#:


#: E222
a = a +  1
b = b + 10
#: E222 E222
x =            -1
y =            -2
long_variable = 3
#: E222 E222
x[0] =          1
x[1] =          2
long_variable = 3
#:


#: E223
foobart = 4
a	= 3  # aligned with tab
#:


#: E224
a +=	1
b += 1000
#:


#: E225
submitted +=1
#: E225
submitted+= 1
#: E225
c =-1
#: E225
x = x /2 - 1
#: E225
c = alpha -4
#: E225
c = alpha- 4
#: E225
z = x **y
#: E225
z = (x + 1) **y
#: E225
z = (x + 1)** y
#: E225
_1kB = _1MB >>10
#: E225
_1kB = _1MB>> 10
#: E225 E225
i=i+ 1
#: E225 E225
i=i +1
#: E225
i = 1and 1
#: E225
i = 1or 0
#: E225
1is 1
#: E225
1in []
#: E225
i = 1 @2
#: E225
i = 1@ 2
#: E225 E226
i=i+1
#: E225 E226
i =i+1
#: E225 E226
i= i+1
#: E225 E226
c = (a +b)*(a - b)
#: E225 E226
c = (a+ b)*(a - b)
#:

#: E226
z = 2//30
#: E226 E226
c = (a+b) * (a-b)
#: E226
norman = True+False
#: E226
x = x*2 - 1
#: E226
x = x/2 - 1
#: E226 E226
hypot2 = x*x + y*y
#: E226
c = (a + b)*(a - b)
#: E226
def halves(n):
    return (i//2 for i in range(n))
#: E227
_1kB = _1MB>>10
#: E227
_1MB = _1kB<<10
#: E227
a = b|c
#: E227
b = c&a
#: E227
c = b^a
#: E228
a = b%c
#: E228
msg = fmt%(errno, errmsg)
#: E228
msg = "Error %d occurred"%errno
#:

#: Okay
i = i + 1
submitted += 1
x = x * 2 - 1
hypot2 = x * x + y * y
c = (a + b) * (a - b)
_1MiB = 2 ** 20
_1TiB = 2**30
foo(bar, key='word', *args, **kwargs)
baz(**kwargs)
negative = -1
spam(-1)
-negative
func1(lambda *args, **kw: (args, kw))
func2(lambda a, b=h[:], c=0: (a, b, c))
if not -5 < x < +5:
    print >>sys.stderr, "x is out of range."
print >> sys.stdout, "x is an integer."
x = x / 2 - 1
x = 1 @ 2

if alpha[:-i]:
    *a, b = (1, 2, 3)


def squares(n):
    return (i**2 for i in range(n))


ENG_PREFIXES = {
    -6: "\u03bc",  # Greek letter mu
    -3: "m",
}
#:
