l = [1,2,3,4]


def foo():
    yield from l # some comment

    # weird indents
    yield\
                    from\
                        l
                    # indented trailing comment

    a = yield from l

    with (
        # Comment
        yield from l
        # Comment
    ):
        pass

    c = [(yield from l) , (
        yield from l

    )]

    while (
        yield from l
    ):
        pass

    yield (
        yield from l
    )

