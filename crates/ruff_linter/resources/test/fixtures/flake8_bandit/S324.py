import hashlib
from hashlib import new as hashlib_new
from hashlib import sha1 as hashlib_sha1

# Invalid

hashlib.new('md5')

hashlib.new('md4', b'test')

hashlib.new(name='md5', data=b'test')

hashlib.new('MD4', data=b'test')

hashlib.new('sha1')

hashlib.new('sha1', data=b'test')

hashlib.new('sha', data=b'test')

hashlib.new(name='SHA', data=b'test')

hashlib.sha(data=b'test')

hashlib.md5()

hashlib_new('sha1')

hashlib_sha1('sha1')

# usedforsecurity arg only available in Python 3.9+
hashlib.new('sha1', usedforsecurity=True)

# Valid

hashlib.new('sha256')

hashlib.new('SHA512')

hashlib.sha256(data=b'test')

# usedforsecurity arg only available in Python 3.9+
hashlib_new(name='sha1', usedforsecurity=False)

# usedforsecurity arg only available in Python 3.9+
hashlib_sha1(name='sha1', usedforsecurity=False)

# usedforsecurity arg only available in Python 3.9+
hashlib.md4(usedforsecurity=False)

# usedforsecurity arg only available in Python 3.9+
hashlib.new(name='sha256', usedforsecurity=False)
