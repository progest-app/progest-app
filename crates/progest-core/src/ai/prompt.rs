use std::fmt::Write;

use super::types::{AiContext, SuggestionType};

const MAX_SIBLINGS: usize = 30;
const MAX_RULES: usize = 10;
const MAX_TAGS: usize = 100;
const MAX_DIRS: usize = 50;

pub fn build_prompt(suggestion_type: SuggestionType, ctx: &AiContext) -> (String, String) {
    match suggestion_type {
        SuggestionType::Naming => build_naming(ctx),
        SuggestionType::Tags => build_tags(ctx),
        SuggestionType::Notes => build_notes(ctx),
        SuggestionType::Placement => build_placement(ctx),
    }
}

fn build_naming(ctx: &AiContext) -> (String, String) {
    let system = "\
You are a file naming assistant for creative pipeline projects (VFX, games, 3DCG). \
Suggest clean filenames that follow the project's naming conventions. \
Respond ONLY with a JSON array. Each element must have a \"value\" (the suggested filename \
including extension) and an optional \"explanation\". Return 1-3 suggestions.";

    let mut user = String::new();
    let _ = writeln!(user, "Current filename: {}", ctx.file_name);
    let _ = writeln!(user, "Parent directory: {}", ctx.parent_dir);
    if let Some(ext) = &ctx.file_extension {
        let _ = writeln!(user, "Extension: .{ext}");
    }
    append_rules(&mut user, ctx);
    append_siblings(&mut user, ctx);
    append_glossary(&mut user, ctx);
    user.push_str("\nSuggest improved filenames that follow the naming conventions above.");
    (system.to_string(), user)
}

fn build_tags(ctx: &AiContext) -> (String, String) {
    let system = "\
You are a tagging assistant for creative pipeline projects. \
Suggest relevant tags based on the file context and the project's existing tag vocabulary. \
Tags must match the pattern [a-zA-Z0-9_-]+. \
Respond ONLY with a JSON array. Each element must have a \"value\" (the tag) and an optional \
\"explanation\". Return 3-5 suggestions.";

    let mut user = String::new();
    let _ = writeln!(user, "Filename: {}", ctx.file_name);
    let _ = writeln!(user, "Parent directory: {}", ctx.parent_dir);
    if let Some(ext) = &ctx.file_extension {
        let _ = writeln!(user, "Extension: .{ext}");
    }
    if !ctx.existing_tags.is_empty() {
        let _ = writeln!(
            user,
            "Existing tags on this file: {}",
            ctx.existing_tags.join(", ")
        );
    }
    if !ctx.project_tag_vocabulary.is_empty() {
        let tags: Vec<&str> = ctx
            .project_tag_vocabulary
            .iter()
            .take(MAX_TAGS)
            .map(String::as_str)
            .collect();
        let _ = writeln!(
            user,
            "Project tag vocabulary (prefer reusing these): {}",
            tags.join(", ")
        );
    }
    append_glossary(&mut user, ctx);
    user.push_str("\nSuggest tags for this file. Avoid duplicating existing tags.");
    (system.to_string(), user)
}

fn build_notes(ctx: &AiContext) -> (String, String) {
    let system = "\
You are a documentation assistant for creative pipeline projects. \
Generate a concise descriptive note for the given file. \
Respond ONLY with a JSON array with a single element containing \"value\" (the note text) \
and an optional \"explanation\".";

    let mut user = String::new();
    let _ = writeln!(user, "Filename: {}", ctx.file_name);
    let _ = writeln!(user, "Parent directory: {}", ctx.parent_dir);
    if let Some(ext) = &ctx.file_extension {
        let _ = writeln!(user, "Extension: .{ext}");
    }
    if !ctx.existing_tags.is_empty() {
        let _ = writeln!(user, "Tags: {}", ctx.existing_tags.join(", "));
    }
    if let Some(notes) = &ctx.notes_body {
        let _ = writeln!(user, "Existing notes (update or enhance):\n{notes}");
    }
    append_siblings(&mut user, ctx);
    append_rules(&mut user, ctx);
    append_glossary(&mut user, ctx);
    user.push_str("\nWrite a concise note describing this file's purpose and content.");
    (system.to_string(), user)
}

