t1 = tuple([])
t2 = tuple([1, 2])
t3 = tuple((1, 2))
t4 = tuple([
    1,
    2
])
t5 = tuple(
    (1, 2)
)

tuple(  # comment
    [1, 2]
)

tuple([  # comment
    1, 2
])

tuple((
    1,
))

t6 = tuple([1])
t7 = tuple((1,))
t8 = tuple([1,])

tuple([x for x in range(5)])
tuple({x for x in range(10)})
tuple(x for x in range(5))
tuple([
    x for x in [1,2,3]
])
tuple( # comment
    [x for x in [1,2,3]]
)
tuple([ # comment
    x for x in range(10)
])
tuple(
    {
        x for x in [1,2,3]
    }
)

t9 = tuple([1],)
t10 = tuple([1, 2],)
