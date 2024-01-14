import hashlib
from hashlib import (
    blake2b,
    blake2s,
    md5,
    sha1,
    sha3_224,
    sha3_256,
    sha3_384,
    sha3_512,
    sha224,
)
from hashlib import sha256
from hashlib import sha256 as hash_algo
from hashlib import sha384, sha512, shake_128, shake_256

# these will match

blake2b().digest().hex()
blake2s().digest().hex()
md5().digest().hex()
sha1().digest().hex()
sha224().digest().hex()
sha256().digest().hex()
sha384().digest().hex()
sha3_224().digest().hex()
sha3_256().digest().hex()
sha3_384().digest().hex()
sha3_512().digest().hex()
sha512().digest().hex()
shake_128().digest(10).hex()
shake_256().digest(10).hex()

hashlib.sha256().digest().hex()

sha256(b"text").digest().hex()

hash_algo().digest().hex()

# not yet supported
h = sha256()
h.digest().hex()


# these will not

sha256().digest()
sha256().digest().hex("_")
sha256().digest().hex(bytes_per_sep=4)
sha256().hexdigest()

class Hash:
    def digest(self) -> bytes:
        return b""


Hash().digest().hex()
