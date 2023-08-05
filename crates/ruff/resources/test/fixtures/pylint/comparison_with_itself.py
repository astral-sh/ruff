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
