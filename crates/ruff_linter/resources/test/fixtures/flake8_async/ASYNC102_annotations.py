import trio


async def cleanup():
    try:
        pass
    finally:
        async def annotated(value: await factory()) -> await factory():
            await work()

        # Defaults are evaluated regardless of the annotation semantics.
        async def default(value=await factory()):  # ASYNC102
            await work()
