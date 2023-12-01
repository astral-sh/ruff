import urllib.request
import requests
import httpx


async def foo():
    urllib.request.urlopen("http://example.com/foo/bar").read()


async def foo():
    requests.get()


async def foo():
    httpx.get()


async def foo():
    requests.post()


async def foo():
    httpx.post()
