import urllib
import requests
import httpx
import httpx2
import urllib3


async def foo():
    urllib.request.urlopen("http://example.com/foo/bar").read()  # ASYNC210


async def foo():
    requests.get()  # ASYNC210


async def foo():
    httpx.get()  # ASYNC210


async def foo():
    httpx2.get()  # ASYNC210


async def foo():
    requests.post()  # ASYNC210


async def foo():
    httpx.post()  # ASYNC210


async def foo():
    httpx2.post()  # ASYNC210


async def foo():
    requests.get()  # ASYNC210
    requests.get(...)  # ASYNC210
    requests.get  # Ok
    print(requests.get())  # ASYNC210
    print(requests.get(requests.get()))  # ASYNC210

    requests.options()  # ASYNC210
    requests.head()  # ASYNC210
    requests.post()  # ASYNC210
    requests.put()  # ASYNC210
    requests.patch()  # ASYNC210
    requests.delete()  # ASYNC210
    requests.foo()

    httpx.options("")  # ASYNC210
    httpx.head("")  # ASYNC210
    httpx.post("")  # ASYNC210
    httpx.put("")  # ASYNC210
    httpx.patch("")  # ASYNC210
    httpx.delete("")  # ASYNC210
    httpx.foo()  # Ok

    httpx2.options("")  # ASYNC210
    httpx2.head("")  # ASYNC210
    httpx2.post("")  # ASYNC210
    httpx2.put("")  # ASYNC210
    httpx2.patch("")  # ASYNC210
    httpx2.delete("")  # ASYNC210
    httpx2.foo()  # Ok

    urllib3.request()  # ASYNC210
    urllib3.request(...)  # ASYNC210

    urllib.request.urlopen("")  # ASYNC210

    r = {}
    r.get("not a sync http client")  # Ok


async def bar():
    def request():
        pass

    request()  # Ok

    def urlopen():
        pass

    urlopen()  # Ok
