# Gangstarr Architecture: Python + Rust (maturin)

## Overview

Gangstarr is a Django query tracing and analysis tool inspired by `django-queryhunter`.

The project uses a **hybrid Python + Rust architecture** built with **maturin**.

The guiding principle:

- Python handles **Django runtime integration and query capture**
- Rust handles **analysis, grouping, detection, and future optimization logic**

This keeps the system:

- easy to integrate into Django
- extremely fast for analysis
- extensible for advanced Postgres insights later

---

# System Architecture

## Responsibilities

### Python Layer

Python is responsible for runtime integration:

- Django middleware
- database execution hooks
- context manager capture
- request lifecycle tracking
- stack inspection
- file and line attribution
- source code lookup
- query event collection
- developer-facing reporting

Python acts as the **data collector and presentation layer**.

---

### Rust Layer

Rust is responsible for analysis:

- SQL normalization
- SQL fingerprinting
- grouping duplicate queries
- N+1 detection
- hotspot detection
- scoring results
- generating structured findings
- future Postgres plan analysis

Rust acts as the **analysis engine**.

---

# Runtime Flow

```

Django request
│
▼
Gangstarr middleware
│
Intercept SQL execution
│
Capture query metadata
│
Create QueryEvent objects
│
Send events to Rust engine
│
Rust performs analysis
│
Return structured findings
│
Python reporting layer
│
Console / logs / JSON / tests

```

---

# Query Event Model

The Python layer produces a stable event structure sent to Rust.

Example:

```

{
"sql": "SELECT * FROM song WHERE id = 1",
"duration_ms": 1.42,
"file": "app/views.py",
"line": 42,
"function": "song_list",
"source": "songs = Song.objects.all()",
"label": "request",
"request_id": "abc123",
"db_alias": "default"
}

```

Rust receives a list of these events for analysis.

---

# Project Layout

The project structure should evolve toward the following layout:

```

gangstarr/
├── Cargo.toml
├── pyproject.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── errors.rs
│   ├── models.rs
│   ├── normalize.rs
│   ├── fingerprint.rs
│   ├── group.rs
│   ├── detect.rs
│   ├── score.rs
│   ├── findings.rs
│   ├── explain.rs
│   └── serde_utils.rs
├── python/
│   └── gangstarr/
│       ├── **init**.py
│       ├── middleware.py
│       ├── context_manager.py
│       ├── reporting.py
│       ├── collector.py
│       ├── engine.py
│       ├── schemas.py
│       └── testapp/
└── tests/

```

Python remains inside the `python/gangstarr` package.

Rust lives in the `src/` directory.

---

# Rust Modules

## lib.rs

Purpose: PyO3 entrypoint.

Responsibilities:

- expose the Python extension module
- convert Python dictionaries into Rust structs
- convert Rust results back into Python objects

Public functions exposed to Python:

```

fingerprint_sql(sql)
normalize_sql(sql)
analyze_events(events)
summarize_events(events)

```

`lib.rs` should remain thin and contain no analysis logic.

---

## errors.rs

Centralized error definitions.

Responsibilities:

- define Rust error types
- map Rust errors to Python exceptions
- prevent panics escaping into Python

Example error types:

```

GangstarrError
ParseError
ValidationError
AnalysisError

```

---

## models.rs

Defines internal Rust data models.

### QueryEvent

Represents a captured query execution.

Fields:

```

sql
duration_ms
file
line
function
source
label
request_id
db_alias

```

---

### NormalizedQuery

Represents a normalized query.

```

raw_sql
normalized_sql
fingerprint

```

---

### GroupedQuery

Represents a group of similar queries.

```

fingerprint
count
total_duration_ms
avg_duration_ms
sample_sql
callsites

```

---

### CallsiteGroup

Represents grouped queries by source location.

```

file
line
function
source
count

```

---

### Finding

Represents an analysis result.

```

code
title
severity
message
fingerprint
file
line
evidence
suggestion

```

---

# SQL Normalization

File: `normalize.rs`

Purpose:

Convert SQL queries into a consistent comparable representation.

Example:

```

SELECT * FROM song WHERE id = 1
SELECT * FROM song WHERE id = 2

```

Normalized:

```

SELECT * FROM song WHERE id = ?

```

This enables accurate grouping.

Responsibilities:

- normalize whitespace
- standardize formatting
- replace literal values
- produce stable SQL shape

---

# Query Fingerprinting

