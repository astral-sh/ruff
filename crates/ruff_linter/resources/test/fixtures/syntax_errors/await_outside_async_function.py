def func():
    await 1

# Top-level await
await 1

([await c for c in cor] async for cor in func())  # ok
