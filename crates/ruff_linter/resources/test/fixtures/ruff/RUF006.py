import asyncio


# Error
def f():
    asyncio.create_task(coordinator.ws_connect())  # Error


# Error
def f():
    asyncio.ensure_future(coordinator.ws_connect())  # Error


# OK
def f():
    background_tasks = set()

    for i in range(10):
        task = asyncio.create_task(some_coro(param=i))

        # Add task to the set. This creates a strong reference.
        background_tasks.add(task)

        # To prevent keeping references to finished tasks forever,
        # make each task remove its own reference from the set after
        # completion:
        task.add_done_callback(background_tasks.discard)


# OK
def f():
    background_tasks = set()

    for i in range(10):
        task = asyncio.ensure_future(some_coro(param=i))

        # Add task to the set. This creates a strong reference.
        background_tasks.add(task)

        # To prevent keeping references to finished tasks forever,
        # make each task remove its own reference from the set after
        # completion:
        task.add_done_callback(background_tasks.discard)


# OK
def f():
    ctx.task = asyncio.create_task(make_request())


# OK
def f():
    tasks.append(asyncio.create_task(self._populate_collection(coll, coll_info)))


# OK
def f():
    asyncio.wait([asyncio.create_task(client.close()) for client in clients.values()])


# OK
def f():
    tasks = [asyncio.create_task(task) for task in tasks]


# Error
def f():
    task = asyncio.create_task(coordinator.ws_connect())


# Error
def f():
    loop = asyncio.get_running_loop()
    task: asyncio.Task = loop.create_task(coordinator.ws_connect())


# OK (potential false negative)
def f():
    task = asyncio.create_task(coordinator.ws_connect())
    background_tasks.add(task)


# OK
async def f():
    task = asyncio.create_task(coordinator.ws_connect())
    await task


# OK (potential false negative)
def f():
    do_nothing_with_the_task(asyncio.create_task(coordinator.ws_connect()))


# Error
def f():
    loop = asyncio.get_running_loop()
    loop.create_task(coordinator.ws_connect())  # Error


# OK
def f():
    loop.create_task(coordinator.ws_connect())


# OK
def f():
    loop = asyncio.get_running_loop()
    loop.do_thing(coordinator.ws_connect())


# OK
async def f():
    task = unused = asyncio.create_task(coordinator.ws_connect())
    await task


# OK (false negative)
async def f():
    task = unused = asyncio.create_task(coordinator.ws_connect())


# OK
async def f():
    task[i] = asyncio.create_task(coordinator.ws_connect())


# OK
async def f(x: int):
    if x > 0:
        task = asyncio.create_task(make_request())
    else:
        task = asyncio.create_task(make_request())
    await task


# OK
async def f(x: bool):
    if x:
        t = asyncio.create_task(asyncio.sleep(1))
    else:
        t = None
    try:
        await asyncio.sleep(1)
    finally:
        if t:
            await t


# Error
async def f(x: bool):
    if x:
        t = asyncio.create_task(asyncio.sleep(1))
    else:
        t = None


# OK
async def f(x: bool):
    global T

    if x:
        T = asyncio.create_task(asyncio.sleep(1))
    else:
        T = None


# Error
def f():
    loop = asyncio.new_event_loop()
    loop.create_task(main()) # Error

# Error
def f():
    loop = asyncio.get_event_loop()
    loop.create_task(main()) # Error

# OK
def f():
    global task
    loop = asyncio.new_event_loop()
    task = loop.create_task(main()) # Error

# OK
def f():
    global task
    loop = asyncio.get_event_loop()
    task = loop.create_task(main()) # Error