File: `fingerprint.rs`

Purpose:

Generate deterministic identifiers for query shapes.

Example pipeline:

```

raw SQL
↓
normalize
↓
hash(normalized_sql)
↓
fingerprint

```

Fingerprints drive duplicate detection.

---

# Query Grouping

File: `group.rs`

Groups queries for analysis.

Supported grouping:

- by fingerprint
- by callsite
- by fingerprint + callsite

Responsibilities:

- count duplicates
- compute duration statistics
- preserve example SQL
- preserve callsite evidence

---

# Pattern Detection

File: `detect.rs`

Detects suspicious patterns.

Initial detectors:

### Duplicate Query Detector

Same fingerprint repeated many times.

### Likely N+1 Detector

Same query shape repeated from the same callsite.

### Hot Callsite Detector

Single source line issuing many queries.

### Expensive Duplicate Detector

Repeated queries with high cumulative duration.

Detectors produce **Finding objects**.

---

# Scoring

File: `score.rs`

Ranks findings based on impact.

Inputs:

- query count
- cumulative duration
- average duration
- callsite repetition

Outputs:

```

info
warning
error

```

Scoring prevents overwhelming users with noise.

---

# Findings Registry

File: `findings.rs`

Central registry of detection codes.

Examples:

```

G001 duplicate queries
G002 likely N+1 pattern
G003 hotspot callsite
G004 expensive repeated queries

```

Future codes:

```

G101 missing index suspicion
G201 bad query plan

```

Each finding includes:

- code
- title
- severity
- evidence
- suggested fix

---

# EXPLAIN Analysis

File: `explain.rs`

Future module for Postgres query plan analysis.

Responsibilities:

- parse `EXPLAIN (FORMAT JSON)`
- detect sequential scans
- detect nested loop explosions
- detect sort spills
- detect missing indexes

This module is not required for initial parity with `django-queryhunter`.

---

# Serialization Utilities

File: `serde_utils.rs`

Handles conversion between:

- Python dictionaries
- Rust structs

Responsibilities:

- deserialize query events
- validate fields
- manage optional values
- simplify PyO3 integration

---

# Python Modules

## collector.py

Captures query events.

Responsibilities:

- intercept database execution
- capture metadata
- store events during request lifecycle

---

## schemas.py

Defines Python-side data structures.

Examples:

```

QueryEvent
Finding
AnalysisResult

```

Use:

```

dataclasses
TypedDict

```

---

## engine.py

Wrapper around Rust extension.

Responsibilities:

- call Rust functions
- hide binding details
- provide Python-friendly interface

Example:

```

results = engine.analyze(events)

```

---

# Rust-Python Data Contract

Python sends:

```

list[QueryEvent]

```

Rust returns:

```

{
"summary": {...},
"findings": [...]
}

```

Example output:

```

{
"summary": {
"total_queries": 42,
"unique_queries": 8,
"duplicate_groups": 3,
"n_plus_one": 1
},
"findings": [
{
"code": "G002",
"title": "Likely N+1 query pattern",
"severity": "warning",
"file": "app/views.py",
"line": 42,
"message": "Query executed 15 times from the same callsite",
"suggestion": "Consider select_related or prefetch_related"
}
]
}

```

---

# Development Phases

## Phase 1

Introduce maturin.

Goals:

- Python package works
- Rust extension builds
- simple Rust function exposed

---

## Phase 2

Move analysis into Rust.

Rust handles:

- normalization
- fingerprinting
- grouping
- detection

Python still handles reporting.

---

## Phase 3

Improve developer experience.

Add:

- severity ranking
- configurable thresholds
- pytest integration
- JSON reports

---

## Phase 4

Postgres-aware analysis.

Add:

- EXPLAIN plan parsing
- index suggestions
- join analysis
- scan warnings

---

## Phase 5

AI-assisted optimization.

Use findings to generate:

- ORM improvements
- index migrations
- automated regression tests

---

# Design Principles

1. Python owns Django integration.
2. Rust owns analysis logic.
3. PyO3 bindings remain thin.
4. Findings are structured objects.
5. Design for Postgres-aware evolution.

---

# Final Concept

Gangstarr should be thought of as:

**A Django query tracing system with a Rust analysis engine.**

Python provides:

```

runtime integration

```

Rust provides:

```

fast query intelligence

```

This architecture allows Gangstarr to grow from a simple query tracer into a full **local performance auditing system for Django + Postgres applications.**
```
