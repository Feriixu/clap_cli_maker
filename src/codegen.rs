use crate::model::{ArgDef, ArgKind, CommandNode, FlattenGroup, FlattenRef, Project};
use heck::{ToKebabCase, ToPascalCase, ToSnakeCase};
use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};
use uuid::Uuid;

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "abstract", "become", "box", "do", "final",
    "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

fn normalize_field_ident(raw: &str) -> String {
    let mut s = raw.to_snake_case();
    if s.is_empty() {
        s = "field".to_string();
    }
    if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s = format!("_{s}");
    }
    if RUST_KEYWORDS.contains(&s.as_str()) {
        s.push('_');
    }
    s
}

struct Ctx {
    items: Vec<String>,
    imports: BTreeSet<&'static str>,
    uses_subcommand: bool,
    uses_args: bool,
    uses_value_enum: bool,
    used_groups: HashSet<Uuid>,
}

/// Appends `#[command(args_conflicts_with_subcommands = true)]` /
/// `#[command(subcommand_negates_reqs = true)]` when set. Shared by the root
/// `Cli` struct and every generated subcommand `Args` struct.
fn push_subcommand_interaction_settings(node: &CommandNode, parts: &mut Vec<String>) {
    if node.args_conflicts_with_subcommands {
        parts.push("args_conflicts_with_subcommands = true".to_string());
    }
    if node.subcommand_negates_reqs {
        parts.push("subcommand_negates_reqs = true".to_string());
    }
}

/// Emits `#[command(flatten)]` fields for every `FlattenRef` on a command,
/// resolving each against the project's shared `FlattenGroup` definitions
/// and recording which groups actually get used.
fn gen_flatten_fields(flattens: &[FlattenRef], groups: &[FlattenGroup], ctx: &mut Ctx) -> String {
    let mut out = String::new();
    for fref in flattens {
        let Some(group) = groups.iter().find(|g| g.id == fref.group_id) else {
            continue;
        };
        ctx.uses_args = true;
        ctx.used_groups.insert(group.id);
        let group_ident = group.display_ident();
        let field_source = fref
            .field_name_override
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&group.name);
        let field_name = normalize_field_ident(field_source);
        let ty = if fref.optional {
            format!("Option<{group_ident}>")
        } else {
            group_ident
        };
        out.push_str("    #[command(flatten)]\n");
        out.push_str(&format!("    pub {field_name}: {ty},\n"));
    }
    out
}

fn gen_flatten_group_struct(group: &FlattenGroup, ctx: &mut Ctx) {
    ctx.uses_args = true;
    let scope = group.display_ident();
    let mut fields = String::new();
    for arg in &group.args {
        fields.push_str(&gen_field(arg, &scope, ctx));
    }
    ctx.items.push(format!(
        "#[derive(Args, Debug)]\npub struct {scope} {{\n{fields}}}\n"
    ));
}

