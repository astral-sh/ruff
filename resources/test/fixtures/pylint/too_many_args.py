def foo(x,y,z,t,u,v,w,r): # Too many arguments (8/5)
    pass

def foo(x,y,z,t,u): #Ok
    pass

def foo(x): # Ok
    pass

def foo(x,y,z,_t,_u,_v,_w,r): # OK _.* can be ignored as per --ignored-argument-names configuration
    pass

def foo(x,y,z,ignored_u,unused_v,r): #OK ^ignored_|^unused_ can be ignored
    pass

def foo(x,y,z,u_ignored,v_unused,r): # Too many arguments (6/5)
    pass