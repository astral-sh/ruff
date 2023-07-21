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
        b  # b
        ,  # comma
        c  # c
        ): # colon
    ...  # body
    # body trailing own


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
