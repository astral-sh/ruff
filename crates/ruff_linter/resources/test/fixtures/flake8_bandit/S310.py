import urllib

urllib.urlopen(url='http://www.google.com')
urllib.urlopen(url='http://www.google.com', **kwargs)
urllib.urlopen('http://www.google.com')
urllib.urlopen('file:///foo/bar/baz')
urllib.urlopen(url)

urllib.Request(url='http://www.google.com', **kwargs)
urllib.Request(url='http://www.google.com')
urllib.Request('http://www.google.com')
urllib.Request('file:///foo/bar/baz')
urllib.Request(url)

urllib.URLopener().open(fullurl='http://www.google.com', **kwargs)
urllib.URLopener().open(fullurl='http://www.google.com')
urllib.URLopener().open('http://www.google.com')
urllib.URLopener().open('file:///foo/bar/baz')
urllib.URLopener().open(url)
