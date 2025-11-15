import urllib.request

urllib.request.urlopen(url='http://www.google.com')
urllib.request.urlopen(url=f'http://www.google.com')
urllib.request.urlopen(url='http://' + 'www' + '.google.com')
urllib.request.urlopen(url='http://www.google.com', **kwargs)
urllib.request.urlopen(url=f'http://www.google.com', **kwargs)
urllib.request.urlopen('http://www.google.com')
urllib.request.urlopen(f'http://www.google.com')
urllib.request.urlopen('file:///foo/bar/baz')
urllib.request.urlopen(url)

urllib.request.Request(url='http://www.google.com')
urllib.request.Request(url=f'http://www.google.com')
urllib.request.Request(url='http://' + 'www' + '.google.com')
urllib.request.Request(url='http://www.google.com', **kwargs)
urllib.request.Request(url=f'http://www.google.com', **kwargs)
urllib.request.Request('http://www.google.com')
urllib.request.Request(f'http://www.google.com')
urllib.request.Request('file:///foo/bar/baz')
urllib.request.Request(url)

urllib.request.URLopener().open(fullurl='http://www.google.com')
urllib.request.URLopener().open(fullurl=f'http://www.google.com')
urllib.request.URLopener().open(fullurl='http://' + 'www' + '.google.com')
urllib.request.URLopener().open(fullurl='http://www.google.com', **kwargs)
urllib.request.URLopener().open(fullurl=f'http://www.google.com', **kwargs)
urllib.request.URLopener().open('http://www.google.com')
urllib.request.URLopener().open(f'http://www.google.com')
urllib.request.URLopener().open('http://' + 'www' + '.google.com')
urllib.request.URLopener().open('file:///foo/bar/baz')
urllib.request.URLopener().open(url)

urllib.request.urlopen(url=urllib.request.Request('http://www.google.com'))
urllib.request.urlopen(url=urllib.request.Request(f'http://www.google.com'))
urllib.request.urlopen(url=urllib.request.Request('http://' + 'www' + '.google.com'))
urllib.request.urlopen(url=urllib.request.Request('http://www.google.com'), **kwargs)
urllib.request.urlopen(url=urllib.request.Request(f'http://www.google.com'), **kwargs)
urllib.request.urlopen(urllib.request.Request('http://www.google.com'))
urllib.request.urlopen(urllib.request.Request(f'http://www.google.com'))
urllib.request.urlopen(urllib.request.Request('file:///foo/bar/baz'))
urllib.request.urlopen(urllib.request.Request(url))


# https://github.com/astral-sh/ruff/issues/15522
map(urllib.request.urlopen, [])
foo = urllib.request.urlopen

# https://github.com/astral-sh/ruff/issues/21462
path = "https://example.com/data.csv"
urllib.request.urlretrieve(path, "data.csv")
