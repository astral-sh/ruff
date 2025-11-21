"""Stuff to parse Sun and NeXT audio files.

An audio file consists of a header followed by the data.  The structure
of the header is as follows.

        +---------------+
        | magic word    |
        +---------------+
        | header size   |
        +---------------+
        | data size     |
        +---------------+
        | encoding      |
        +---------------+
        | sample rate   |
        +---------------+
        | # of channels |
        +---------------+
        | info          |
        |               |
        +---------------+

The magic word consists of the 4 characters '.snd'.  Apart from the
info field, all header fields are 4 bytes in size.  They are all
32-bit unsigned integers encoded in big-endian byte order.

The header size really gives the start of the data.
The data size is the physical size of the data.  From the other
parameters the number of frames can be calculated.
The encoding gives the way in which audio samples are encoded.
Possible values are listed below.
The info field currently consists of an ASCII string giving a
human-readable description of the audio file.  The info field is
padded with NUL bytes to the header size.

Usage.

Reading audio files:
        f = sunau.open(file, 'r')
where file is either the name of a file or an open file pointer.
The open file pointer must have methods read(), seek(), and close().
When the setpos() and rewind() methods are not used, the seek()
method is not  necessary.

This returns an instance of a class with the following public methods:
        getnchannels()  -- returns number of audio channels (1 for
                           mono, 2 for stereo)
        getsampwidth()  -- returns sample width in bytes
        getframerate()  -- returns sampling frequency
        getnframes()    -- returns number of audio frames
        getcomptype()   -- returns compression type ('NONE' or 'ULAW')
        getcompname()   -- returns human-readable version of
                           compression type ('not compressed' matches 'NONE')
        getparams()     -- returns a namedtuple consisting of all of the
                           above in the above order
        getmarkers()    -- returns None (for compatibility with the
                           aifc module)
        getmark(id)     -- raises an error since the mark does not
                           exist (for compatibility with the aifc module)
        readframes(n)   -- returns at most n frames of audio
        rewind()        -- rewind to the beginning of the audio stream
        setpos(pos)     -- seek to the specified position
        tell()          -- return the current position
        close()         -- close the instance (make it unusable)
The position returned by tell() and the position given to setpos()
are compatible and have nothing to do with the actual position in the
file.
The close() method is called automatically when the class instance
is destroyed.

Writing audio files:
        f = sunau.open(file, 'w')
where file is either the name of a file or an open file pointer.
The open file pointer must have methods write(), tell(), seek(), and
close().

This returns an instance of a class with the following public methods:
        setnchannels(n) -- set the number of channels
        setsampwidth(n) -- set the sample width
        setframerate(n) -- set the frame rate
        setnframes(n)   -- set the number of frames
        setcomptype(type, name)
                        -- set the compression type and the
                           human-readable compression type
        setparams(tuple)-- set all parameters at once
        tell()          -- return current position in output file
        writeframesraw(data)
                        -- write audio frames without pathing up the
                           file header
        writeframes(data)
                        -- write audio frames and patch up the file header
        close()         -- patch up the file header and close the
                           output file
You should set the parameters before the first writeframesraw or
writeframes.  The total number of frames does not need to be set,
but when it is set to the correct value, the header does not have to
be patched up.
It is best to first set all parameters, perhaps possibly the
compression type, and then write audio frames using writeframesraw.
When all frames have been written, either call writeframes(b'') or
close() to patch up the sizes in the header.
The close() method is called automatically when the class instance
is destroyed.
"""

from _typeshed import Unused
from typing import IO, Any, Final, Literal, NamedTuple, NoReturn, overload
from typing_extensions import Self, TypeAlias

_File: TypeAlias = str | IO[bytes]

class Error(Exception): ...

AUDIO_FILE_MAGIC: Final = 0x2E736E64
AUDIO_FILE_ENCODING_MULAW_8: Final = 1
AUDIO_FILE_ENCODING_LINEAR_8: Final = 2
AUDIO_FILE_ENCODING_LINEAR_16: Final = 3
AUDIO_FILE_ENCODING_LINEAR_24: Final = 4
AUDIO_FILE_ENCODING_LINEAR_32: Final = 5
AUDIO_FILE_ENCODING_FLOAT: Final = 6
AUDIO_FILE_ENCODING_DOUBLE: Final = 7
AUDIO_FILE_ENCODING_ADPCM_G721: Final = 23
AUDIO_FILE_ENCODING_ADPCM_G722: Final = 24
AUDIO_FILE_ENCODING_ADPCM_G723_3: Final = 25
AUDIO_FILE_ENCODING_ADPCM_G723_5: Final = 26
AUDIO_FILE_ENCODING_ALAW_8: Final = 27
AUDIO_UNKNOWN_SIZE: Final = 0xFFFFFFFF

class _sunau_params(NamedTuple):
    """_sunau_params(nchannels, sampwidth, framerate, nframes, comptype, compname)"""

    nchannels: int
    sampwidth: int
    framerate: int
    nframes: int
    comptype: str
    compname: str

class Au_read:
    def __init__(self, f: _File) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def __del__(self) -> None: ...
    def getfp(self) -> IO[bytes] | None: ...
    def rewind(self) -> None: ...
    def close(self) -> None: ...
    def tell(self) -> int: ...
    def getnchannels(self) -> int: ...
    def getnframes(self) -> int: ...
    def getsampwidth(self) -> int: ...
    def getframerate(self) -> int: ...
    def getcomptype(self) -> str: ...
    def getcompname(self) -> str: ...
    def getparams(self) -> _sunau_params: ...
    def getmarkers(self) -> None: ...
    def getmark(self, id: Any) -> NoReturn: ...
    def setpos(self, pos: int) -> None: ...
    def readframes(self, nframes: int) -> bytes | None: ...

class Au_write:
    def __init__(self, f: _File) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def __del__(self) -> None: ...
    def setnchannels(self, nchannels: int) -> None: ...
    def getnchannels(self) -> int: ...
    def setsampwidth(self, sampwidth: int) -> None: ...
    def getsampwidth(self) -> int: ...
    def setframerate(self, framerate: float) -> None: ...
    def getframerate(self) -> int: ...
    def setnframes(self, nframes: int) -> None: ...
    def getnframes(self) -> int: ...
    def setcomptype(self, type: str, name: str) -> None: ...
    def getcomptype(self) -> str: ...
    def getcompname(self) -> str: ...
    def setparams(self, params: _sunau_params) -> None: ...
    def getparams(self) -> _sunau_params: ...
    def tell(self) -> int: ...
    # should be any bytes-like object after 3.4, but we don't have a type for that
    def writeframesraw(self, data: bytes) -> None: ...
    def writeframes(self, data: bytes) -> None: ...
    def close(self) -> None: ...

@overload
def open(f: _File, mode: Literal["r", "rb"]) -> Au_read: ...
@overload
def open(f: _File, mode: Literal["w", "wb"]) -> Au_write: ...
@overload
def open(f: _File, mode: str | None = None) -> Any: ...
