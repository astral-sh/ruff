foo = 1
bar = 2
baz = 3

# Errors.
foo = foo
bar = bar
foo, bar = foo, bar
bar, foo = bar, foo
(foo, bar) = (foo, bar)
(bar, foo) = (bar, foo)
foo, (bar, baz) = foo, (bar, baz)
bar, (foo, baz) = bar, (foo, baz)
(foo, bar), baz = (foo, bar), baz
(foo, (bar, baz)) = (foo, (bar, baz))
foo, bar = foo, 1
bar, foo = bar, 1
(foo, bar) = (foo, 1)
(bar, foo) = (bar, 1)
foo, (bar, baz) = foo, (bar, 1)
bar, (foo, baz) = bar, (foo, 1)
(foo, bar), baz = (foo, bar), 1
(foo, (bar, baz)) = (foo, (bar, 1))
foo: int = foo
bar: int = bar
foo = foo = bar
(foo, bar) = (foo, bar) = baz
(foo, bar) = baz = (foo, bar) = 1

# Non-errors.
foo = bar
bar = foo
foo, bar = bar, foo
foo, bar = bar, foo
(foo, bar) = (bar, foo)
foo, bar = bar, 1
bar, foo = foo, 1
foo: int = bar
bar: int = 1


class Foo:
    foo = foo
    bar = bar
