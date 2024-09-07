any([x.id for x in bar])
all([x.id for x in bar])
any(  # first comment
    [x.id for x in bar],  # second comment
)  # third comment
all(  # first comment
    [x.id for x in bar],  # second comment
)  # third comment
any({x.id for x in bar})

# OK
all(x.id for x in bar)
all(x.id for x in bar)
any(x.id for x in bar)
all((x.id for x in bar))
# we don't lint on these in stable yet
sum([x.val for x in bar])
min([x.val for x in bar])
max([x.val for x in bar])


async def f() -> bool:
    return all([await use_greeting(greeting) for greeting in await greetings()])


# Special comment handling
any(
    [  # lbracket comment
        # second line comment
        i.bit_count()
        # random middle comment
        for i in range(5)  # rbracket comment
    ]  # rpar comment
    # trailing comment
)

# Weird case where the function call, opening bracket, and comment are all
# on the same line.
any([  # lbracket comment
        # second line comment
        i.bit_count() for i in range(5)  # rbracket comment
    ]  # rpar comment
)

## Set comprehensions should only be linted
## when function is invariant under duplication of inputs

# should be linted...
any({x.id for x in bar})
all({x.id for x in bar})

# should be linted in preview...
min({x.id for x in bar})
max({x.id for x in bar})

# should not be linted...
sum({x.id for x in bar})


# https://github.com/astral-sh/ruff/issues/12891
from collections.abc import AsyncGenerator


async def test() -> None:
    async def async_gen() -> AsyncGenerator[bool, None]:
        yield True

    assert all([v async for v in async_gen()])  # OK
