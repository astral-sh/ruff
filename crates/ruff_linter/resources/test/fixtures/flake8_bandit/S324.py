import crypt
import hashlib
from hashlib import new as hashlib_new
from hashlib import sha1 as hashlib_sha1

# Errors
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

crypt.crypt("test", salt=crypt.METHOD_CRYPT)
crypt.crypt("test", salt=crypt.METHOD_MD5)
crypt.crypt("test", salt=crypt.METHOD_BLOWFISH)
crypt.crypt("test", crypt.METHOD_BLOWFISH)

crypt.mksalt(crypt.METHOD_CRYPT)
crypt.mksalt(crypt.METHOD_MD5)
crypt.mksalt(crypt.METHOD_BLOWFISH)

# OK
hashlib.new('sha256')
hashlib.new('SHA512')
hashlib.sha256(data=b'test')
# usedforsecurity arg only available in Python 3.9+
hashlib_new(name='sha1', usedforsecurity=False)
hashlib_sha1(name='sha1', usedforsecurity=False)
hashlib.md4(usedforsecurity=False)
hashlib.new(name='sha256', usedforsecurity=False)

crypt.crypt("test")
crypt.crypt("test", salt=crypt.METHOD_SHA256)
crypt.crypt("test", salt=crypt.METHOD_SHA512)

crypt.mksalt()
crypt.mksalt(crypt.METHOD_SHA256)
crypt.mksalt(crypt.METHOD_SHA512)
