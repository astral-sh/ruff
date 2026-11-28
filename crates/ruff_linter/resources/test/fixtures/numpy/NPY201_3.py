def func():
    import numpy as np

    np.DTypePromotionError

    np.ModuleDeprecationWarning

    np.RankWarning

    np.TooHardError

    np.VisibleDeprecationWarning

    np.chararray

    np.format_parser


# https://github.com/astral-sh/ruff/issues/30770
# Top-level import produces safe fixes.
import numpy as top_level_np

top_level_np.DTypePromotionError

