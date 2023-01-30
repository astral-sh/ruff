def foo(x,y,z,t,u,v,w,r): # Too many arguments (8/5)
    pass

def foo(x,y,z,t,u): #Ok
    pass

def foo(x): # Ok
    pass

def foo(x,y,z,_t,_u,_v,_w,r): # OK _.* are ignored
    pass

def foo(x,y,z,u=1,v=1,r=1): #Too many arguments (6/5)
    pass

def foo(x=1,y=1,z=1): #OK
    pass

def foo(x,y,z,/,u,v,w): #OK
    pass

def foo(x,y,z,*,u,v,w): #OK
    pass

def foo(x,y,z,a,b,c,*,u,v,w): #Too many arguments (6/5)
    pass
