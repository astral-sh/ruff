with (
        a  # a
        as  # as
        # own line
        b  # b
        ,  # comma
        c  # c
        ): # colon
    ...  # body
    # body trailing own

with (test # bar
as # foo
    (
    # test
foo)):
    pass

with test as (
    # test
foo):
    pass

with (test # bar
as # foo
    ( # baz
    # test
foo)):
    pass

with (a as b, c as d):
    pass

with (
    a as b,
    # foo
    c as d
):
    pass

with (
    a as ( #  foo
    b
    )
):
    pass

with (
    f(a,) as b
    
):
    pass

with (x := 1) as d:
    pass


with (x[1, 2,] as d):
    pass


with (f(a,) as b, c as d):
    pass

with f(a,) as b, c as d:
    pass

with (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
) as b:
    pass

with aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as b:
    pass

with (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
) as b, c as d:
    pass

with (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as b, c as d):
    pass

with (
    (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) as b, c as d):
    pass

