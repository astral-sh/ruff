"""Encoding Aliases Support

This module is used by the encodings package search function to
map encodings names to module names.

Note that the search function normalizes the encoding names before
doing the lookup, so the mapping will have to map normalized
encoding names to module names.

Contents:

    The following aliases dictionary contains mappings of all IANA
    character set names for which the Python core library provides
    codecs. In addition to these, a few Python specific codec
    aliases have also been added.

"""

aliases: dict[str, str]
