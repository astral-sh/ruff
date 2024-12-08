from typing import Any


##### Errors


### Lists

def f(l: list[Any]): l.extend(("append",))                 # `l.append("append")` (safe)
def f(l: list[Any]): l.extend(["append"])                  # `l.append("append")` (safe)
def f(l: list[Any]): l.extend({"append"})                  # `l.append("append")` (safe)
def f(l: list[Any]): l.extend({"append"})                  # `l.append("append")` (safe)

def f(l: list[Any]): l += ("append",)                      # `l.append("append")` (safe)
def f(l: list[Any]): l += ["append"]                       # `l.append("append")` (safe)
def f(l: list[Any]): l += {"append"}                       # `l.append("append")` (safe)
def f(l: list[Any]): l += {"append": 0}                    # `l.append("append")` (safe)

def f(l: list[Any]): l += ("ext", "end")                   # `l.extend(("ext", "end"))` (safe)
def f(l: list[Any]): l += ["ext", "end"]                   # `l.extend(("ext", "end"))` (safe)
def f(l: list[Any]): l += {"ext", "end"}                   # `l.extend(("ext", "end"))` (safe)
def f(l: list[Any]): l += {"ext": 0, "end": 1}             # `l.extend(("ext", "end"))` (safe)

def f(l: list[Any]): l += (*extend,)                       # `l.extend(extend)` (safe)
def f(l: list[Any]): l += [*extend]                        # `l.extend(extend)` (safe)


### Sets
def f(s: set[Any]): s.update(("add",))                     # `s.add("add")` (safe)
def f(s: set[Any]): s.update(["add"])                      # `s.add("add")` (safe)
def f(s: set[Any]): s.update({"add"})                      # `s.add("add")` (safe)
def f(s: set[Any]): s.update({"add": 0})                   # `s.add("add")` (safe)

def f(s: set[Any]): s.difference_update(("discard",))      # `s.discard("discard")` (safe)
def f(s: set[Any]): s.difference_update(["discard"])       # `s.discard("discard")` (safe)
def f(s: set[Any]): s.difference_update({"discard"})       # `s.discard("discard")` (safe)
def f(s: set[Any]): s.difference_update({"discard": 0})    # `s.discard("discard")` (safe)

def f(s: set[Any]): s |= {"add"}                           # `s.add("add")`          (safe)
def f(s: set[Any]): s -= {"discard"}                       # `s.discard("discard"))` (safe)

def f(s: set[Any]): s |= {"upd", "ate"}                    # `s.update(("upd", "ate"))`            (safe)
def f(s: set[Any]): s -= {"upd", "ate"}                    # `s.difference_update(("upd", "ate"))` (safe)

def f(s: set[Any]): s |= {"upd", "ate"}                    # `s.update(("upd", "ate")`                       (safe)
def f(s: set[Any]): s &= {"upd", "ate"}                    # `s.intersection_update(("upd", "ate"))`         (safe)
def f(s: set[Any]): s -= {"upd", "ate"}                    # `s.difference_update(("upd", "ate"))`           (safe)
def f(s: set[Any]): s ^= {"upd", "ate"}                    # `s.symmetric_difference_update(("upd", "ate"))` (safe)

def f(s: set[Any]): s |= {*update}                         # `s.update(update)`            (safe)
def f(s: set[Any]): s -= {*update}                         # `s.difference_update(update)` (safe)


### Dictionaries

def f(d: dict[Any, Any]): d.update(**update)               # `d.update(update)` (safe)
def f(d: dict[Any, Any]): d.update({**update})             # `d.update(update)` (safe)

def f(d: dict[Any, Any]): d.update({"s": "et"})            # `d["s"] = "et"` (safe)
def f(d: dict[Any, Any]): d.update(s="et")                 # `d["s"] = "et"` (safe)

def f(d: dict[Any, Any]): d |= {"s": "et"}                 # `d["s"] = "et"` (safe)

def f(d: dict[Any, Any]): d |= {**update}                  # `d.update(update)` (safe)


##### No errors


### Lists

# Empty
def f(l: list[Any]): l += ()
def f(l: list[Any]): l += []
def f(l: list[Any]): l += {}

# Non-literals
def f(l: list[Any]): l += set()
def f(l: list[Any]): l += frozenset()

# Unpacks
def f(l: list[Any]): l += {*unpack}
def f(l: list[Any]): l += {**unpack}
def f(l: list[Any]): l += ("unp", *ack)
def f(l: list[Any]): l += ["unp", *ack]
def f(l: list[Any]): l += {"unp", *ack}
def f(l: list[Any]): l += {"u": "np", **ack}

# .append(iterable)
def f(l: list[Any]): l.append(("iterable",))
def f(l: list[Any]): l.append(["iterable"])
def f(l: list[Any]): l.append({"iterable"})
def f(l: list[Any]): l.append({"iter": "able"})

