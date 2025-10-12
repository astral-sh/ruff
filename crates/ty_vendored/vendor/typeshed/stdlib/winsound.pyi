"""PlaySound(sound, flags) - play a sound
SND_FILENAME - sound is a wav file name
SND_ALIAS - sound is a registry sound association name
SND_LOOP - Play the sound repeatedly; must also specify SND_ASYNC
SND_MEMORY - sound is a memory image of a wav file
SND_PURGE - stop all instances of the specified sound
SND_ASYNC - PlaySound returns immediately
SND_NODEFAULT - Do not play a default beep if the sound can not be found
SND_NOSTOP - Do not interrupt any sounds currently playing
SND_NOWAIT - Return immediately if the sound driver is busy
SND_APPLICATION - sound is an application-specific alias in the registry.
SND_SENTRY - Triggers a SoundSentry event when the sound is played.
SND_SYNC - Play the sound synchronously, default behavior.
SND_SYSTEM - Assign sound to the audio session for system notification sounds.

Beep(frequency, duration) - Make a beep through the PC speaker.
MessageBeep(type) - Call Windows MessageBeep.
"""

import sys
from _typeshed import ReadableBuffer
from typing import Final, Literal, overload

if sys.platform == "win32":
    SND_APPLICATION: Final = 128
    SND_FILENAME: Final = 131072
    SND_ALIAS: Final = 65536
    SND_LOOP: Final = 8
    SND_MEMORY: Final = 4
    SND_PURGE: Final = 64
    SND_ASYNC: Final = 1
    SND_NODEFAULT: Final = 2
    SND_NOSTOP: Final = 16
    SND_NOWAIT: Final = 8192
    if sys.version_info >= (3, 14):
        SND_SENTRY: Final = 524288
        SND_SYNC: Final = 0
        SND_SYSTEM: Final = 2097152

    MB_ICONASTERISK: Final = 64
    MB_ICONEXCLAMATION: Final = 48
    MB_ICONHAND: Final = 16
    MB_ICONQUESTION: Final = 32
    MB_OK: Final = 0
    if sys.version_info >= (3, 14):
        MB_ICONERROR: Final = 16
        MB_ICONINFORMATION: Final = 64
        MB_ICONSTOP: Final = 16
        MB_ICONWARNING: Final = 48

    def Beep(frequency: int, duration: int) -> None:
        """A wrapper around the Windows Beep API.

        frequency
          Frequency of the sound in hertz.
          Must be in the range 37 through 32,767.
        duration
          How long the sound should play, in milliseconds.
        """
    # Can actually accept anything ORed with 4, and if not it's definitely str, but that's inexpressible
    @overload
    def PlaySound(sound: ReadableBuffer | None, flags: Literal[4]) -> None:
        """A wrapper around the Windows PlaySound API.

        sound
          The sound to play; a filename, data, or None.
        flags
          Flag values, ored together.  See module documentation.
        """

    @overload
    def PlaySound(sound: str | ReadableBuffer | None, flags: int) -> None: ...
    def MessageBeep(type: int = 0) -> None:
        """Call Windows MessageBeep(x).

        x defaults to MB_OK.
        """
