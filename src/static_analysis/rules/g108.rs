use std::collections::HashSet;

use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::Rule;

/// G108 — GraphQL implicit resolver N+1.
///
/// Flags `DjangoObjectType` classes that expose a related field (FK / reverse
/// FK) in their `fields` tuple WITHOUT a corresponding `def resolve_<field>`
/// method.  The implicit default resolver triggers one SQL query per parent
/// row — the classic GraphQL N+1 problem.
///
/// The rule also checks whether an explicit resolver exists but does NOT use a
/// DataLoader (heuristic: the resolver body mentions `dataloader` or
/// `loader` or `info.context.loaders`).  When a resolver exists with a
/// DataLoader, the field is considered optimised and is not flagged.
pub struct G108 {
    class_re: Regex,
    fields_re: Regex,
    field_name_re: Regex,
    resolver_re: Regex,
    model_re: Regex,
}

impl G108 {
    pub fn new() -> Self {
        G108 {
            // Match: class FooType(DjangoObjectType):
            class_re: Regex::new(
                r"^class\s+(\w+)\s*\([^)]*DjangoObjectType[^)]*\)\s*:",
            )
            .unwrap(),
            // Match: fields = ('a', 'b') or fields = ["a", "b"]
            fields_re: Regex::new(r#"^\s+fields\s*=\s*[\(\[](.*?)[\)\]]"#).unwrap(),
            // Individual quoted field names
            field_name_re: Regex::new(r#"['"](\w+)['"]"#).unwrap(),
            // Match: def resolve_foo(self, ...)
            resolver_re: Regex::new(r"^\s+(?:async\s+)?def\s+resolve_(\w+)\s*\(").unwrap(),
            // Match: model = SomeModel
            model_re: Regex::new(r"^\s+model\s*=\s*(\w+)").unwrap(),
        }
    }
}

/// Heuristic list of field names that are almost certainly scalar (not
/// relations).  We skip these to reduce false positives — the rule only cares
/// about fields that look like related-object traversals.
const SCALAR_FIELD_NAMES: &[&str] = &[
    "id", "pk", "name", "title", "slug", "email", "phone", "fax",
    "address", "city", "state", "country", "postal_code",
    "description", "content", "body", "text", "summary",
    "created_at", "updated_at", "modified_at", "deleted_at",
    "birth_date", "hire_date", "start_date", "end_date", "date",
    "first_name", "last_name", "username", "password",
    "is_active", "is_staff", "is_superuser", "is_published",
    "price", "unit_price", "total", "amount", "quantity",
    "milliseconds", "bytes", "duration", "size", "count", "order",
    "url", "image", "file", "path", "type", "status", "code",
    "invoice_date", "billing_address", "billing_city",
    "billing_state", "billing_country", "billing_postal_code",
    "composer", "last_name", "first_name",
];

impl Rule for G108 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        // Quick bail: only applies to files that define DjangoObjectType subclasses.
        if !source.contains("DjangoObjectType") {
            return Vec::new();
        }

        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // ── Pass 1: identify DjangoObjectType class regions ──────────────
        let mut classes: Vec<GraphQLClass> = Vec::new();

        for (i, &line) in lines.iter().enumerate() {
            if let Some(caps) = self.class_re.captures(line) {
                classes.push(GraphQLClass {
                    type_name: caps[1].to_string(),
                    model_name: None,
                    start: i,
                    end: lines.len(),
                    fields_line: 0,
                    declared_fields: Vec::new(),
                    explicit_resolvers: HashSet::new(),
                    has_dataloader: HashSet::new(),
                });
            }
        }

        // Narrow end boundaries.
        for ci in 0..classes.len() {
            let search_start = classes[ci].start + 1;
            for j in search_start..lines.len() {
                let line = lines[j];
                if !line.is_empty()
                    && !line.starts_with(' ')
                    && !line.starts_with('\t')
                    && !line.starts_with('#')
                {
                    classes[ci].end = j;
                    break;
                }
            }
        }

        // ── Pass 2: inside each class, collect fields, model, resolvers ──
        for cls in &mut classes {
            for i in cls.start..cls.end {
                let line = lines[i];

                // Model name
                if let Some(caps) = self.model_re.captures(line) {
                    cls.model_name = Some(caps[1].to_string());
                }

                // fields = (...)
                if let Some(caps) = self.fields_re.captures(line) {
                    cls.fields_line = i + 1;
                    for fcap in self.field_name_re.captures_iter(&caps[1]) {
                        cls.declared_fields.push(fcap[1].to_string());
                    }
                }

                // def resolve_foo(...)
                if let Some(caps) = self.resolver_re.captures(line) {
                    let field_name = caps[1].to_string();

                    // Peek at the resolver body for DataLoader hints.
                    let body_end = (i + 15).min(cls.end);
                    let body_has_loader = (i + 1..body_end).any(|bi| {
                        let bline = lines[bi].to_lowercase();
                        bline.contains("loader") || bline.contains("dataloader")
                    });

                    if body_has_loader {
                        cls.has_dataloader.insert(field_name.clone());
                    }
                    cls.explicit_resolvers.insert(field_name);
                }
            }
        }

        // ── Pass 3: flag related fields without a resolver or DataLoader ─
        let scalar_set: HashSet<&str> = SCALAR_FIELD_NAMES.iter().copied().collect();

        for cls in &classes {
            for field in &cls.declared_fields {
                // Skip obviously-scalar fields.
                if scalar_set.contains(field.as_str()) {
                    continue;
                }

                let has_resolver = cls.explicit_resolvers.contains(field);
                let has_loader = cls.has_dataloader.contains(field);

                if !has_resolver {
                    // No resolver at all → implicit default will N+1.
                    let model_hint = cls
                        .model_name
                        .as_deref()
                        .map(|m| format!(" on {}", m))
                        .unwrap_or_default();
                    findings.push(StaticFinding {
                        rule: "G108".to_string(),
                        message: format!(
                            "GraphQL N+1: `{}.{}` is an implicit resolver{} — each parent row triggers a separate query",
                            cls.type_name, field, model_hint
                        ),
                        severity: Severity::Warning,
                        file: file.to_string(),
                        line: cls.fields_line,
                        col: 0,
                        suggestion: Some(format!(
                            "Add a DataLoader for `{}` or use graphene-django-optimizer to batch related lookups",
                            field
                        )),
                    });
                } else if !has_loader {
                    // Has a resolver but no DataLoader → still may N+1.
                    findings.push(StaticFinding {
                        rule: "G108".to_string(),
                        message: format!(
                            "GraphQL N+1: `{}.{}` has a resolver but no DataLoader — may still issue per-row queries",
                            cls.type_name, field
                        ),
                        severity: Severity::Warning,
                        file: file.to_string(),
                        line: cls.fields_line,
                        col: 0,
                        suggestion: Some(format!(
                            "Consider a DataLoader for `{}` to batch queries across all parent rows",
                            field
                        )),
                    });
                }
                // has_resolver && has_loader → optimised, no finding.
            }
        }

        findings
    }
}

