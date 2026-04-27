//! `progest search <query>` — DSL-driven search over the index.
//!
//! Pipeline: parse → validate (against `schema.toml` custom fields)
//! → plan → execute → project rich hits → emit text or JSON per
//! `docs/SEARCH_DSL.md` §8. `--view <id>` resolves a saved view from
//! `views.toml` and uses its query.
//!
//! Exit codes follow §8.3:
//! - `0` — success (zero hits is still 0, queries don't fail on empty)
//! - `2` — parse error / unknown reserved key
//! - `3` — internal error (sqlite / IO)

use std::path::Path;

use anyhow::{Context, Result};
use progest_core::search::{
    CustomFieldKind, CustomFields, RichSearchHit, ValidatedQuery, execute, parse, plan,
    project_hits, validate_with_catalog,
    views::{ViewError, load as load_views_doc},
};
use serde::Serialize;

use crate::context::{discover_root, load_alias_catalog_from_root, open_index};
use crate::output::{OutputFormat, emit_json};

pub struct SearchArgs {
    pub query: Option<String>,
    pub view: Option<String>,
    pub format: OutputFormat,
    pub explain: bool,
}

/// CLI driver. Returns the exit code (0 / 2 / 3).
pub fn run(cwd: &Path, args: &SearchArgs) -> Result<i32> {
    let root = discover_root(cwd)?;

    // Resolve query: explicit text wins; else --view id from views.toml.
    let query_text = match (&args.query, &args.view) {
        (Some(q), _) => q.clone(),
        (None, Some(view_id)) => match load_view(&root, view_id) {
            Ok(q) => q,
            Err(SearchCliError::NotFound) => {
                eprintln!("error: view {view_id:?} not found");
                return Ok(2);
            }
            Err(SearchCliError::Other(e)) => {
                eprintln!("error: {e}");
                return Ok(3);
            }
        },
        (None, None) => {
            eprintln!("error: provide a query or --view <id>");
            return Ok(2);
        }
    };

    // Parse → validate → plan.
    let parsed = match parse(&query_text) {
        Ok(p) => p,
        Err(e) => {
            emit_parse_error(&query_text, &e, args.format)?;
            return Ok(2);
        }
    };
    let schema = load_schema(&root).unwrap_or_default();
    let aliases = load_alias_catalog_from_root(&root).unwrap_or_default();
    let validated = validate_with_catalog(&parsed, &schema, &aliases);
    let planned = plan(&validated);

    // Execute against the index.
    let index = open_index(&root).context("opening index")?;
    let hits = match index.with_connection(|conn| execute(conn, &planned)) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(3);
        }
    };

    // Project the rich shape.
    let rich = match project_hits(&index, &hits) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(3);
        }
    };

    emit(&query_text, &rich, &validated, args)?;
    Ok(0)
}

#[derive(Debug, Serialize)]
struct SearchEnvelope<'a> {
    query: &'a str,
    result_count: usize,
    warnings: Vec<String>,
    hits: &'a [RichSearchHit],
}

fn emit(
    query: &str,
    rich: &[RichSearchHit],
    validated: &ValidatedQuery,
    args: &SearchArgs,
) -> Result<()> {
    let warnings: Vec<String> = validated.warnings.iter().map(ToString::to_string).collect();
    match args.format {
        OutputFormat::Text => {
            for h in rich {
                emit_one_text(h);
            }
            if !warnings.is_empty() && args.explain {
                eprintln!("warnings:");
                for w in &warnings {
                    eprintln!("  - {w}");
                }
            }
        }
        OutputFormat::Json => {
            let env = SearchEnvelope {
                query,
                result_count: rich.len(),
                warnings,
                hits: rich,
            };
            emit_json(&env, "search")?;
        }
    }
    Ok(())
}

fn emit_one_text(h: &RichSearchHit) {
    let mut parts = vec![h.path.clone()];
    if !h.tags.is_empty() {
        parts.push(format!("tags:{}", h.tags.join(",")));
    }
    let total_violations = h.violations.naming + h.violations.placement + h.violations.sequence;
    if total_violations > 0 {
        let mut marks = Vec::new();
        if h.violations.naming > 0 {
            marks.push("naming");
        }
        if h.violations.placement > 0 {
            marks.push("placement");
        }
        if h.violations.sequence > 0 {
            marks.push("sequence");
        }
        parts.push(format!("★{}", marks.join(",")));
    }
    println!("{}", parts.join("  "));
}

fn emit_parse_error(
    query: &str,
    e: &progest_core::search::ParseError,
    format: OutputFormat,
) -> Result<()> {
    match format {
        OutputFormat::Text => {
            eprintln!("error: {e}");
            if let Some(col) = e.column() {
                eprintln!("  {query}");
                eprintln!("  {:>col$}", "^", col = col);
            }
        }
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct ParseErrorPayload<'a> {
                ok: bool,
                error: ErrorBody<'a>,
            }
            #[derive(Serialize)]
            struct ErrorBody<'a> {
                kind: &'a str,
                message: String,
                #[serde(skip_serializing_if = "Option::is_none")]
                column: Option<usize>,
            }
            let payload = ParseErrorPayload {
                ok: false,
                error: ErrorBody {
                    kind: "parse",
                    message: e.to_string(),
                    column: e.column(),
                },
            };
            emit_json(&payload, "search")?;
        }
    }
    Ok(())
}

#[derive(Debug)]
enum SearchCliError {
    NotFound,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for SearchCliError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

fn load_view(
    root: &progest_core::project::ProjectRoot,
    id: &str,
) -> std::result::Result<String, SearchCliError> {
    use progest_core::fs::{ProjectPath, StdFileSystem};
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let path = ProjectPath::new(".progest/views.toml")
        .map_err(|e| SearchCliError::Other(anyhow::anyhow!("{e}")))?;
    let doc = match load_views_doc(&fs, &path) {
        Ok(d) => d,
        Err(ViewError::NotFound) => return Err(SearchCliError::NotFound),
        Err(e) => return Err(SearchCliError::Other(anyhow::anyhow!("{e}"))),
    };
    let view = doc
        .views
        .iter()
        .find(|v| v.id == id)
        .ok_or(SearchCliError::NotFound)?;
    Ok(view.query.clone())
}

fn load_schema(root: &progest_core::project::ProjectRoot) -> Option<CustomFields> {
    let path = root.schema_toml();
    let text = std::fs::read_to_string(&path).ok()?;
    parse_schema_toml(&text)
}

fn parse_schema_toml(text: &str) -> Option<CustomFields> {
    use std::collections::BTreeMap;
    #[derive(serde::Deserialize)]
    struct Doc {
        #[serde(default)]
        custom_fields: BTreeMap<String, FieldEntry>,
    }
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase")]
    enum FieldEntry {
        String,
        Int,
        Enum { values: Vec<String> },
    }
    let doc: Doc = toml::from_str(text).ok()?;
    let mut schema = CustomFields::new();
    for (name, entry) in doc.custom_fields {
        let kind = match entry {
            FieldEntry::String => CustomFieldKind::String,
            FieldEntry::Int => CustomFieldKind::Int,
            FieldEntry::Enum { values } => CustomFieldKind::Enum { values },
        };
        schema.insert(name, kind);
    }
    Some(schema)
}
