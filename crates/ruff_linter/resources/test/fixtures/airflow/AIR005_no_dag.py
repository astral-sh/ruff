from airflow.models import Variable

# No DAG import → not a DAG file → no violations
foo = Variable.get("foo")

def helper():
    return Variable.get("bar")
