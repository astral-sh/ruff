import sys
from _typeshed import ReadableBuffer
from typing import final
from typing_extensions import Self

@final
class BZ2Compressor:
    """Create a compressor object for compressing data incrementally.

      compresslevel
        Compression level, as a number between 1 and 9.

    For one-shot compression, use the compress() function instead.
    """

    if sys.version_info >= (3, 12):
        def __new__(cls, compresslevel: int = 9, /) -> Self: ...
    else:
        def __init__(self, compresslevel: int = 9, /) -> None: ...

    def compress(self, data: ReadableBuffer, /) -> bytes:
        """Provide data to the compressor object.

        Returns a chunk of compressed data if possible, or b'' otherwise.

        When you have finished providing data to the compressor, call the
        flush() method to finish the compression process.
        """

    def flush(self) -> bytes:
        """Finish the compression process.

        Returns the compressed data left in internal buffers.

        The compressor object may not be used after this method is called.
        """

@final
class BZ2Decompressor:
    """Create a decompressor object for decompressing data incrementally.

    For one-shot decompression, use the decompress() function instead.
    """

    def decompress(self, data: ReadableBuffer, max_length: int = -1) -> bytes:
        """Decompress *data*, returning uncompressed data as bytes.

        If *max_length* is nonnegative, returns at most *max_length* bytes of
        decompressed data. If this limit is reached and further output can be
        produced, *self.needs_input* will be set to ``False``. In this case, the next
        call to *decompress()* may provide *data* as b'' to obtain more of the output.

        If all of the input data was decompressed and returned (either because this
        was less than *max_length* bytes, or because *max_length* was negative),
        *self.needs_input* will be set to True.

        Attempting to decompress data after the end of stream is reached raises an
        EOFError.  Any data found after the end of the stream is ignored and saved in
        the unused_data attribute.
        """

    @property
    def eof(self) -> bool:
        """True if the end-of-stream marker has been reached."""

    @property
    def needs_input(self) -> bool:
        """True if more input is needed before more decompressed data can be produced."""

    @property
    def unused_data(self) -> bytes:
        """Data found after the end of the compressed stream."""
