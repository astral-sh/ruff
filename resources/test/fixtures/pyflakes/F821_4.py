"""Test: import alias tracking."""
from typing import List

_ = List["Model"]


from typing import List as IList

_ = IList["Model"]


from collections.abc import ItemsView

_ = ItemsView["Model"]


import collections.abc

_ = collections.abc.ItemsView["Model"]


from collections import abc

_ = abc.ItemsView["Model"]
