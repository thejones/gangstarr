from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from gangstarr.premier import Premier

SORT_BY_OPTIONS = ['line_no', '-line_no', 'count', '-count', 'duration', '-duration']


@dataclass
class ReportingOptions:
    sort_by: str = 'line_no'
    modules: list[str] | None = None
    max_sql_length: int | None = None
    count_threshold: int = 1
    duration_threshold: float = 0.0

    def __post_init__(self):
        if self.sort_by not in SORT_BY_OPTIONS:
            raise ValueError(f'sort_by must be one of {SORT_BY_OPTIONS}')


@dataclass
class PrintingOptions(ReportingOptions):
    count_highlighting_threshold: int = 5
    duration_highlighting_threshold: float = 0.5


@dataclass
class LoggingOptions(ReportingOptions):
    logger_name: str = 'gangstarr'


@dataclass
class RaisingOptions(ReportingOptions):
    count_threshold: int = 5
    duration_threshold: float = 0.5


@dataclass
class JsonOptions(ReportingOptions):
    """Options for JSON file output."""
    output_dir: str = '.gangstarr'


class MassAppealException(Exception):
    pass


class Guru:
    def __init__(self, premier: Premier):
        self.premier = premier
        self.query_info = premier.query_info
        self.options = premier.reporting_options

    @classmethod
    def create(cls, premier: Premier) -> PrintingGuru | LoggingGuru | RaisingGuru | JsonGuru:
        reporting_options = premier.reporting_options
        if isinstance(reporting_options, PrintingOptions):
            return PrintingGuru(premier)
        elif isinstance(reporting_options, LoggingOptions):
            return LoggingGuru(premier)
        elif isinstance(reporting_options, RaisingOptions):
            return RaisingGuru(premier)
        elif isinstance(reporting_options, JsonOptions):
            return JsonGuru(premier)
        # Default to printing
        return PrintingGuru(premier)

    def _run_analysis(self) -> dict[str, Any] | None:
        """Run the Rust analysis engine on collected events."""
        if not self.premier.events:
            return None
        from gangstarr.engine import analyze
        return analyze(self.premier.events)


def _extract_root_fields_from_groups(groups: list[dict]) -> list[str]:
    """Derive root GraphQL field names from Query.* resolver paths in analysis groups."""
    seen = set()
    fields = []
    for g in groups:
        for cs in g.get('callsites', []):
            rp = cs.get('resolver_path', '')
            if rp.startswith('Query.') or rp.startswith('Mutation.') or rp.startswith('Subscription.'):
                field = rp.split('.', 1)[1]
                if field not in seen:
                    seen.add(field)
                    fields.append(field)
    return fields


def _collect_file_locations(groups: list[dict]) -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    """Collect query entry points and resolver file locations from analysis groups.

    Returns (query_locations, resolver_locations) where each is a list of
    (label, "file:line") tuples, deduplicated and ordered by first appearance.
    """
    query_locs: list[tuple[str, str]] = []
    resolver_locs: list[tuple[str, str]] = []
    seen = set()

    for g in groups:
        for cs in g.get('callsites', []):
            rp = cs.get('resolver_path', '')
            loc = f"{cs['file']}:{cs['line']}"
            if not rp or loc in seen:
                continue
            seen.add(loc)
            if rp.startswith(('Query.', 'Mutation.', 'Subscription.')):
                op_type = rp.split('.', 1)[0].lower()
                field = rp.split('.', 1)[1]
                query_locs.append((f"{op_type} {field}", loc))
            else:
                resolver_locs.append((f"resolver {rp}", loc))

    return query_locs, resolver_locs


