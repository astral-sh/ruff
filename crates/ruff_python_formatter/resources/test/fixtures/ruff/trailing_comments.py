# Pragma reserved width fixtures
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # noqa: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # NoQA: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # type: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # pyright: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # pylint: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # noqa This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # nocoverage: This should break


# Pragma fixtures for non-breaking space (lead by NBSP)
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # noqa: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # NoQA: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # type: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # pyright: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # pylint: This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # noqa This shouldn't break
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # nocoverage: This should break


# As of adding this fixture Black adds a space before the non-breaking space if part of a type pragma.
# https://github.com/psf/black/blob/b4dca26c7d93f930bbd5a7b552807370b60d4298/src/black/comments.py#L122-L129
i = ""  #         type: Add space before leading NBSP followed by spaces
i = ""  #type: A space is added
i = ""  #  type: Add space before leading NBSP followed by a space
i = ""  # type: Add space before leading NBSP
i = ""  #  type: Add space before two leading NBSP


# A noqa as `#\u{A0}\u{A0}noqa` becomes `# \u{A0}noqa`
i = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"  #  noqa
