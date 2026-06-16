"""JSON Lines (JSONL) collector for the sampling profiler.

Emits a normalized newline-delimited JSON record stream suitable for
programmatic consumption by external tools, scripts, and agents. Each line
is one JSON object; consumers can parse the file incrementally line by
line, but the producer writes the whole file at the end of the run (it is
not a live/streaming producer).

Record schema
=============

Every record is a JSON object with at least ``"type"``, ``"v"`` (record
schema version), and ``"run_id"`` (UUID4 hex tagging the run; allows
demultiplexing concatenated streams). Records appear in this fixed order:

1. ``meta`` (exactly one, first line)::

      {"type":"meta","v":0,"run_id":"<hex>",
       "sample_interval_usec":<int>,"mode":"wall|cpu|gil|all|exception"}

   ``mode`` is omitted when not provided.

2. ``string_table`` (zero or more)::

      {"type":"string_table","v":0,"run_id":"<hex>",
       "strings":[{"str_id":<int>,"value":"<str>"}, ...]}

   Strings (filenames, function names) are interned to keep repeated values
   compact. IDs are zero-based. Each chunk holds up to ``_CHUNK_SIZE``
   entries, and each entry carries its explicit ``str_id`` so consumers do
   not need to infer offsets across chunks.

3. ``frame_table`` (zero or more)::

      {"type":"frame_table","v":0,"run_id":"<hex>",
       "frames":[{"frame_id":<int>,"path_str_id":<int>,"func_str_id":<int>,
                  "line":<int>,"end_line":<int>,"col":<int>,
                  "end_col":<int>}, ...]}

   ``end_line``/``col``/``end_col`` are *omitted* when source location data
   is unavailable (a missing key means "not available", not zero or null).
   ``line`` is ``0`` for synthetic frames (for example, internal marker
   frames whose source location is None). Frame IDs are zero-based.

4. ``agg`` (zero or more)::

      {"type":"agg","v":0,"run_id":"<hex>","kind":"frame","scope":"final",
       "samples_total":<int>,
       "entries":[{"frame_id":<int>,"self":<int>,"cumulative":<int>}, ...]}

   ``self`` counts samples where the frame was the leaf (currently
   executing); ``cumulative`` counts samples where the frame appeared
   anywhere in the stack (deduped per sample so recursion does not
   double-count). ``samples_total`` is the run-wide total, repeated on
   each chunk so a streaming consumer always knows the denominator.

5. ``end`` (exactly one, last line)::

      {"type":"end","v":0,"run_id":"<hex>","samples_total":<int>}

   Presence of ``end`` is the consumer's signal that the file is complete.

Forward compatibility
=====================

Consumers MUST ignore unknown record ``"type"`` values and unknown object
fields. New fields will be added by adding optional keys; an incompatible
schema change will bump the per-record ``"v"``.
"""

from _typeshed import StrOrBytesPath
from collections.abc import Sequence

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import _Frame, _Timestamps
from .stack_collector import StackTraceCollector

class JsonlCollector(StackTraceCollector):
    """Collector that exports finalized profiling data as JSONL.

    See the module docstring for the full record schema. The collector
    accumulates samples in memory and writes the complete file at
    ``export()`` time.
    """

    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False, mode: int | None = None) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], _thread_id: int, weight: int = 1) -> None: ...
