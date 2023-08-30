# As of adding this fixture Black adds a space before the non-breaking space if part of a type pragma.
# https://github.com/psf/black/blob/b4dca26c7d93f930bbd5a7b552807370b60d4298/src/black/comments.py#L122-L129
i2 = ""  #         type: Add space before leading NBSP followed by spaces
i3 = ""  #type: A space is added
i4 = ""  #  type: Add space before leading NBSP followed by a space
i5 = ""  # type: Add space before leading NBSP
