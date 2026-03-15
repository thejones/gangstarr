use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Convert a camelCase or PascalCase GraphQL field name to snake_case.
///
/// Examples:
///   "artistsWithAlbumsAndTracks" → "artists_with_albums_and_tracks"
///   "allArtists" → "all_artists"
///   "id" → "id"
///   "__typename" → "__typename"
pub fn camel_to_snake(name: &str) -> String {
    // Pass through names that are already snake_case or dunder
    if name.starts_with("__") || !name.contains(|c: char| c.is_uppercase()) {
        return name.to_string();
    }

    let mut result = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() {
            // Insert underscore before uppercase if:
            // - not the first character
            // - previous char was lowercase, OR
            // - next char is lowercase (handles "XMLParser" → "xml_parser")
            if i > 0 {
                let prev_lower = chars[i - 1].is_lowercase();
                let next_lower = chars.get(i + 1).map_or(false, |c| c.is_lowercase());
                if prev_lower || next_lower {
                    result.push('_');
                }
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

/// Input: a single file to scan.
#[derive(Debug, Deserialize)]
pub struct FileInput {
    pub path: String,
    pub content: String,
}

/// Output: a resolved source location for a GraphQL resolver.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedLocation {
    pub file: String,
    pub line: u32,
    pub source: String,
    /// "explicit" (def resolve_*) or "implicit" (field declaration)
    pub kind: String,
}

/// A GraphQL type class found during scanning.
struct TypeClass {
    /// The class name, e.g. "ArtistType", "Query"
    name: String,
    /// Line number of the class definition
    _line: u32,
    /// The parent class(es) string, e.g. "DjangoObjectType", "graphene.ObjectType"
    parent: String,
    /// Start line of the class body
    start: usize,
    /// End line of the class body (exclusive)
    end: usize,
}

/// Scan a list of Python files and build a resolver index.
///
/// Returns a map of "TypeName.fieldName" → ResolvedLocation.
pub fn scan_files(files: &[FileInput]) -> HashMap<String, ResolvedLocation> {
    let class_re =
        Regex::new(r"^class\s+(\w+)\s*\(([^)]*(?:ObjectType|DjangoObjectType)[^)]*)\)\s*:")
            .unwrap();
    let resolver_re = Regex::new(r"^\s+def\s+resolve_(\w+)\s*\(").unwrap();
    // Match fields = ('a', 'b', 'c') or fields = ("a", "b") — handles tuples and lists
    let fields_re = Regex::new(r#"^\s+fields\s*=\s*[\(\[](.*?)[\)\]]"#).unwrap();
    // Individual field names inside quotes
    let field_name_re = Regex::new(r#"['"](\w+)['"]"#).unwrap();

    let mut index: HashMap<String, ResolvedLocation> = HashMap::new();

    for file in files {
        let lines: Vec<&str> = file.content.lines().collect();

        // Pass 1: find all GraphQL type classes and their line ranges
        let mut classes: Vec<TypeClass> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = class_re.captures(line) {
                let name = caps[1].to_string();
                let parent = caps[2].to_string();
                classes.push(TypeClass {
                    name,
                    _line: (i + 1) as u32,
                    parent,
                    start: i,
                    end: lines.len(), // will be narrowed below
                });
            }
        }

        // Narrow class end boundaries: each class ends where the next top-level
        // definition starts (class or def at indent 0)
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

        // Pass 2: within each class, find resolvers and field declarations
        for cls in &classes {
            let is_django_type = cls.parent.contains("DjangoObjectType");

            for i in cls.start..cls.end {
                let line = lines[i];
                let line_no = (i + 1) as u32;

                // Explicit resolvers: def resolve_foo(self, ...)
                if let Some(caps) = resolver_re.captures(line) {
                    let snake_name = &caps[1];
                    // The GraphQL field name is the camelCase version, but we
                    // store by ClassName.snake_name so Python can look up both
                    // camelCase and snake_case variants.
                    let key = format!("{}.{}", cls.name, snake_name);
                    index.insert(
                        key,
                        ResolvedLocation {
                            file: file.path.clone(),
                            line: line_no,
                            source: line.trim().to_string(),
                            kind: "explicit".to_string(),
                        },
                    );
                }

                // Implicit field declarations in DjangoObjectType Meta
                if is_django_type {
                    if let Some(caps) = fields_re.captures(line) {
                        let fields_str = &caps[1];
                        for fcap in field_name_re.captures_iter(fields_str) {
                            let field_name = &fcap[1];
                            let key = format!("{}.{}", cls.name, field_name);
                            // Don't overwrite explicit resolvers
                            index.entry(key).or_insert_with(|| ResolvedLocation {
                                file: file.path.clone(),
                                line: line_no,
                                source: line.trim().to_string(),
                                kind: "implicit".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_snake_basic() {
        assert_eq!(camel_to_snake("artistsWithAlbumsAndTracks"), "artists_with_albums_and_tracks");
        assert_eq!(camel_to_snake("allArtists"), "all_artists");
        assert_eq!(camel_to_snake("id"), "id");
        assert_eq!(camel_to_snake("__typename"), "__typename");
        assert_eq!(camel_to_snake("unitPrice"), "unit_price");
    }

    #[test]
    fn test_camel_to_snake_already_snake() {
        assert_eq!(camel_to_snake("already_snake"), "already_snake");
        assert_eq!(camel_to_snake("simple"), "simple");
    }

    #[test]
    fn test_camel_to_snake_pascal_case() {
        assert_eq!(camel_to_snake("ArtistType"), "artist_type");
        assert_eq!(camel_to_snake("XMLParser"), "xml_parser");
    }

    #[test]
    fn test_scan_explicit_resolver() {
        let content = r#"
import graphene

class Query(graphene.ObjectType):
    all_artists = graphene.List(ArtistType)
    artists_with_albums_and_tracks = graphene.List(ArtistType)

    def resolve_all_artists(self, info, limit=10):
        return Artist.objects.prefetch_related('albums')[:limit]

    def resolve_artists_with_albums_and_tracks(self, info, limit=10):
        return Artist.objects.all()[:limit]
"#;
        let files = vec![FileInput {
            path: "testapp/schema.py".to_string(),
            content: content.to_string(),
        }];
        let index = scan_files(&files);

        assert!(index.contains_key("Query.all_artists"));
        let loc = &index["Query.all_artists"];
        assert_eq!(loc.file, "testapp/schema.py");
        assert_eq!(loc.kind, "explicit");
        assert!(loc.source.contains("def resolve_all_artists"));

        assert!(index.contains_key("Query.artists_with_albums_and_tracks"));
    }

    #[test]
    fn test_scan_implicit_fields() {
        let content = r#"
from graphene_django import DjangoObjectType

class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ('id', 'name', 'albums')

class AlbumType(DjangoObjectType):
    class Meta:
        model = Album
        fields = ('id', 'title', 'artist', 'tracks')
"#;
        let files = vec![FileInput {
            path: "testapp/schema.py".to_string(),
            content: content.to_string(),
        }];
        let index = scan_files(&files);

        assert!(index.contains_key("ArtistType.albums"));
        let loc = &index["ArtistType.albums"];
        assert_eq!(loc.kind, "implicit");

        assert!(index.contains_key("AlbumType.tracks"));
        assert!(index.contains_key("ArtistType.id"));
    }
}
