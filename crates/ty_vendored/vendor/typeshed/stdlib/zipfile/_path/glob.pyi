import sys
from collections.abc import Iterator
from re import Match

if sys.version_info >= (3, 13):
    class Translator:
        """
        >>> Translator('xyz')
        Traceback (most recent call last):
        ...
        AssertionError: Invalid separators

        >>> Translator('')
        Traceback (most recent call last):
        ...
        AssertionError: Invalid separators
        """

        if sys.platform == "win32":
            def __init__(self, seps: str = "\\/") -> None: ...
        else:
            def __init__(self, seps: str = "/") -> None: ...

        def translate(self, pattern: str) -> str:
            """
            Given a glob pattern, produce a regex that matches it.
            """

        def extend(self, pattern: str) -> str:
            """
            Extend regex for pattern-wide concerns.

            Apply '(?s:)' to create a non-matching group that
            matches newlines (valid on Unix).

            Append '\\z' to imply fullmatch even when match is used.
            """

        def match_dirs(self, pattern: str) -> str:
            """
            Ensure that zipfile.Path directory names are matched.

            zipfile.Path directory names always end in a slash.
            """

        def translate_core(self, pattern: str) -> str:
            """
            Given a glob pattern, produce a regex that matches it.

            >>> t = Translator()
            >>> t.translate_core('*.txt').replace('\\\\\\\\', '')
            '[^/]*\\\\.txt'
            >>> t.translate_core('a?txt')
            'a[^/]txt'
            >>> t.translate_core('**/*').replace('\\\\\\\\', '')
            '.*/[^/][^/]*'
            """

        def replace(self, match: Match[str]) -> str:
            """
            Perform the replacements for a match from :func:`separate`.
            """

        def restrict_rglob(self, pattern: str) -> None:
            """
            Raise ValueError if ** appears in anything but a full path segment.

            >>> Translator().translate('**foo')
            Traceback (most recent call last):
            ...
            ValueError: ** must appear alone in a path segment
            """

        def star_not_empty(self, pattern: str) -> str:
            """
            Ensure that * will not match an empty segment.
            """

else:
    def translate(pattern: str) -> str:
        """
        Given a glob pattern, produce a regex that matches it.

        >>> translate('*.txt')
        '[^/]*\\\\.txt'
        >>> translate('a?txt')
        'a.txt'
        >>> translate('**/*')
        '.*/[^/]*'
        """

    def match_dirs(pattern: str) -> str:
        """
        Ensure that zipfile.Path directory names are matched.

        zipfile.Path directory names always end in a slash.
        """

    def translate_core(pattern: str) -> str:
        """
        Given a glob pattern, produce a regex that matches it.

        >>> translate('*.txt')
        '[^/]*\\\\.txt'
        >>> translate('a?txt')
        'a.txt'
        >>> translate('**/*')
        '.*/[^/]*'
        """

    def replace(match: Match[str]) -> str:
        """
        Perform the replacements for a match from :func:`separate`.
        """

def separate(pattern: str) -> Iterator[Match[str]]:
    """
    Separate out character sets to avoid translating their contents.

    >>> [m.group(0) for m in separate('*.txt')]
    ['*.txt']
    >>> [m.group(0) for m in separate('a[?]txt')]
    ['a', '[?]', 'txt']
    """
