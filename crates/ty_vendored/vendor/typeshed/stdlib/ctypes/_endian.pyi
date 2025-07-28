import sys
from ctypes import Structure, Union

# At runtime, the native endianness is an alias for Structure,
# while the other is a subclass with a metaclass added in.
class BigEndianStructure(Structure):
    """Structure with big endian byte order"""

class LittleEndianStructure(Structure):
    """Structure base class"""

# Same thing for these: one is an alias of Union at runtime
if sys.version_info >= (3, 11):
    class BigEndianUnion(Union):
        """Union with big endian byte order"""

    class LittleEndianUnion(Union):
        """Union base class"""