/// Builds the raw (unformatted) generated Rust source for a project's CLI.
pub fn generate_source(project: &Project) -> String {
    let mut ctx = Ctx {
        items: Vec::new(),
        imports: BTreeSet::new(),
        uses_subcommand: false,
        uses_args: false,
        uses_value_enum: false,
        used_groups: HashSet::new(),
    };
    let groups = &project.flatten_groups;

    let root = &project.root;
    let mut fields = String::new();
    for arg in &root.args {
        fields.push_str(&gen_field(arg, "Cli", &mut ctx));
    }
    fields.push_str(&gen_flatten_fields(&root.flattens, groups, &mut ctx));
    if !root.subcommands.is_empty() {
        ctx.uses_subcommand = true;
        let field_ty = if root.require_subcommand {
            "Commands".to_string()
        } else {
            "Option<Commands>".to_string()
        };
        fields.push_str("    #[command(subcommand)]\n");
        fields.push_str(&format!("    pub command: {field_ty},\n"));
        gen_subcommand_enum(root, None, "Commands", groups, &mut ctx);
    }

    // Emit shared flatten-group structs after the whole tree has been
    // walked, so `ctx.used_groups` is complete — only groups actually
    // referenced somewhere get generated.
    for group in groups {
        if ctx.used_groups.contains(&group.id) {
            gen_flatten_group_struct(group, &mut ctx);
        }
    }

    let mut out = String::new();

    let mut clap_items = vec!["Parser"];
    if ctx.uses_subcommand {
        clap_items.push("Subcommand");
    }
    if ctx.uses_args {
        clap_items.push("Args");
    }
    if ctx.uses_value_enum {
        clap_items.push("ValueEnum");
    }
    out.push_str(&format!("use clap::{{{}}};\n", clap_items.join(", ")));
    for imp in &ctx.imports {
        out.push_str(&format!("use {imp};\n"));
    }
    for extra in &project.extra_uses {
        let trimmed = extra.trim().trim_end_matches(';');
        if !trimmed.is_empty() {
            out.push_str(&format!("use {trimmed};\n"));
        }
    }
    out.push('\n');

    for line in root.about.lines() {
        out.push_str(&format!("/// {line}\n"));
    }
    out.push_str("#[derive(Parser, Debug)]\n");
    let mut cmd_parts = vec![format!("name = {:?}", root.name)];
    match &project.version {
        Some(v) if !v.trim().is_empty() => cmd_parts.push(format!("version = {:?}", v.trim())),
        Some(_) => cmd_parts.push("version".to_string()),
        None => {}
    }
    push_subcommand_interaction_settings(root, &mut cmd_parts);
    out.push_str(&format!("#[command({})]\n", cmd_parts.join(", ")));
    out.push_str("pub struct Cli {\n");
    out.push_str(&fields);
    out.push_str("}\n");

    for item in &ctx.items {
        out.push('\n');
        out.push_str(item);
    }

    out
}

/// scope is the PascalCase path from (but excluding) the root down to
/// `parent`, e.g. `None` at the root, `Some("Add")`, `Some("AddRemote")`.
fn gen_subcommand_enum(
    parent: &CommandNode,
    parent_scope: Option<&str>,
    enum_ident: &str,
    groups: &[FlattenGroup],
    ctx: &mut Ctx,
) {
    ctx.uses_subcommand = true;
    let mut body = String::new();
    for child in &parent.subcommands {
        let variant = child.display_ident();
        let child_scope = match parent_scope {
            None => variant.clone(),
            Some(p) => format!("{p}{variant}"),
        };

        for line in child.about.lines() {
            body.push_str(&format!("    /// {line}\n"));
        }
        body.push_str(&format!("    #[command(name = {:?})]\n", child.name));

        let is_leaf_empty =
            child.args.is_empty() && child.subcommands.is_empty() && child.flattens.is_empty();
        if is_leaf_empty {
            body.push_str(&format!("    {variant},\n"));
        } else {
            let struct_ident = format!("{child_scope}Args");
            body.push_str(&format!("    {variant}({struct_ident}),\n"));
            gen_args_struct(child, &child_scope, &struct_ident, groups, ctx);
        }
    }
    ctx.items.push(format!(
        "#[derive(Subcommand, Debug)]\npub enum {enum_ident} {{\n{body}}}\n"
    ));
}

fn gen_args_struct(
    node: &CommandNode,
    scope: &str,
    struct_ident: &str,
    groups: &[FlattenGroup],
    ctx: &mut Ctx,
) {
    ctx.uses_args = true;
    let mut fields = String::new();
    for arg in &node.args {
        fields.push_str(&gen_field(arg, scope, ctx));
    }
    fields.push_str(&gen_flatten_fields(&node.flattens, groups, ctx));
    if !node.subcommands.is_empty() {
        let enum_ident = format!("{scope}Commands");
        let field_ty = if node.require_subcommand {
            enum_ident.clone()
        } else {
            format!("Option<{enum_ident}>")
        };
        fields.push_str("    #[command(subcommand)]\n");
        fields.push_str(&format!("    pub command: {field_ty},\n"));
        gen_subcommand_enum(node, Some(scope), &enum_ident, groups, ctx);
    }

    let mut block = String::new();
    let mut cmd_parts = Vec::new();
    push_subcommand_interaction_settings(node, &mut cmd_parts);
    if !cmd_parts.is_empty() {
        block.push_str(&format!("#[command({})]\n", cmd_parts.join(", ")));
    }
    block.push_str("#[derive(Args, Debug)]\n");
    block.push_str(&format!("pub struct {struct_ident} {{\n{fields}}}\n"));
    ctx.items.push(block);
}

