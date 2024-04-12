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
