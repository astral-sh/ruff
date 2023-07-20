l = [1,2,3,4]


def foo():
    yield from l # some comment

    # weird indents
    yield\
                    from\
                        l
                    # indented trailing comment
