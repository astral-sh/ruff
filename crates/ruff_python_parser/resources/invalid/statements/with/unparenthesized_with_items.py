# For parenthesized with items test cases, refer to `./ambiguous_lpar_with_items.py`

with item,: pass
with item as x,: pass
with *item: pass
with *item as x: pass
with *item1, item2 as f: pass
with item1 as f, *item2: pass
with item := 0 as f: pass