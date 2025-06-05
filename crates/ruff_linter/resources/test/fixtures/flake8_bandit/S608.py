# single-line failures
query1 = "SELECT %s FROM table" % (var,) # bad
query2 = "SELECT var FROM " + table
query3 = "SELECT " + val + " FROM " + table
query4 = "SELECT {} FROM table;".format(var)
query5 = f"SELECT * FROM table WHERE var = {var}"

query6 = "DELETE FROM table WHERE var = %s" % (var,)
query7 = "DELETE FROM table WHERE VAR = " + var
query8 = "DELETE FROM " + table + "WHERE var = " + var
query9 = "DELETE FROM table WHERE var = {}".format(var)
query10 = f"DELETE FROM table WHERE var = {var}"

query11 = "INSERT INTO table VALUES (%s)" % (var,)
query12 = "INSERT INTO TABLE VALUES (" + var + ")"
query13 = "INSERT INTO {} VALUES ({})".format(table, var)
query14 = f"INSERT INTO {table} VALUES var = {var}"

query15 = "UPDATE %s SET var = %s" % (table, var)
query16 = "UPDATE " + table + " SET var = " + var
query17 = "UPDATE {} SET var = {}".format(table, var)
query18 = f"UPDATE {table} SET var = {var}"

query19 = "select %s from table" % (var,)
query20 = "select var from " + table
query21 = "select " + val + " from " + table
query22 = "select {} from table;".format(var)
query23 = f"select * from table where var = {var}"

query24 = "delete from table where var = %s" % (var,)
query25 = "delete from table where var = " + var
query26 = "delete from " + table + "where var = " + var
query27 = "delete from table where var = {}".format(var)
query28 = f"delete from table where var = {var}"

query29 = "insert into table values (%s)" % (var,)
query30 = "insert into table values (" + var + ")"
query31 = "insert into {} values ({})".format(table, var)
query32 = f"insert into {table} values var = {var}"

query33 = "update %s set var = %s" % (table, var)
query34 = "update " + table + " set var = " + var
query35 = "update {} set var = {}".format(table, var)
query36 = f"update {table} set var = {var}"

# multi-line failures
def query37():
    return """
    SELECT *
    FROM table
    WHERE var = %s
    """ % var

def query38():
    return """
    SELECT *
    FROM TABLE
    WHERE var =
    """ + var

def query39():
    return """
    SELECT *
    FROM table
    WHERE var = {}
    """.format(var)

def query40():
    return f"""
    SELECT *
    FROM table
    WHERE var = {var}
    """

def query41():
    return (
        "SELECT * "
        "FROM table "
        f"WHERE var = {var}"
    )

# # cursor-wrapped failures
query42 = cursor.execute("SELECT * FROM table WHERE var = %s" % var)
query43 = cursor.execute(f"SELECT * FROM table WHERE var = {var}")
query44 = cursor.execute("SELECT * FROM table WHERE var = {}".format(var))
query45 = cursor.executemany("SELECT * FROM table WHERE var = %s" % var, [])

# # pass
query = "SELECT * FROM table WHERE id = 1"
query = "DELETE FROM table WHERE id = 1"
query = "INSERT INTO table VALUES (1)"
query = "UPDATE table SET id = 1"
cursor.execute('SELECT * FROM table WHERE id = %s', var)
cursor.execute('SELECT * FROM table WHERE id = 1')
cursor.executemany('SELECT * FROM table WHERE id = %s', [var, var2])

# # INSERT without INTO (e.g. MySQL and derivatives)
query46 = "INSERT table VALUES (%s)" % (var,)

# # REPLACE (e.g. MySQL and derivatives, SQLite)
query47 = "REPLACE INTO table VALUES (%s)" % (var,)
query48 = "REPLACE table VALUES (%s)" % (var,)

query49 = "Deselect something that is not SQL even though it has a ' from ' somewhere in %s." % "there"

# # pass
["select colA from tableA"] + ["select colB from tableB"]
"SELECT * FROM " + (["table1"] if x > 0 else ["table2"])

# # errors
"SELECT * FROM " + ("table1" if x > 0 else "table2") # query50
"SELECT * FROM " + ("table1" if x > 0 else ["table2"]) # query51

# test cases from #12044

def query52():
    return f"""
SELECT {var}
    FROM bar
    """

def query53():
    return f"""
    SELECT
        {var}
    FROM bar
    """

def query54():
    return f"""
    SELECT {var}
    FROM
        bar
    """

query55 = f"""SELECT * FROM
{var}.table
"""

query56 = f"""SELECT *
FROM {var}.table
"""

query57 = f"""
SELECT *
FROM {var}.table
"""

query57 = f"""
PRESELECT *
FROM {var}.table
"""

# to be handled separately
# query58 = f"SELECT\
#  * FROM {var}.table"


# https://github.com/astral-sh/ruff/issues/15653
query59 = f"""
    SELECT *, foo
    FROM ({user_input}) raw
"""
query60 = f"""
    SELECT *,
        foo
    FROM ({user_input}) raw
"""

# https://github.com/astral-sh/ruff/issues/17967
query61 = f"SELECT * FROM table" # skip expressionless f-strings

# t-strings
query62 = t"SELECT * FROM table"
query63 = t"""
    SELECT *,
        foo
    FROM ({user_input}) raw
"""
query64 = f"update {t"{table}"} set var = {t"{var}"}"
query65 = t"update {f"{table}"} set var = {f"{var}"}"
