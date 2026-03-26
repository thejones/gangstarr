# Gangstarr — Product Overview

## Purpose

Gangstarr is a Django SQL query profiling and performance analysis library with a Rust/PyO3 native extension. It instruments Django's database layer to track which lines of application code trigger SQL queries, reporting on query count, duration, duplicates, and N+1 patterns. The long-term vision is a full performance copilot for Django + PostgreSQL applications combining static analysis, runtime SQL inspection, and AI-ready remediation artifacts.

## Value Proposition

- **Drop-in adoption**: No migrations, no wrappers — add middleware or use the `full_clip` context manager and get immediate query profiling
- **Rust-powered analysis**: SQL normalization, fingerprinting, grouping, and pattern detection run in a native Rust extension via PyO3 for speed
- **GraphQL-aware**: Includes Graphene middleware (`DWYCKMiddleware`) and a resolver index that attributes N+1 queries to the actual GraphQL resolver, not generic middleware code
- **Multiple reporting modes**: Console printing, logging, exception raising, and NDJSON file output — all configurable via dataclass options
- **AI-ready output**: Structured JSON reports designed to be handed to AI coding tools for automated remediation

## Naming Convention

All public API names reference Gang Starr (the hip-hop duo) songs, albums, and members:
- `full_clip` — context manager (song: "Full Clip")
- `Premier` — query interceptor (DJ Premier)
- `Guru` — reporter base class (rapper Guru)
- `MomentOfTruthMiddleware` — Django middleware (album: "Moment of Truth")
- `MassAppealException` — threshold exception (song: "Mass Appeal")
- `DWYCKMiddleware` — Graphene middleware (song: "DWYCK")

New features should follow this naming convention.

## Key Features

1. **Context Manager (`full_clip`)**: Wraps any code block to capture and report SQL queries with source attribution
2. **Django Middleware (`MomentOfTruthMiddleware`)**: Automatic request-level profiling when `DEBUG=True`
3. **Rust Analysis Engine**: Normalizes SQL via pg_query, fingerprints query shapes, groups by fingerprint, detects patterns (G001 duplicates, G002 N+1, G003 hot callsites)
4. **GraphQL Resolver Attribution**: `DWYCKMiddleware` + `ResolverIndex` maps SQL queries to specific GraphQL resolvers via static analysis of schema files
5. **Configurable Reporters**: `PrintingGuru`, `LoggingGuru`, `RaisingGuru`, `JsonGuru` — each with its own options dataclass
6. **Structured Console Reports**: Color-coded output with summary tables, findings with severity, and repeated SQL sections

## Target Users

- Django developers working with PostgreSQL who want to identify and fix ORM performance issues
- Teams building GraphQL APIs with Graphene/graphene-django who need resolver-level query attribution
- Developers who want AI-ready performance diagnostics they can hand to coding assistants

## Current State

The project is in active development (v0.1.0). The runtime profiling, Rust analysis engine, and GraphQL attribution are functional. The roadmap includes static analysis (AST-based ORM anti-pattern detection), PostgreSQL introspection (index recommendations, EXPLAIN analysis), and `.gangstarr` issue folder generation for AI-native workflows.
