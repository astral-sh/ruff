# Should change
foo, bar, baz = [fn(x) for x in items]

foo, bar, baz =[fn(x) for x in items]

foo, bar, baz =          [fn(x) for x in items]

foo, bar, baz = [[i for i in fn(x)] for x in items]

foo, bar, baz = [
    fn(x)
    for x in items
]

# Should not change
foo = [fn(x) for x in items]

x, = [await foo for foo in bar]
