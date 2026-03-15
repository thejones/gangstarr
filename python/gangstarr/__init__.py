from pathlib import Path

from .context_manager import full_clip
from .gangstarr import *  # noqa: F403
from .reporting import (
    JsonOptions,
    LoggingOptions,
    MassAppealException,
    PrintingOptions,
    RaisingOptions,
    ReportingOptions,
)


def default_base_dir(file) -> str:
    return str(Path(file).resolve().parent.parent)


__all__ = [
    "full_clip",
    "JsonOptions",
    "LoggingOptions",
    "PrintingOptions",
    "RaisingOptions",
    "ReportingOptions",
    "MassAppealException",
    "default_base_dir",
]
