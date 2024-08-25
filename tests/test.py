def sql():
    foo = "foo"

    f"""
SELECT {foo}
    FROM bar
    """

    f"""
    SELECT
        {foo}
    FROM bar
    """

    f"""
    SELECT {foo}
    FROM
        bar
    """


foo = "foo"

f"SELECT * FROM {foo}.table"


f"""SELECT * FROM 
{foo}.table
"""

f"""SELECT *
FROM {foo}.table
"""

f"""
SELECT *
FROM {foo}.table
"""


f"SELECT\
 * FROM {foo}.table"
