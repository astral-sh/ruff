from pyspark.sql import SparkSession, functions as F

spark = (
    SparkSession.builder
    .appName("ruff_test")
    .getOrCreate())

df = spark.read.table("test_table")
df2 = spark.read.table("test_table2")

# errors

for col in df.columns:
    df = df.withColumn(col, F.col(col).cast("string"))

i = 0
while i < 5:
    df = df.withColumn(f"col_{i}", F.lit(i))
    i += 1

# OK

df = (
    df.withColumn("c", F.lit(3))
    .withColumn("d", F.lit(4))
)

df = df.join(df2, on='id')
