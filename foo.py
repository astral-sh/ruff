from typing import List

# OK, does error
_ = List["foo"]


from typing import List as IList

# Bug, should error
_ = IList["foo"]


from collections.abc import ItemsView

# OK, does error
_ = ItemsView["foo"]


import collections.abc

# Bug, should error
_ = collections.abc.ItemsView["foo"]


from collections import abc

# Bug, should error
_ = abc.ItemsView["foo"]
