import os

# Bad
os.environ['foo']

os.environ.get('foo')

os.environ.get('foo', 'bar')

os.getenv('foo')

# Good
os.environ['FOO']

os.environ.get('FOO')

os.environ.get('FOO', 'bar')

os.getenv('FOO')
