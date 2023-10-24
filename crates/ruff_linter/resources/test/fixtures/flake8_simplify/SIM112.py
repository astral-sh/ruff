import os

# Bad
os.environ['foo']

os.environ.get('foo')

os.environ.get('foo', 'bar')

os.getenv('foo')

env = os.environ.get('foo')

env = os.environ['foo']

if env := os.environ.get('foo'):
    pass

if env := os.environ['foo']:
    pass


# Good
os.environ['FOO']

os.environ.get('FOO')

os.environ.get('FOO', 'bar')

os.getenv('FOO')

env = os.getenv('FOO')

if env := os.getenv('FOO'):
    pass

env = os.environ['FOO']

if env := os.environ['FOO']:
    pass

os.environ['https_proxy']
os.environ.get['http_proxy']
os.getenv('no_proxy')