def _format_report(analysis: dict[str, Any], request_context=None) -> str:
    """Format an analysis result into the structured console report."""
    summary = analysis['summary']
    groups = analysis['groups']
    findings = analysis['findings']

    BOLD = "\033[1m"
    RED = "\033[31m"
    YELLOW = "\033[33m"
    GREEN = "\033[32m"
    DIM = "\033[2m"
    RESET = "\033[0m"
    DOUBLE_LINE = "\u2550" * 78
    SINGLE_LINE = "\u2500" * 78

    lines = []

    # Header
    lines.append(f"{BOLD}{DOUBLE_LINE}{RESET}")
    if request_context and request_context.path:
        view = request_context.view_name or ''
        header = f"QUERY REPORT  {request_context.method} {request_context.path}"
        if view:
            header += f"  \u2192  {view}"
        lines.append(f"{BOLD}{header}{RESET}")

        op_name = getattr(request_context, 'operation_name', '')
        op_type = getattr(request_context, 'operation_type', '')
        if op_name or op_type:
            gql_line = "GRAPHQL OPERATION   \u2192  "
            gql_line += f"{op_type} {op_name}".strip()
            # Derive root fields from Query.* resolver paths in the analysis
            root_fields = _extract_root_fields_from_groups(groups)
            if root_fields:
                gql_line += f" \u2192 {', '.join(root_fields)}"
            lines.append(f"{BOLD}{gql_line}{RESET}")

    total_dur = summary['total_duration_ms'] / 1000
    lines.append(
        f"TOTAL         {summary['total_queries']} queries in {total_dur:.4f}s"
    )
    lines.append(f"{BOLD}{DOUBLE_LINE}{RESET}")

    # Summary table
    reads = summary['reads']
    writes = summary['writes']
    total = summary['total_queries']
    dupes = sum(g['count'] - 1 for g in groups if g['count'] > 1)
    hdr = f"| {'Scope':<7} | {'Database':<8} | {'Reads':>5} | {'Writes':>6} | {'Total':>5} | {'Dupes':>5} |"
    lines.append(hdr)
    lines.append(f"|{'-'*9}|{'-'*10}|{'-'*7}|{'-'*8}|{'-'*7}|{'-'*7}|")
    lines.append(f"| {'RESP':<7} | {'default':<8} | {reads:>5} | {writes:>6} | {total:>5} | {dupes:>5} |")
    lines.append("")

    # Files section (GraphQL only — show query entry points and resolver locations)
    if request_context and (getattr(request_context, 'operation_name', '') or getattr(request_context, 'operation_type', '')):
        query_locations, resolver_locations = _collect_file_locations(groups)
        if query_locations or resolver_locations:
            lines.append(f"{BOLD}Files{RESET}")
            lines.append(SINGLE_LINE)
            for label, loc in query_locations:
                lines.append(f"{label}")
                lines.append(f"  {loc}")
            for label, loc in resolver_locations:
                lines.append(f"{label}")
                lines.append(f"  {loc}")
            lines.append("")

    # Findings
    if findings:
        lines.append(f"{BOLD}Findings{RESET}")
        lines.append(SINGLE_LINE)
        for f in findings:
            sev = f['severity']
            if sev == 'error':
                color = RED
            elif sev == 'warning':
                color = YELLOW
            else:
                color = GREEN
            loc = ""
            if f.get('file'):
                loc = f"{f['file']}:{f.get('line', '')}"
            # Show resolver_path when available (e.g. "ArtistType.albums → schema.py:10")
            resolver = f.get('resolver_path', '')
            if resolver:
                lines.append(f"{color}[{f['code']}] {f['title']}{RESET}  {resolver} \u2192 {loc}")
            else:
                lines.append(f"{color}[{f['code']}] {f['title']}{RESET}  {loc}")
            lines.append(f"  {DIM}{f['message']}{RESET}")
            if f.get('suggestion'):
                lines.append(f"  {GREEN}\u2192 {f['suggestion']}{RESET}")
        lines.append("")

    # Most repeated SQL
    repeated = [g for g in groups if g['count'] > 1]
    if repeated:
        lines.append(f"{BOLD}Most repeated SQL{RESET}")
        lines.append(SINGLE_LINE)
        for g in repeated[:10]:  # top 10
            top_cs = g['callsites'][0] if g['callsites'] else None
            loc = f"{top_cs['file']}:{top_cs['line']}" if top_cs else "unknown"
            lines.append(f"\n{YELLOW}[{g['count']}x]{RESET} {loc}")
            lines.append(f"{DIM}{g['normalized_sql'][:200]}{RESET}")
        lines.append("")

    return "\n".join(lines)


