from pathlib import Path

from .gangstarr import *
from .context_manager import full_clip
from .reporting import (
    LoggingOptions,
    PrintingOptions,
    MassAppealException,
    RaisingOptions,
    ReportingOptions,
)


def default_base_dir(file) -> str:
    return str(Path(file).resolve().parent.parent)


__all__ = [
    "full_clip",
    "LoggingOptions",
    "PrintingOptions",
    "RaisingOptions",
    "ReportingOptions",
    "MassAppealException",
    "default_base_dir",
]
