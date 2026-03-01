"""Conversion functions between RGB and other color systems.

This modules provides two functions for each color system ABC:

  rgb_to_abc(r, g, b) --> a, b, c
  abc_to_rgb(a, b, c) --> r, g, b

All inputs and outputs are triples of floats in the range [0.0...1.0]
(with the exception of I and Q, which covers a slightly larger range).
Inputs outside the valid range may cause exceptions or invalid outputs.

Supported color systems:
RGB: Red, Green, Blue components
YIQ: Luminance, Chrominance (used by composite video signals)
HLS: Hue, Luminance, Saturation
HSV: Hue, Saturation, Value
"""

from typing import Final

__all__ = ["rgb_to_yiq", "yiq_to_rgb", "rgb_to_hls", "hls_to_rgb", "rgb_to_hsv", "hsv_to_rgb"]

def rgb_to_yiq(r: float, g: float, b: float) -> tuple[float, float, float]: ...
def yiq_to_rgb(y: float, i: float, q: float) -> tuple[float, float, float]: ...
def rgb_to_hls(r: float, g: float, b: float) -> tuple[float, float, float]: ...
def hls_to_rgb(h: float, l: float, s: float) -> tuple[float, float, float]: ...
def rgb_to_hsv(r: float, g: float, b: float) -> tuple[float, float, float]: ...
def hsv_to_rgb(h: float, s: float, v: float) -> tuple[float, float, float]: ...

# TODO: undocumented
ONE_SIXTH: Final[float]
ONE_THIRD: Final[float]
TWO_THIRD: Final[float]