fn gen_value_enum(enum_ident: &str, choices: &[String], ctx: &mut Ctx) {
    ctx.uses_value_enum = true;
    let mut body = String::new();
    for choice in choices {
        if choice.trim().is_empty() {
            continue;
        }
        let variant = normalize_variant_ident(choice);
        let choice = choice.trim();
        // clap's default `ValueEnum` renaming is kebab-case of the variant
        // ident; only pin an explicit name when that default wouldn't
        // reproduce the user's exact choice string.
        if variant.to_kebab_case() != choice {
            body.push_str(&format!("    #[value(name = {:?})]\n", choice));
        }
        body.push_str(&format!("    {variant},\n"));
    }
    ctx.items.push(format!(
        "#[derive(ValueEnum, Clone, Debug)]\npub enum {enum_ident} {{\n{body}}}\n"
    ));
}

fn normalize_variant_ident(raw: &str) -> String {
    let mut s = raw.to_pascal_case();
    if s.is_empty() {
        s = "Variant".to_string();
    }
    if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s = format!("_{s}");
    }
    s
}

fn gen_field(arg: &ArgDef, scope: &str, ctx: &mut Ctx) -> String {
    let field_name = normalize_field_ident(&arg.name);
    let mut lines = String::new();
    for line in arg.help.lines() {
        lines.push_str(&format!("    /// {line}\n"));
    }

    // Plain (non-multiple) named bool fields are auto-detected by clap as
    // `ArgAction::SetTrue` flags and must stay a bare `bool` — no Option
    // wrapping, no required/default semantics apply.
    if arg.is_plain_bool_flag() {
        let mut parts = Vec::new();
        push_short_long(arg, &mut parts);
        if parts.is_empty() {
            parts.push("long".to_string());
        }
        lines.push_str(&format!("    #[arg({})]\n", parts.join(", ")));
        lines.push_str(&format!("    pub {field_name}: bool,\n"));
        return lines;
    }

    let base_ty = if arg.choices.iter().any(|c| !c.trim().is_empty()) {
        let enum_ident = format!("{scope}{}", field_name.to_pascal_case());
        gen_value_enum(&enum_ident, &arg.choices, ctx);
        enum_ident
    } else {
        if let Some(imp) = arg.ty.needs_import() {
            ctx.imports.insert(imp);
        }
        arg.ty.type_token()
    };

    let has_default = arg
        .default_value
        .as_ref()
        .map(|d| !d.trim().is_empty())
        .unwrap_or(false);

    let full_ty = if arg.multiple {
        format!("Vec<{base_ty}>")
    } else if !arg.required && !has_default {
        format!("Option<{base_ty}>")
    } else {
        base_ty
    };

    let mut parts = Vec::new();
    if matches!(arg.kind, ArgKind::Named) {
        push_short_long(arg, &mut parts);
        if parts.is_empty() {
            // A field with no attributes at all is treated as positional by
            // clap derive; force `long` so it stays a named option.
            parts.push("long".to_string());
        }
    }
    if let Some(dv) = &arg.default_value
        && !dv.trim().is_empty() {
            parts.push(format!("default_value = {:?}", dv));
        }
    if arg.multiple && arg.required {
        parts.push("required = true".to_string());
    }
    if !parts.is_empty() {
        lines.push_str(&format!("    #[arg({})]\n", parts.join(", ")));
    }
    lines.push_str(&format!("    pub {field_name}: {full_ty},\n"));
    lines
}

fn push_short_long(arg: &ArgDef, parts: &mut Vec<String>) {
    if arg.short {
        match arg.short_char {
            Some(c) => parts.push(format!("short = {:?}", c)),
            None => parts.push("short".to_string()),
        }
    }
    if arg.long {
        match &arg.long_name {
            Some(n) if !n.trim().is_empty() => parts.push(format!("long = {:?}", n.trim())),
            _ => parts.push("long".to_string()),
        }
    }
}

/// Runs `rustfmt` over generated source, falling back to the raw string if
/// rustfmt isn't available or fails.
pub fn format_source(src: &str) -> String {
    let child = Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(_) => return src.to_string(),
    };

    if let Some(mut stdin) = child.stdin.take()
        && stdin.write_all(src.as_bytes()).is_err() {
            return src.to_string();
        }

    match child.wait_with_output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
        _ => src.to_string(),
    }
}