# .extend(name)
def f(l: list[Any]): l.extend(l)
def f(l: list[Any]): l.extend(s)
def f(l: list[Any]): l.extend(d)

# Comprehensions
def f(l: list[Any]): l += ("append" for _ in range(0))
def f(l: list[Any]): l += ["append" for _ in range(0)]
def f(l: list[Any]): l += {"append" for _ in range(0)}
def f(l: list[Any]): l += {"app": "end" for _ in range(0)}


### Sets

# Unpacks
def f(s: set[Any]): s |= {"unp", *ack}

# Non-literals
def f(s: set[Any]): s |= set()
def f(s: set[Any]): s |= frozenset()

# [&^]= single
def f(s: set[Any]): s &= {"update"}
def f(s: set[Any]): s ^= {"update"}

# [|&^-]= name
def f(s: set[Any]): s |= l
def f(s: set[Any]): s |= s
def f(s: set[Any]): s |= d
def f(s: set[Any]): s &= l
def f(s: set[Any]): s &= s
def f(s: set[Any]): s &= d
def f(s: set[Any]): s -= l
def f(s: set[Any]): s -= s
def f(s: set[Any]): s -= d
def f(s: set[Any]): s ^= l
def f(s: set[Any]): s ^= s
def f(s: set[Any]): s ^= d

# .*update(name)
def f(s: set[Any]): s.update(l)
def f(s: set[Any]): s.update(s)
def f(s: set[Any]): s.update(d)
def f(s: set[Any]): s.intersection_update(l)
def f(s: set[Any]): s.intersection_update(s)
def f(s: set[Any]): s.intersection_update(d)
def f(s: set[Any]): s.difference_update(l)
def f(s: set[Any]): s.difference_update(s)
def f(s: set[Any]): s.difference_update(d)
def f(s: set[Any]): s.symmetric_difference_update(l)
def f(s: set[Any]): s.symmetric_difference_update(s)
def f(s: set[Any]): s.symmetric_difference_update(d)

# Unsupported operations
def f(s: set[Any]): s |= ("upd", "ate")
def f(s: set[Any]): s |= ["upd", "ate"]
def f(s: set[Any]): s |= {"upd": 1, "ate": 2}
def f(s: set[Any]): s &= ("upd", "ate")
def f(s: set[Any]): s &= ["upd", "ate"]
def f(s: set[Any]): s &= {"upd": 1, "ate": 2}
def f(s: set[Any]): s -= ("upd", "ate")
def f(s: set[Any]): s -= ["upd", "ate"]
def f(s: set[Any]): s -= {"upd": 1, "ate": 2}
def f(s: set[Any]): s ^= ("upd", "ate")
def f(s: set[Any]): s ^= ["upd", "ate"]
def f(s: set[Any]): s ^= {"upd": 1, "ate": 2}

# Comprehensions
def f(s: set[Any]): s |= ("append" for _ in range(0))
def f(s: set[Any]): s |= ["append" for _ in range(0)]
def f(s: set[Any]): s |= {"append" for _ in range(0)}
def f(s: set[Any]): s |= {"app": "end" for _ in range(0)}
def f(s: set[Any]): s &= ("append" for _ in range(0))
def f(s: set[Any]): s &= ["append" for _ in range(0)]
def f(s: set[Any]): s &= {"append" for _ in range(0)}
def f(s: set[Any]): s &= {"app": "end" for _ in range(0)}
def f(s: set[Any]): s -= ("append" for _ in range(0))
def f(s: set[Any]): s -= ["append" for _ in range(0)]
def f(s: set[Any]): s -= {"append" for _ in range(0)}
def f(s: set[Any]): s -= {"app": "end" for _ in range(0)}
def f(s: set[Any]): s ^= ("append" for _ in range(0))
def f(s: set[Any]): s ^= ["append" for _ in range(0)]
def f(s: set[Any]): s ^= {"append" for _ in range(0)}
def f(s: set[Any]): s ^= {"app": "end" for _ in range(0)}


### Dictionaries

# Empty
def f(d: dict[Any, Any]): d |= {}

# Unpacks
def f(d: dict[Any, Any]): d |= {"unp": "ack", **unpack}

# Non-literals
def f(d: dict[Any, Any]): d |= dict()

# .update(name)
def f(d: dict[Any, Any]): d.update(l)
def f(d: dict[Any, Any]): d.update(s)
def f(d: dict[Any, Any]): d.update(d)

# Multiple items
def f(d: dict[Any, Any]): d |= {"mult": "iple", "it": "ems"}
def f(d: dict[Any, Any]): d.update({"mult": "iple", "it": "ems"})
def f(d: dict[Any, Any]): d.update(mult="iple", it="ems")

# Comprehensions
def f(d: dict[Any, Any]): d |= {"app": "end" for _ in range(0)}
def f(d: dict[Any, Any]): d.update({"app": "end" for _ in range(0)})
