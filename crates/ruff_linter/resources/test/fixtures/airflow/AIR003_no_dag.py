from airflow.models import Variable

# No Dag import → not a Dag file → no violations
foo = Variable.get("foo")

def helper():
    return Variable.get("bar")
