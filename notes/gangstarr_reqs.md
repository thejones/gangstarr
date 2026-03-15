# Gangstarr

## AI Plan for Django and PostgreSQL Performance Assistance

> **Mission:** Make Django applications not just feel fast, but actually be fast, by combining static analysis, PostgreSQL introspection, runtime query reporting, and AI-ready remediation artifacts with a dead-simple developer experience.

---

## 1. Executive Summary

Gangstarr is a Rust and Python project focused on helping Django developers build performant applications on PostgreSQL.

The core problem is not that Django itself is too slow. Django is generally fast enough and provides a strong developer experience. The real issue is that ORM-driven applications make it easy to write inefficient query patterns, over-fetch data, push too much work into Python, and miss database-level optimizations until performance problems show up later.

Gangstarr aims to solve that by letting a developer run a Django server locally and immediately receive practical, high-quality performance assistance. That includes:

* static analysis
* PostgreSQL-specific optimization suggestions
* structured runtime query reporting
* AI-ready remediation artifacts
* a simple, drop-in developer experience

---

## 2. Problem Statement

Frameworks like Django and Ruby on Rails make it easy to ship software quickly, but they also make it easy to write slow code.

The ORM is a major source of this problem. It helps developers move faster, but it can hide expensive SQL behavior, repeated queries, poor data access patterns, and application logic that should likely live closer to the database.

As a result, many teams end up with code that looks clean at the Python level while generating inefficient database behavior underneath.

---

## 3. Product Vision

Gangstarr should act like a drop-in performance copilot for Django + PostgreSQL applications.

A developer should be able to:

1. add Gangstarr to a Django project with minimal setup
2. run the local development server
3. hit endpoints normally
4. receive highly actionable feedback about performance issues
5. open AI-ready artifacts for diagnosis and fixing

The experience should be opinionated, simple, and useful out of the box.

### Key experience goals

* no migrations
* no invasive wrappers
* minimal configuration
* sensible defaults
* PostgreSQL-first optimization
* output that works equally well for humans and AI tools

---

## 4. Core Objectives

| Objective               | What success looks like                                                                                         |
| ----------------------- | --------------------------------------------------------------------------------------------------------------- |
| Static analysis         | Find common ORM and Python-side performance anti-patterns before they become production issues.                 |
| PostgreSQL optimization | Use database introspection and query statistics to recommend better indexes, query shapes, and execution plans. |
| Developer experience    | Require no migrations, no wrappers around app code, and minimal config with good defaults.                      |
| AI-native workflow      | Produce structured, deduplicated artifacts that can be handed directly to AI coding tools.                      |
| Runtime reporting       | Show concise query reports with source locations, repeated SQL summaries, and direct links to fix artifacts.    |

---

## 5. Primary User Promise

Gangstarr should let a Django developer do the following:

1. Run the Django development server locally.
2. Make requests to the application as usual.
3. See clean, structured query reports with clickable source paths.
4. Open a generated `.gangstarr` issue folder containing code context, issue classification, SQL evidence, and remediation guidance.
5. Fix the issue manually or pass the artifact into the AI tool of their choice.

---

## 6. Main Output: Structured Debug Logs and AI Issue Folders

The central experience is the combination of:

* human-readable runtime logs
* machine-friendly remediation folders

The logs should identify:

* request and handler
* source file and line
* response status
* total query count and total query time
* duplicate query behavior
* repeated SQL patterns
* a link to the generated AI fix folder

Each issue should map to a deduplicated folder inside `.gangstarr`, so hitting the same endpoint 100 times does not generate 100 copies of the same fix artifact.

### Example log shape

Inline examples of the intended output:

```text
SOURCE        testapp/views.py:14
[FIX: testapp/.gangstarr/<folder_path>]
```

### Example query report

```text
══════════════════════════════════════════════════════════════════════════════
QUERY REPORT  GET /  →  index
SOURCE        testapp/views.py:14
STATUS        200
TOTAL         1 queries in 0.0010s
══════════════════════════════════════════════════════════════════════════════

| Scope | Database | Reads | Writes | Total | Duplicates |
|-------|----------|------:|-------:|------:|-----------:|
| RESP  | default  |     1 |      0 |     1 |          0 |

Queries
──────────────────────────────────────────────────────────────────────────────

[1x] testapp/views.py:14
SELECT COUNT(*) AS "__count__"
FROM "testapp_artist"
```

### Example repeated-query report

```text
══════════════════════════════════════════════════════════════════════════════
QUERY REPORT  GET /api/artists/  →  ArtistListAPIView
SOURCE        testapp/api_views.py:13
STATUS        200
TOTAL         51 queries in 0.0041s
══════════════════════════════════════════════════════════════════════════════

| Scope | Database | Reads | Writes | Total | Duplicates |
|-------|----------|------:|-------:|------:|-----------:|
| RESP  | default  |    51 |      0 |    51 |         50 |

Most repeated SQL
──────────────────────────────────────────────────────────────────────────────

[50x] testapp/api_views.py:13
[FIX: testapp/.gangstarr/<folder_path>]
SELECT "testapp_artist"."id", "testapp_artist"."name"
FROM "testapp_artist"
LIMIT 50

[200x] testapp/views.py:14
[FIX: testapp/.gangstarr/<folder_path>]
SELECT "albums"."AlbumId", "albums"."Title", "albums"."ArtistId"
FROM "albums"
WHERE "albums"."ArtistId" = #number#
```

