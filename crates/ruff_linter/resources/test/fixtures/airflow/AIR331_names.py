from __future__ import annotations

# airflow.traces (removed in Airflow 3.2)
from airflow.traces import otel_tracer
from airflow.traces.otel_tracer import get_otel_tracer
from airflow.traces import NO_TRACE_ID
from airflow.traces.tracer import Trace
from airflow.traces.tracer import add_debug_span
from airflow.traces.utils import gen_span_id_from_ti_key

otel_tracer
get_otel_tracer
NO_TRACE_ID
Trace
add_debug_span
gen_span_id_from_ti_key

# These should not trigger
from airflow.models import DAG
from airflow.operators.python import PythonOperator

DAG
PythonOperator
