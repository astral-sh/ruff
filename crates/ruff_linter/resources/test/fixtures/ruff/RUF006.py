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


# Ok (false negative)
def f():
    task = asyncio.create_task(coordinator.ws_connect())


# Ok (potential false negative)
def f():
    do_nothing_with_the_task(asyncio.create_task(coordinator.ws_connect()))
