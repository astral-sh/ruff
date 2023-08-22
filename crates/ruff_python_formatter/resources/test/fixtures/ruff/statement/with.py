with aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:
    ...
    # trailing

with a, a:  # after colon
    ...
    # trailing

with (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
):
    ...
    # trailing


with (
        a  # a
        ,  # comma
        b  # c
        ): # colon
    ...


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

with (
        a  # a
        as  # as
        # own line
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  # b
): pass


with (a,):  # magic trailing comma
    ...


with (a):  # should remove brackets
    ...

with aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as c:
    ...


# currently unparsable by black: https://github.com/psf/black/issues/3678
with (name_2 for name_0 in name_4):
    pass
with (a, *b):
    pass

with (
    # leading comment
    a) as b: ...

with (
    # leading comment
    a as b
): ...

with (
    a as b
    # trailing comment
): ...

with (
    a as (
        # leading comment
        b
    )
): ...

with (
    a as (
        b
        # trailing comment
    )
): ...

with (a # trailing same line comment
    # trailing own line comment
    ) as b: ...

with (
    a # trailing same line comment
    # trailing own line comment
    as b
): ...

with (a # trailing same line comment
    # trailing own line comment
) as b: ...

with (
    (a
    # trailing own line comment
    )
    as # trailing as same line comment
    b # trailing b same line comment
): ...

with (
    [
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbb",
        "cccccccccccccccccccccccccccccccccccccccccc",
        dddddddddddddddddddddddddddddddd,
    ] as example1,
    aaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    + cccccccccccccccccccccccccccc
    + ddddddddddddddddd as example2,
    CtxManager2() as example2,
    CtxManager2() as example2,
    CtxManager2() as example2,
):
    ...

with [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "bbbbbbbbbb",
    "cccccccccccccccccccccccccccccccccccccccccc",
    dddddddddddddddddddddddddddddddd,
] as example1, aaaaaaaaaaaaaaaaaaaaaaaaaa * bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb * cccccccccccccccccccccccccccc + ddddddddddddddddd as example2, CtxManager222222222222222() as example2:
    ...

# Comments on open parentheses
with (  # comment
    CtxManager1() as example1,
    CtxManager2() as example2,
    CtxManager3() as example3,
):
    ...

with (  # outer comment
    (  # inner comment
        CtxManager1()
    ) as example1,
    CtxManager2() as example2,
    CtxManager3() as example3,
):
    ...

with (  # outer comment
    CtxManager()
) as example:
    ...

with (  # outer comment
    CtxManager()
) as example, (  # inner comment
    CtxManager2()
) as example2:
    ...

with (  # outer comment
    CtxManager1(),
    CtxManager2(),
) as example:
    ...

with (  # outer comment
    (  # inner comment
        CtxManager1()
    ),
    CtxManager2(),
) as example:
    ...

# Breaking of with items.
with (test  # bar
      as  # foo
      (
          # test
          foo)):
    pass

with test as (
    # test
    foo):
    pass

with (test  # bar
      as  # foo
      (  # baz
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
    a as (  # foo
        b
    )
):
    pass

with (
    f(a, ) as b

):
    pass

with (x := 1) as d:
    pass

with (x[1, 2,] as d):
    pass

with (f(a, ) as b, c as d):
    pass

with f(a, ) as b, c as d:
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

with (foo() as bar, baz() as bop):
    pass
