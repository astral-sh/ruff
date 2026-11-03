# `blocking-http-call-in-async-function` (`ASYNC210`)

```toml
lint.select = ["ASYNC210"]
```

## Generic request functions

The generic `requests.request` and `httpx.request` functions block just like the method-specific
helpers such as `get`. Imported aliases have the same behavior.

```py
import httpx
import requests
from httpx import request as httpx_request
from requests import request as requests_request

async def fetch(url):
    requests.get(url)  # error: [blocking-http-call-in-async-function]
    requests.request("GET", url)  # error: [blocking-http-call-in-async-function]
    requests_request("GET", url)  # error: [blocking-http-call-in-async-function]
    httpx.get(url)  # error: [blocking-http-call-in-async-function]
    httpx.request("GET", url)  # error: [blocking-http-call-in-async-function]
    httpx_request("GET", url)  # error: [blocking-http-call-in-async-function]
```

## Synchronous functions

The rule only checks calls in async contexts.

```py
import httpx
import requests

def fetch(url):
    requests.request("GET", url)
    httpx.request("GET", url)
```

## Shadowed module names

Parameters named `requests` and `httpx` do not refer to the imported libraries.

```py
import httpx
import requests

async def custom_client(requests, httpx, url):
    requests.request("GET", url)
    httpx.request("GET", url)
```

## Requests dispatched to a worker thread

Passing the request functions to `asyncio.to_thread` does not block the event loop.

```py
import asyncio
import httpx
import requests

async def fetch_in_thread(url):
    await asyncio.to_thread(requests.request, "GET", url)
    await asyncio.to_thread(httpx.request, "GET", url)
```
