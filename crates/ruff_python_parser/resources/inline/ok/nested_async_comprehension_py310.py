# parse_options: {"target-version": "3.10"}
# if all the comprehensions are async, it should be okay
async def test(): return [[x async for x in elements(n)] async for n in range(3)]
