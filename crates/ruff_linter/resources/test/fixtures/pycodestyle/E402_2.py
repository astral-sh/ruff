import os

os.environ["WORLD_SIZE"] = "1"
os.putenv("CUDA_VISIBLE_DEVICES", "4")
del os.environ["WORLD_SIZE"]

import torch
