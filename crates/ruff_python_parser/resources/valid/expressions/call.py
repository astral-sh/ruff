call()
call(1, 2)
call(1, 2, x=3, y=4)
call(*l)
call(**a)
call(*a, b, **l)
call(*a, *b)
call(
    [
        [a]
        for d in f
    ],
)
call(
    {
        [a]
        for d in f
    },
)
call(
    {
        A: [a]
        for d in f
    },
)
call(
    a=1 if True else None,
    x=0,
)
