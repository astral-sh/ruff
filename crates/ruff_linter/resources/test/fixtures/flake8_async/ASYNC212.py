from typing import Optional

import httpx


def foo():
    client = httpx.Client()
    client.close()  # Ok
    client.delete()  # Ok
    client.get()  # Ok
    client.head()  # Ok
    client.options()  # Ok
    client.patch()  # Ok
    client.post()  # Ok
    client.put()  # Ok
    client.request()  # Ok
    client.send()  # Ok
    client.stream()  # Ok

    client.anything()  # Ok
    client.build_request()  # Ok
    client.is_closed  # Ok


async def foo():
    client = httpx.Client()
    client.close()  # ASYNC212
    client.delete()  # ASYNC212
    client.get()  # ASYNC212
    client.head()  # ASYNC212
    client.options()  # ASYNC212
    client.patch()  # ASYNC212
    client.post()  # ASYNC212
    client.put()  # ASYNC212
    client.request()  # ASYNC212
    client.send()  # ASYNC212
    client.stream()  # ASYNC212

    client.anything()  # Ok
    client.build_request()  # Ok
    client.is_closed  # Ok


async def foo(client: httpx.Client):
    client.request()  # ASYNC212
    client.anything()  # Ok


async def foo(client: httpx.Client | None):
    client.request()  # ASYNC212
    client.anything()  # Ok


async def foo(client: Optional[httpx.Client]):
    client.request()  # ASYNC212
    client.anything()  # Ok


async def foo():
    client: httpx.Client = ...
    client.request()  # ASYNC212
    client.anything()  # Ok


global_client = httpx.Client()


async def foo():
    global_client.request()  # ASYNC212
    global_client.anything()  # Ok


async def foo():
    async with httpx.AsyncClient() as client:
        await client.get()  # Ok
