"""The functions in this module allow compression and decompression using the
zlib library, which is based on GNU zip.

adler32(string[, start]) -- Compute an Adler-32 checksum.
compress(data[, level]) -- Compress data, with compression level 0-9 or -1.
compressobj([level[, ...]]) -- Return a compressor object.
crc32(string[, start]) -- Compute a CRC-32 checksum.
decompress(string,[wbits],[bufsize]) -- Decompresses a compressed string.
decompressobj([wbits[, zdict]]) -- Return a decompressor object.

'wbits' is window buffer size and container format.
Compressor objects support compress() and flush() methods; decompressor
objects support decompress() and flush().
"""

from zlib import *
