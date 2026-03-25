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
    consolidated = analysis.get('consolidated', [])

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
    has_gql = request_context and (
        getattr(request_context, 'operation_name', '')
        or getattr(request_context, 'operation_type', '')
    )
    if has_gql:
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

    # Consolidated findings by callsite
    if consolidated:
        lines.append(f"{BOLD}Consolidated findings by callsite{RESET}")
        lines.append(SINGLE_LINE)
        lines.append("")
        loc_col_width = 60
        col_hdr = (
            f"| {'File:Line':<{loc_col_width}} | {'Total Q':>7} "
            f"| {'Dup Groups':>10} | {'Worst Rep':>9} "
            f"| {'Dup Time':>8} | {'Flags':<10} |"
        )
        lines.append(col_hdr)
        sep = f"|{'-'*(loc_col_width+2)}|{'-'*9}|{'-'*12}|{'-'*11}|{'-'*10}|{'-'*12}|"
        lines.append(sep)
        for c in consolidated:
            loc = f"{c['file']}:{c['line']}"
            # Append caller chain if available
            chain = c.get('caller_chain', [])
            if chain:
                caller = chain[0]
                cf = caller['file']
                caller_file = cf.rsplit('/', 1)[-1] if '/' in cf else cf
                loc += f" \u2192 {caller_file}:{caller['line']}"
            if len(loc) > loc_col_width:
                loc = "\u2026" + loc[-(loc_col_width - 1):]
            worst = f"{c['worst_repeat']}x" if c['worst_repeat'] > 0 else "-"
            dup_time = f"{c['dup_duration_ms']:.1f}ms" if c['dup_duration_ms'] > 0 else "-"
            flags = ', '.join(c['flags']) if c['flags'] else ''
            fl = c.get('flags', [])
            color = RED if 'HOT' in fl else (YELLOW if 'N+1' in fl else GREEN)
            row = (
                f"| {loc:<{loc_col_width}} | {c['total_queries']:>7} "
                f"| {c['dup_groups']:>10} | {worst:>9} "
                f"| {dup_time:>8} | {flags:<10} |"
            )
            lines.append(f"{color}{row}{RESET}")
        lines.append("")

    # Top repeated query groups
    repeated = [g for g in groups if g['count'] > 1]
    if repeated:
        lines.append(f"{BOLD}Top repeated query groups{RESET}")
        lines.append(SINGLE_LINE)
        for g in repeated[:5]:
            top_cs = g['callsites'][0] if g['callsites'] else None
            loc = f"{top_cs['file']}:{top_cs['line']}" if top_cs else "unknown"
            avg = g.get('avg_duration_ms', 0)
            p50 = g.get('p50_duration_ms', 0)
            mx = g.get('max_duration_ms', 0)
            timing = f"{g['total_duration_ms']:.1f}ms total | avg {avg:.1f}ms | p50 {p50:.1f}ms | max {mx:.1f}ms"
            lines.append(f"\n{YELLOW}[{g['count']}x | {timing}]{RESET} {loc}")
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
