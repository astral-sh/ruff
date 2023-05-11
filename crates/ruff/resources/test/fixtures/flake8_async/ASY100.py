import urllib.request


async def foo():
    urllib.request.urlopen("http://example.com/foo/bar").read()

