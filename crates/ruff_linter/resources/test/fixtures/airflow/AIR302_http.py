from __future__ import annotations

from airflow.hooks.http_hook import HttpHook
from airflow.operators.http_operator import SimpleHttpOperator
from airflow.sensors.http_sensor import HttpSensor

HttpHook()
SimpleHttpOperator()
HttpSensor()
