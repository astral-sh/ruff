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

def fun(_valid_arg):
    return _valid_arg

def fun(_valid_arg):
    _valid_unused_var = _valid_arg
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
    
    def method(_valid_arg=3):
        _valid_unused_var = _valid_arg
        return 

# Incorrect

def fun(_valid_arg):
    _invalid_var = _valid_arg  # [WPS121]
    return _invalid_var

class ClassOk:
    
    def __init__(self):
        _ = 1
        __ = 2  # [WPS121]
        ___ = 3
        _invalid_type = int  # [WPS121]
        isinstance(_, _invalid_type)
        ___ = __ + _
        self._private_ins_attr_ok = 2
    
    def met(_valid_arg=3):
        _invalid_var_1 = _valid_arg  # [WPS121]
        
        _invalid_var_2 = 1  # [WPS121]
        if _valid_arg:
            return _invalid_var_1
        return _invalid_var_2
