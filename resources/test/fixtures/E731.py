#: E731
f = lambda x: 2 * x
#: E731
f = lambda x: 2 * x
#: E731
while False:
    this = lambda y, z: 2 * x


f = object()
#: E731
f.method = lambda: "Method"

f = {}
#: E731
f["a"] = lambda x: x ** 2

f = []
f.append(lambda x: x ** 2)

lambda: "no-op"
