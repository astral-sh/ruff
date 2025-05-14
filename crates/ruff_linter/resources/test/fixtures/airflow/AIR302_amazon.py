from __future__ import annotations

from airflow.hooks.S3_hook import (
    S3Hook,
    provide_bucket_name,
)
from airflow.operators.gcs_to_s3 import GCSToS3Operator
from airflow.operators.google_api_to_s3_transfer import (
    GoogleApiToS3Operator,
    GoogleApiToS3Transfer,
)
from airflow.operators.redshift_to_s3_operator import (
    RedshiftToS3Operator,
    RedshiftToS3Transfer,
)
from airflow.operators.s3_file_transform_operator import S3FileTransformOperator
from airflow.operators.s3_to_redshift_operator import (
    S3ToRedshiftOperator,
    S3ToRedshiftTransfer,
)
from airflow.sensors.s3_key_sensor import S3KeySensor

S3Hook()
provide_bucket_name()

GCSToS3Operator()

GoogleApiToS3Operator()
GoogleApiToS3Transfer()

RedshiftToS3Operator()
RedshiftToS3Transfer()

S3FileTransformOperator()

S3ToRedshiftOperator()
S3ToRedshiftTransfer()

S3KeySensor()
