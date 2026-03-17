from __future__ import annotations

import linecache
import os
import time
import traceback
from dataclasses import dataclass
from typing import TYPE_CHECKING

from django.conf import settings

if TYPE_CHECKING:
    from gangstarr import ReportingOptions


# --- Legacy data structures (kept for backward compat) ---

@dataclass
class Line:
    line_no: int
    code: str
    sql: str
    count: int
    duration: float
    meta_data: dict[str, str] | None = None

    def __str__(self):
        string = (
            f'Line no: {self.line_no} | Code: {self.code} | '
            f'Num. Queries: {self.count} | SQL: {self.sql} | Duration: {self.duration}'
        )
        if self.meta_data:
            for key, value in self.meta_data.items():
                string += f' | {key}: {value}'
        return string


@dataclass
class Module:
    name: str
    lines: list[Line]

    def __str__(self):
        data = ''
        for line_data in self.lines:
            data += f'Module: {self.name} | {line_data} \n'
        data.rstrip('\n')
        return data


# --- Premier: the Django execute_wrapper hook ---

class Premier:
    """DJ Premier intercepts SQL execution and captures query events.

    Operates in two modes:
    - New mode: collects QueryEvent objects for Rust analysis
    - Legacy mode: builds Module/Line dicts (backward compat for old reporters)
    """

    def __init__(self, reporting_options: ReportingOptions, meta_data: dict[str, str] = None):
        from gangstarr.schemas import QueryEvent, RequestContext

        self.reporting_options = reporting_options
        self.meta_data = meta_data

        # New: structured event collection
        self.events: list[QueryEvent] = []
        self.request_context = RequestContext(
            method=meta_data.get('method', '') if meta_data else '',
            path=meta_data.get('url', '') if meta_data else '',
            operation_name=meta_data.get('operation_name', '') if meta_data else '',
            operation_type=meta_data.get('operation_type', '') if meta_data else '',
        )

        # Legacy: kept for backward compat
        self.query_info: dict[str, Module] = {}

    def __call__(self, execute, sql, params, many, context):
        stack_trace = traceback.extract_stack()[:-1]

        app_frame = None
        caller_frames = []
        for frame in reversed(stack_trace):
            filename = frame.filename
            if self.is_application_code(filename):
                if app_frame is None:
                    app_frame = frame
                elif len(caller_frames) < 2:
                    relative = str(os.path.relpath(frame.filename, settings.GANGSTAR_BASE_DIR))
                    caller_frames.append({
                        'file': relative,
                        'line': frame.lineno,
                        'function': frame.name,
                    })

        if app_frame:
            filename = app_frame.filename
            relative_path = str(os.path.relpath(app_frame.filename, settings.GANGSTAR_BASE_DIR))

            if self.reporting_options.modules is not None:
                if relative_path not in self.reporting_options.modules:
                    return execute(sql, params, many, context)

            line_no = app_frame.lineno
            code = self.get_code_from_line(filename, line_no)
            start = time.monotonic()
            result = execute(sql, params, many, context)
            duration = time.monotonic() - start

            # Read resolver context from Graphene middleware (if active)
            resolver_path = ''
            try:
                from gangstarr.graphene import get_resolver_path
                resolver_path = get_resolver_path()
            except ImportError:
                pass

            # If we have a resolver path, try to resolve it to a real source
            # location via the static index (instead of middleware.py:67).
            event_file = relative_path
            event_line = line_no
            event_function = app_frame.name
            event_source = code
            if resolver_path:
                try:
                    from gangstarr.resolver_index import get_index
                    loc = get_index().lookup(resolver_path)
                    if loc:
                        event_file = loc.file
                        event_line = loc.line
                        event_function = resolver_path
                        event_source = loc.source
                except Exception:
                    pass  # Fail open — fall back to stack frame attribution

            # New: collect structured event
            from gangstarr.schemas import QueryEvent

            self.events.append(
                QueryEvent(
                    sql=sql,
                    duration_ms=duration * 1000,
                    file=event_file,
                    line=event_line,
                    function=event_function,
                    source=event_source,
                    label=self.request_context.method or None,
                    request_id=self.request_context.request_id,
                    db_alias="default",
                    resolver_path=resolver_path,
                    caller_chain=caller_frames,
                )
            )

            # Legacy: build Module/Line structures
            if (max_length := self.reporting_options.max_sql_length) is not None:
                reportable_sql = sql[:max_length]
            else:
                reportable_sql = sql

            module = self.query_info.get(relative_path, Module(relative_path, lines=[]))
            try:
                line = next(line for line in module.lines if line.line_no == line_no)
            except StopIteration:
                line = Line(
                    line_no=line_no,
                    code=code,
                    sql=reportable_sql,
                    count=1,
                    duration=duration,
                    meta_data=self.meta_data,
                )
                module.lines.append(line)
            else:
                line.count += 1
                line.duration += duration

            reverse = self.reporting_options.sort_by.startswith('-')
            sort_by = self.reporting_options.sort_by[1:] if reverse else self.reporting_options.sort_by
            module.lines = sorted(module.lines, key=lambda x: getattr(x, sort_by), reverse=reverse)

            self.query_info[relative_path] = module
            return result
        else:
            raise ValueError("Unable to determine application frame for SQL execution")

    @staticmethod
    def is_application_code(filename: str) -> bool:
        try:
            base_dir = settings.GANGSTAR_BASE_DIR
        except AttributeError:
            raise ValueError(
                "GANGSTAR_BASE_DIR not set in settings. "
                "Define manually or use the built in gangstarr.default_base_dir function",
            )
        return filename.startswith(base_dir)

    @staticmethod
    def get_code_from_line(filename: str, lineno: int) -> str:
        return linecache.getline(filename, lineno).strip()