class PrintingGuru(Guru):
    def report(self):
        analysis = self._run_analysis()
        if analysis:
            print(_format_report(analysis, self.premier.request_context))
        else:
            # Fallback to legacy output if no events collected
            self._legacy_report()

    def _legacy_report(self):
        RED = "\033[31m"
        GREEN = "\033[32m"
        BOLD = "\033[1m"
        for name, module in self.query_info.items():
            print(f'{BOLD}{name}')
            print('=' * 2 * len(name))
            for line in module.lines:
                if line.duration < self.options.duration_threshold or line.count < self.options.count_threshold:
                    continue
                if line.duration >= self.options.duration_highlighting_threshold:
                    print(f'   {RED}{line}')
                elif line.count >= self.options.count_highlighting_threshold:
                    print(f'   {RED}{line}')
                else:
                    print(f'   {GREEN}{line}')
            print('\n')


class LoggingGuru(Guru):
    def report(self):
        analysis = self._run_analysis()
        logger = logging.getLogger(self.options.logger_name)
        if analysis:
            for finding in analysis.get('findings', []):
                loc = f"{finding.get('file', '')}:{finding.get('line', '')}"
                logger.info(f"[{finding['code']}] {finding['title']} | {loc} | {finding['message']}")
        else:
            # Fallback to legacy
            for _name, module in self.query_info.items():
                for line in module.lines:
                    if line.duration < self.options.duration_threshold or line.count < self.options.count_threshold:
                        continue
                    logger.info(f'Module: {module.name} | {line}')


class RaisingGuru(Guru):
    def report(self):
        analysis = self._run_analysis()
        if analysis:
            for finding in analysis.get('findings', []):
                if finding['severity'] in ('error', 'warning'):
                    loc = f"{finding.get('file', '')}:{finding.get('line', '')}"
                    raise MassAppealException(
                        f"[{finding['code']}] {finding['title']} at {loc}: {finding['message']}"
                    )
        # Fallback to legacy
        for name, module in self.query_info.items():
            for line in module.lines:
                if line.duration < self.options.duration_threshold or line.count < self.options.count_threshold:
                    continue
                if line.duration >= self.options.duration_threshold:
                    raise MassAppealException(f'Excessive time spent in module: {name} | {line}')
                elif line.count >= self.options.count_threshold:
                    raise MassAppealException(f'Excessive repeated queries in module: {name} | {line}')


class JsonGuru(Guru):
    """Writes NDJSON analysis logs to .gangstarr/logs/."""

    def report(self):
        analysis = self._run_analysis()
        if not analysis:
            return

        output_dir = getattr(self.options, 'output_dir', '.gangstarr')
        logs_dir = Path(output_dir) / 'logs'
        logs_dir.mkdir(parents=True, exist_ok=True)

        timestamp = datetime.now(UTC).strftime('%Y%m%d_%H%M%S')
        log_file = logs_dir / f'query_report_{timestamp}.ndjson'

        ctx = self.premier.request_context
        record = {
            'timestamp': datetime.now(UTC).isoformat(),
            'request': {
                'method': ctx.method,
                'path': ctx.path,
                'view_name': ctx.view_name,
                'status_code': ctx.status_code,
                'request_id': ctx.request_id,
            },
            'summary': analysis['summary'],
            'findings': analysis['findings'],
            'groups': analysis['groups'],
        }

        with open(log_file, 'a') as f:
            f.write(json.dumps(record) + '\n')
