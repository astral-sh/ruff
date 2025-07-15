from typing_extensions import Buffer, TypeAlias

_AdpcmState: TypeAlias = tuple[int, int]
_RatecvState: TypeAlias = tuple[int, tuple[tuple[int, int], ...]]

class error(Exception): ...

def add(fragment1: Buffer, fragment2: Buffer, width: int, /) -> bytes:
    """Return a fragment which is the addition of the two samples passed as parameters."""

def adpcm2lin(fragment: Buffer, width: int, state: _AdpcmState | None, /) -> tuple[bytes, _AdpcmState]:
    """Decode an Intel/DVI ADPCM coded fragment to a linear fragment."""

def alaw2lin(fragment: Buffer, width: int, /) -> bytes:
    """Convert sound fragments in a-LAW encoding to linearly encoded sound fragments."""

def avg(fragment: Buffer, width: int, /) -> int:
    """Return the average over all samples in the fragment."""

def avgpp(fragment: Buffer, width: int, /) -> int:
    """Return the average peak-peak value over all samples in the fragment."""

def bias(fragment: Buffer, width: int, bias: int, /) -> bytes:
    """Return a fragment that is the original fragment with a bias added to each sample."""

def byteswap(fragment: Buffer, width: int, /) -> bytes:
    """Convert big-endian samples to little-endian and vice versa."""

def cross(fragment: Buffer, width: int, /) -> int:
    """Return the number of zero crossings in the fragment passed as an argument."""

def findfactor(fragment: Buffer, reference: Buffer, /) -> float:
    """Return a factor F such that rms(add(fragment, mul(reference, -F))) is minimal."""

def findfit(fragment: Buffer, reference: Buffer, /) -> tuple[int, float]:
    """Try to match reference as well as possible to a portion of fragment."""

def findmax(fragment: Buffer, length: int, /) -> int:
    """Search fragment for a slice of specified number of samples with maximum energy."""

def getsample(fragment: Buffer, width: int, index: int, /) -> int:
    """Return the value of sample index from the fragment."""

def lin2adpcm(fragment: Buffer, width: int, state: _AdpcmState | None, /) -> tuple[bytes, _AdpcmState]:
    """Convert samples to 4 bit Intel/DVI ADPCM encoding."""

def lin2alaw(fragment: Buffer, width: int, /) -> bytes:
    """Convert samples in the audio fragment to a-LAW encoding."""

def lin2lin(fragment: Buffer, width: int, newwidth: int, /) -> bytes:
    """Convert samples between 1-, 2-, 3- and 4-byte formats."""

def lin2ulaw(fragment: Buffer, width: int, /) -> bytes:
    """Convert samples in the audio fragment to u-LAW encoding."""

def max(fragment: Buffer, width: int, /) -> int:
    """Return the maximum of the absolute value of all samples in a fragment."""

def maxpp(fragment: Buffer, width: int, /) -> int:
    """Return the maximum peak-peak value in the sound fragment."""

def minmax(fragment: Buffer, width: int, /) -> tuple[int, int]:
    """Return the minimum and maximum values of all samples in the sound fragment."""

def mul(fragment: Buffer, width: int, factor: float, /) -> bytes:
    """Return a fragment that has all samples in the original fragment multiplied by the floating-point value factor."""

def ratecv(
    fragment: Buffer,
    width: int,
    nchannels: int,
    inrate: int,
    outrate: int,
    state: _RatecvState | None,
    weightA: int = 1,
    weightB: int = 0,
    /,
) -> tuple[bytes, _RatecvState]:
    """Convert the frame rate of the input fragment."""

def reverse(fragment: Buffer, width: int, /) -> bytes:
    """Reverse the samples in a fragment and returns the modified fragment."""

def rms(fragment: Buffer, width: int, /) -> int:
    """Return the root-mean-square of the fragment, i.e. sqrt(sum(S_i^2)/n)."""

def tomono(fragment: Buffer, width: int, lfactor: float, rfactor: float, /) -> bytes:
    """Convert a stereo fragment to a mono fragment."""

def tostereo(fragment: Buffer, width: int, lfactor: float, rfactor: float, /) -> bytes:
    """Generate a stereo fragment from a mono fragment."""

def ulaw2lin(fragment: Buffer, width: int, /) -> bytes:
    """Convert sound fragments in u-LAW encoding to linearly encoded sound fragments."""
