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


async def f() -> bool:
    return all([await use_greeting(greeting) for greeting in await greetings()])
