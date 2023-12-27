async def function():  # has two awaits
    async def factory():  # has one await
        var_one = await one()
    await two()