fn build_placement(ctx: &AiContext) -> (String, String) {
    let system = "\
You are a file organization assistant for creative pipeline projects. \
Suggest the best directory for this file based on the project structure and \
directory accept rules. \
Respond ONLY with a JSON array. Each element must have a \"value\" (the directory path) \
and an optional \"explanation\". Return 1-3 suggestions.";

    let mut user = String::new();
    let _ = writeln!(user, "Filename: {}", ctx.file_name);
    let _ = writeln!(user, "Current location: {}", ctx.parent_dir);
    if let Some(ext) = &ctx.file_extension {
        let _ = writeln!(user, "Extension: .{ext}");
    }
    if !ctx.project_dirs.is_empty() {
        let dirs: Vec<&str> = ctx
            .project_dirs
            .iter()
            .take(MAX_DIRS)
            .map(String::as_str)
            .collect();
        let _ = writeln!(user, "Project directories:\n{}", dirs.join("\n"));
    }
    if !ctx.dir_accepts.is_empty() {
        user.push_str("Directory accept rules:\n");
        for (dir, exts) in ctx.dir_accepts.iter().take(MAX_DIRS) {
            let _ = writeln!(user, "  {dir}: {}", exts.join(", "));
        }
    }
    append_glossary(&mut user, ctx);
    user.push_str("\nSuggest the best directory for this file.");
    (system.to_string(), user)
}

fn append_rules(out: &mut String, ctx: &AiContext) {
    if ctx.rule_summaries.is_empty() {
        return;
    }
    out.push_str("Naming rules:\n");
    for rule in ctx.rule_summaries.iter().take(MAX_RULES) {
        let _ = writeln!(out, "  - {rule}");
    }
}

fn append_siblings(out: &mut String, ctx: &AiContext) {
    if ctx.sibling_names.is_empty() {
        return;
    }
    let siblings: Vec<&str> = ctx
        .sibling_names
        .iter()
        .take(MAX_SIBLINGS)
        .map(String::as_str)
        .collect();
    let _ = writeln!(out, "Sibling files: {}", siblings.join(", "));
}

fn append_glossary(out: &mut String, ctx: &AiContext) {
    if ctx.glossary.is_empty() {
        return;
    }
    let _ = writeln!(out, "Project glossary: {}", ctx.glossary.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> AiContext {
        AiContext {
            file_name: "hero_comp_v03.psd".into(),
            file_extension: Some("psd".into()),
            parent_dir: "shots/sh010/comp".into(),
            sibling_names: vec!["hero_comp_v01.psd".into(), "hero_comp_v02.psd".into()],
            existing_tags: vec!["wip".into(), "comp".into()],
            project_tag_vocabulary: vec![
                "wip".into(),
                "review".into(),
                "final".into(),
                "comp".into(),
            ],
            notes_body: None,
            rule_summaries: vec![
                "rule_shot: template={shot}_{task}_v{ver}, applies_to=*.psd".into(),
            ],
            project_dirs: vec!["shots/sh010/comp".into(), "shots/sh010/plate".into()],
            dir_accepts: vec![
                (
                    "shots/sh010/comp".into(),
                    vec![".psd".into(), ".exr".into()],
                ),
                ("shots/sh010/plate".into(), vec![".exr".into()]),
            ],
            glossary: vec!["VFX".into(), "comp".into()],
        }
    }

    #[test]
    fn naming_prompt_includes_context() {
        let ctx = sample_context();
        let (sys, user) = build_prompt(SuggestionType::Naming, &ctx);
        assert!(sys.contains("naming assistant"));
        assert!(sys.contains("JSON array"));
        assert!(user.contains("hero_comp_v03.psd"));
        assert!(user.contains("shots/sh010/comp"));
        assert!(user.contains("rule_shot"));
        assert!(user.contains("hero_comp_v01.psd"));
        assert!(user.contains("VFX"));
    }

    #[test]
    fn tags_prompt_includes_vocabulary() {
        let ctx = sample_context();
        let (sys, user) = build_prompt(SuggestionType::Tags, &ctx);
        assert!(sys.contains("tagging assistant"));
        assert!(user.contains("wip, comp"));
        assert!(user.contains("review"));
    }

    #[test]
    fn notes_prompt_excludes_notes_when_none() {
        let ctx = sample_context();
        let (_, user) = build_prompt(SuggestionType::Notes, &ctx);
        assert!(!user.contains("Existing notes"));
    }

    #[test]
    fn notes_prompt_includes_notes_when_present() {
        let mut ctx = sample_context();
        ctx.notes_body = Some("Hero composite for shot 10.".into());
        let (_, user) = build_prompt(SuggestionType::Notes, &ctx);
        assert!(user.contains("Hero composite for shot 10."));
    }

    #[test]
    fn placement_prompt_includes_dir_structure() {
        let ctx = sample_context();
        let (sys, user) = build_prompt(SuggestionType::Placement, &ctx);
        assert!(sys.contains("organization assistant"));
        assert!(user.contains("shots/sh010/comp"));
        assert!(user.contains(".psd, .exr"));
    }

    #[test]
    fn truncation_respects_limits() {
        let mut ctx = sample_context();
        ctx.sibling_names = (0..50).map(|i| format!("file_{i:03}.psd")).collect();
        let (_, user) = build_prompt(SuggestionType::Naming, &ctx);
        let count = user.matches("file_").count();
        assert!(count <= MAX_SIBLINGS);
    }
}
