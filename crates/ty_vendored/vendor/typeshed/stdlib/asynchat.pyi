"""A class supporting chat-style (command/response) protocols.

This class adds support for 'chat' style protocols - where one side
sends a 'command', and the other sends a response (examples would be
the common internet protocols - smtp, nntp, ftp, etc..).

The handle_read() method looks at the input stream for the current
'terminator' (usually '\\r\\n' for single-line responses, '\\r\\n.\\r\\n'
for multi-line output), calling self.found_terminator() on its
receipt.

for example:
Say you build an async nntp client using this class.  At the start
of the connection, you'll have self.terminator set to '\\r\\n', in
order to process the single-line greeting.  Just before issuing a
'LIST' command you'll set it to '\\r\\n.\\r\\n'.  The output of the LIST
command will be accumulated (using your own 'collect_incoming_data'
method) up to the terminator, and then control will be returned to
you - by calling your self.found_terminator() method.
"""

import asyncore
from abc import abstractmethod

class simple_producer:
    def __init__(self, data: bytes, buffer_size: int = 512) -> None: ...
    def more(self) -> bytes: ...

class async_chat(asyncore.dispatcher):
    """This is an abstract class.  You must derive from this class, and add
    the two methods collect_incoming_data() and found_terminator()
    """

    ac_in_buffer_size: int
    ac_out_buffer_size: int
    @abstractmethod
    def collect_incoming_data(self, data: bytes) -> None: ...
    @abstractmethod
    def found_terminator(self) -> None: ...
    def set_terminator(self, term: bytes | int | None) -> None:
        """Set the input delimiter.

        Can be a fixed string of any length, an integer, or None.
        """

    def get_terminator(self) -> bytes | int | None: ...
    def push(self, data: bytes) -> None: ...
    def push_with_producer(self, producer: simple_producer) -> None: ...
    def close_when_done(self) -> None:
        """automatically close this channel once the outgoing queue is empty"""

    def initiate_send(self) -> None: ...
    def discard_buffers(self) -> None: ...
