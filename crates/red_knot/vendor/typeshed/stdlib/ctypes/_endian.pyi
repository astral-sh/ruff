import sys
from _ctypes import RTLD_GLOBAL as RTLD_GLOBAL, RTLD_LOCAL as RTLD_LOCAL, Structure, Union
from ctypes import DEFAULT_MODE as DEFAULT_MODE, cdll as cdll, pydll as pydll, pythonapi as pythonapi

if sys.version_info >= (3, 12):
    from _ctypes import SIZEOF_TIME_T as SIZEOF_TIME_T

if sys.platform == "win32":
    from ctypes import oledll as oledll, windll as windll

# At runtime, the native endianness is an alias for Structure,
# while the other is a subclass with a metaclass added in.
class BigEndianStructure(Structure): ...
class LittleEndianStructure(Structure): ...

# Same thing for these: one is an alias of Union at runtime
if sys.version_info >= (3, 11):
    class BigEndianUnion(Union): ...
    class LittleEndianUnion(Union): ...
