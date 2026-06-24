from __future__ import annotations

from airflow.hooks.slack_hook import SlackHook
from airflow.operators.slack_operator import SlackAPIOperator, SlackAPIPostOperator

SlackHook()
SlackAPIOperator()
SlackAPIPostOperator()
