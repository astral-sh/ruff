# Correct

for _ in range(5):
    pass

_valid_type = int

_valid_var_1: _valid_type

_valid_var_1 = 1

_valid_var_2 = 2

_valid_var_3 = _valid_var_1 + _valid_var_2

def _valid_fun():
    pass    

_valid_fun()

def fun(arg):
    _valid_unused_var = arg
    pass

class _ValidClass:
    pass

_ValidClass()

class ClassOk:
    _valid_private_cls_attr = 1

    print(_valid_private_cls_attr)

    def __init__(self):
        self._valid_private_ins_attr = 2
        print(self._valid_private_ins_attr)

    def _valid_method(self):
        return self._valid_private_ins_attr

    def method(arg):
        _valid_unused_var = arg
        return 
    
# Correct if dummy_variable_re = "_+"

def fun(x):
    _ = 1
    __ = 2
    ___ = 3
    if x == 1:
        return _
    if x == 2:
        return __
    if x == 3:
        return ___
    return x

# Incorrect

class Class_:
    def fun(self):
        _var = "method variable"
        return _var # [RUF052]

def fun(_var):
    return _var # [RUF052]

def fun():
    _list = "built-in"
    return _list # [RUF052]

x = "global"

def fun():
    global x
    _x = "shadows global"
    return _x # [RUF052]

def foo():
  x = "outer"
  def bar():
    nonlocal x
    _x = "shadows nonlocal"
    return _x # [RUF052]
  bar()
  return x

def fun():
    x = "local"
    _x = "shadows local"
    return _x # [RUF052]
