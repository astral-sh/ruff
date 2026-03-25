# Errors.
foo == foo

foo != foo

foo > foo

foo >= foo

foo < foo

foo <= foo

foo is foo

foo is not foo

foo in foo

foo not in foo

id(foo) == id(foo)

len(foo) == len(foo)

# Non-errors.
"foo" == "foo"  # This is flagged by `comparison-of-constant` instead.

foo == "foo"

foo == bar

foo != bar

foo > bar

foo >= bar

foo < bar

foo <= bar

foo is bar

foo is not bar

foo in bar

foo not in bar

x(foo) == y(foo)

id(foo) == id(bar)

id(foo, bar) == id(foo, bar)

id(foo, bar=1) == id(foo, bar=1)

# Preview errors (attribute, subscript, call).
self.x == self.x

a.b.c == a.b.c

a[0] == a[0]

obj.method() == obj.method()

a[0:2] == a[0:2]

# Preview non-errors.
self.x == self.y

a.b.c == a.b.d

a[0] == a[1]

obj.method() == other.method()

a.x == b.x