struct GraphQLClass {
    type_name: String,
    model_name: Option<String>,
    start: usize,
    end: usize,
    /// 1-indexed line of `fields = (...)`.
    fields_line: usize,
    declared_fields: Vec<String>,
    explicit_resolvers: HashSet<String>,
    /// Resolvers whose body references a DataLoader.
    has_dataloader: HashSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_implicit_related_field() {
        let src = r#"
from graphene_django import DjangoObjectType

class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ('id', 'name', 'albums')
"#;
        let findings = G108::new().check("schema.py", src);
        let g108: Vec<_> = findings.iter().filter(|f| f.rule == "G108").collect();
        assert_eq!(g108.len(), 1, "should flag 'albums' as implicit N+1");
        assert!(g108[0].message.contains("albums"));
        assert!(g108[0].suggestion.as_ref().unwrap().contains("DataLoader"));
    }

    #[test]
    fn test_no_flag_with_dataloader_resolver() {
        let src = r#"
from graphene_django import DjangoObjectType

class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ("id", "name")

    albums = graphene.List(lambda: AlbumType)

    async def resolve_albums(self, info):
        return await info.context.loaders["albums_by_artist"].load(self.id)
"#;
        let findings = G108::new().check("schema_dl.py", src);
        let g108: Vec<_> = findings.iter().filter(|f| f.rule == "G108").collect();
        assert!(g108.is_empty(), "should not flag resolver with DataLoader");
    }

    #[test]
    fn test_flags_resolver_without_dataloader() {
        let src = r#"
from graphene_django import DjangoObjectType

class AlbumType(DjangoObjectType):
    class Meta:
        model = Album
        fields = ('id', 'title', 'artist', 'tracks')

    def resolve_tracks(self, info):
        return self.tracks.all()
"#;
        let findings = G108::new().check("schema.py", src);
        let g108: Vec<_> = findings.iter().filter(|f| f.rule == "G108").collect();
        // 'artist' has no resolver → flagged as implicit
        // 'tracks' has a resolver but no DataLoader → flagged
        assert_eq!(g108.len(), 2);
    }

    #[test]
    fn test_skips_scalar_fields() {
        let src = r#"
from graphene_django import DjangoObjectType

class TrackType(DjangoObjectType):
    class Meta:
        model = Track
        fields = ('id', 'name', 'milliseconds', 'unit_price')
"#;
        let findings = G108::new().check("schema.py", src);
        assert!(findings.is_empty(), "should not flag scalar-only fields");
    }

    #[test]
    fn test_skips_non_graphql_files() {
        let src = r#"
from django.db import models

class Artist(models.Model):
    name = models.CharField(max_length=120)
"#;
        let findings = G108::new().check("models.py", src);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_multiple_types() {
        let src = r#"
from graphene_django import DjangoObjectType

class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ('id', 'name', 'albums')

class AlbumType(DjangoObjectType):
    class Meta:
        model = Album
        fields = ('id', 'title', 'tracks')
"#;
        let findings = G108::new().check("schema.py", src);
        let g108: Vec<_> = findings.iter().filter(|f| f.rule == "G108").collect();
        assert_eq!(g108.len(), 2, "should flag 'albums' and 'tracks'");
    }
}