### What each AI issue folder should contain

Each `.gangstarr` issue folder should include:

1. the correct context, code paths, and issue type
2. helpful Django performance hints and best practices
3. normalized SQL and deduplicated evidence
4. possible fixes
5. supporting metadata for AI tools

### Recommended issue-folder contents

* issue classification, such as:

  * N+1 query
  * repeated query
  * over-fetching
  * missing index
  * Python-side aggregation
* relevant source paths and snippets
* observed SQL and normalized SQL fingerprints
* Django-specific best-practice suggestions
* candidate fix strategies
* optional database evidence such as:

  * `EXPLAIN`
  * `EXPLAIN ANALYZE`
  * `pg_stat_statements` summaries

---

## 7. Static Analysis Strategy

Gangstarr should perform static analysis against a large Django codebase to identify patterns that commonly hurt performance.

A strong path would be to lean on Ruff if a plugin or custom-rule path is viable, or otherwise use Python AST tooling and Ruff-compatible parsing libraries where useful.

Reference inspiration: `django-check`
[https://github.com/richardhapb/django-check](https://github.com/richardhapb/django-check)

### Static analysis goals

* detect common ORM anti-patterns before runtime
* provide high-signal warnings
* recommend concrete Django fixes
* keep false positives low enough that developers trust the tool

### Top 10 Django ORM performance mistakes to detect

#### 1. N+1 Query Problem

**What happens:**
Querying related objects inside a loop triggers additional queries per row.

```python
books = Book.objects.all()

for book in books:
    print(book.author.name)
```

**Result:**
1 query for books + N queries for authors.

**Fix:**

```python
books = Book.objects.select_related("author")
```

Use:

* `select_related()` for foreign keys and one-to-one relationships
* `prefetch_related()` for many-to-many and reverse relations

---

#### 2. Fetching More Data Than Needed

Developers often retrieve full model objects when only a few fields are needed.

```python
users = User.objects.all()
```

**Fix:**

```python
users = User.objects.only("id", "email")
```

or

```python
users = User.objects.values("id", "email")
```

Benefits:

* less memory usage
* faster serialization
* reduced database load

---

#### 3. Using `len(queryset)` Instead of `count()`

```python
len(User.objects.filter(active=True))
```

**Problem:**
This loads all rows into memory.

**Fix:**

```python
User.objects.filter(active=True).count()
```

This performs an efficient SQL `COUNT()`.

---

#### 4. Iterating QuerySets Without `iterator()`

Large querysets load all rows into memory.

```python
for order in Order.objects.all():
    ...
```

**Fix:**

```python
for order in Order.objects.iterator(chunk_size=2000):
    ...
```

Benefits:

* streams rows from the database
* prevents memory spikes

---

#### 5. Missing Database Indexes

Filtering on non-indexed columns leads to slow scans.

```python
User.objects.filter(email="user@example.com")
```

**Fix:**

```python
class User(models.Model):
    email = models.EmailField(db_index=True)
```

Or:

```python
indexes = [
    models.Index(fields=["email"])
]
```

---

#### 6. Inefficient Bulk Operations

Looping over objects and calling `save()` triggers many queries.

```python
for user in users:
    user.active = True
    user.save()
```

**Fix:**

```python
User.objects.filter(...).update(active=True)
```

Or:

```python
User.objects.bulk_create(objects)
User.objects.bulk_update(objects, ["field"])
```

---

#### 7. Using Python Filtering Instead of Database Filtering

Bad:

```python
users = User.objects.all()
active = [u for u in users if u.is_active]
```

Good:

```python
active = User.objects.filter(is_active=True)
```

Filtering should be pushed into the database whenever possible.

---

#### 8. Not Using `exists()`

Checking presence inefficiently:

```python
if User.objects.filter(email=email):
    ...
```

**Fix:**

```python
User.objects.filter(email=email).exists()
```

This generates a fast `SELECT 1`.

---

#### 9. Multiple Queries for Aggregates

Bad:

```python
orders = Order.objects.filter(user=user)
total = sum(o.amount for o in orders)
```

**Fix:**

```python
from django.db.models import Sum

total = Order.objects.filter(user=user).aggregate(Sum("amount"))
```

Let the database do the computation.

---

#### 10. Re-evaluating QuerySets Repeatedly

QuerySets are lazy, but each evaluation can run the query again.

```python
qs = Product.objects.filter(active=True)

if qs:
    print(len(qs))
    for p in qs:
        ...
```

This can trigger multiple queries.

**Fix:**

```python
products = list(Product.objects.filter(active=True))
```

### Static analysis implementation direction

Gangstarr should be able to detect these patterns ahead of time and should ideally lean into Ruff-style rules or equivalent AST-based parsing infrastructure.

The goal is not just to report that something is wrong, but to produce a fix-oriented explanation tied to the developer’s actual code path.

---

## 8. PostgreSQL-Specific Strategy

This is where Gangstarr becomes more than just a Django linter.

PostgreSQL introspection should be used to understand what the database can already support and where SQL behavior can be improved.

### Key ideas

* inspect all tables, indexes, and schema metadata ahead of time
* correlate Django models with actual PostgreSQL table structure
* analyze repeated or expensive SQL from runtime activity
* use `pg_stat_statements` and related sources for deeper insight
* recommend indexes and query rewrites
* run `EXPLAIN` and query-plan analysis where appropriate

Reference inspiration: Monstrous
[https://gitlab.com/monstrous/monstrous/-/raw/main/lib/sql/tables.sql?ref_type=heads](https://gitlab.com/monstrous/monstrous/-/raw/main/lib/sql/tables.sql?ref_type=heads)

### PostgreSQL feature goals

* find slow or repeated SQL patterns
* recommend useful indexes
* identify missing composite indexes
* surface poor join patterns
* normalize SQL fingerprints for deduplication
* connect query evidence back to Django source locations

---

## 9. Django + PostgreSQL Opportunity Layer

This is where Gangstarr can become especially useful.

Python developers naturally prefer to keep logic in Python. That shift away from SQL happened for good reasons, but AI changes the equation. With strong context and tooling, writing targeted SQL can become much more practical again.

Gangstarr should help identify places where developers are overusing Python for work that belongs in the database.

### High-value pattern categories

* excessive loops over queryset results
* runtime-calculated fields done repeatedly in Python
* expensive model properties or cached properties
* signal-driven logic that may be better handled in the database
* repeated aggregate calculations done in Python instead of SQL

### Potential recommendations

* views
* materialized views
* SQL-side aggregates
* trigger-based workflows
* moving logic closer to PostgreSQL where it improves performance and clarity

### Examples of recommendation types

* **Django signals → database triggers**
* **heavy calculation loops → SQL aggregates or views**
* **repeated derived datasets → materialized views**
* **expensive Python property access → precomputed or query-level expressions**

---

## 10. Architecture Direction

Gangstarr is a Rust and Python project.

The division of labor should be practical.

### Python responsibilities

* Django middleware
* runtime query capture
* local integration
* configuration
* writing `.gangstarr` issue artifacts
* framework-specific glue code

### Rust responsibilities

* static analysis
* high-performance AST processing
* rule evaluation
* fast normalization or matching pipelines where speed matters

### Implementation principle

Lean on open-source projects first and write custom code second.

That keeps the project grounded, faster to ship, and easier to maintain.

---

## 11. Product Principles

### 1. Drop-in adoption

Gangstarr should be easy to add to an existing Django app.

That means:

* no migrations
* no wrappers developers must add around their code
* no heavy integration burden

### 2. Correct defaults

A config object should work correctly out of the box, with overrides only where teams need them.

### 3. High signal, low noise

Repeated requests should not flood the developer with duplicate findings. Deduplication is a core feature, not a nice-to-have.

### 4. Editor-friendly output

Source paths should be clickable in tools like VS Code or Zed. Fix folders should also be easy to open directly.

### 5. AI-ready by design

The `.gangstarr` folder should be designed so a user can hand one issue folder directly to an AI tool and get useful remediation help without extra manual setup.

### 6. PostgreSQL-first focus

Instead of trying to support every database equally, Gangstarr should go deep on PostgreSQL because that is where it can provide the most value.

---

## 12. Suggested MVP

A strong first version of Gangstarr would include:

1. Django middleware that captures request-scoped SQL and emits structured query reports
2. deduplicated `.gangstarr` issue folders for repeated SQL and common query anti-patterns
3. an initial static rule set covering the highest-value Django ORM mistakes
4. basic PostgreSQL schema and index introspection
5. AI-readable issue manifests that bundle source paths, SQL samples, and fix hints

---

## 13. Longer-Term Roadmap

After the MVP, Gangstarr could expand into:

* deeper `pg_stat_statements` analysis
* query-plan classification and scoring
* automated index recommendation confidence levels
* materialized view recommendations
* trigger recommendations
* editor integrations
* CI workflows for regression detection
* team-wide reporting and enforcement

---

## 14. Positioning Statement

Gangstarr helps Django teams on PostgreSQL build enterprise-ready applications by surfacing ORM mistakes, database inefficiencies, and Python-side performance abuse early, then packaging each issue into an AI-ready workflow that is easy to inspect, easy to fix, and easy to adopt.

---

## 15. Concise Product Summary

Gangstarr is a developer-first performance tool for Django + PostgreSQL.

It combines:

* static analysis
* runtime SQL inspection
* PostgreSQL-aware recommendations
* deduplicated AI-ready fix artifacts

The goal is simple:

**make it dead simple to write Django applications that are not only pleasant to build, but actually performant at scale.**
