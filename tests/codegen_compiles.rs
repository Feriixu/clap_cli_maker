//! Builds a CLI structure that exercises every codegen feature, generates
//! its clap-derive source, and compiles + runs that source in a scratch
//! cargo project against the real `clap` crate. `Cli::command().debug_assert()`
//! is clap's own recommended way to validate a derived CLI structure (no
//! duplicate ids, no clashing short/long flags, no contradictory
//! required/default combinations, ...) without needing real argv.
//!
//! Requires network access (or a warm cargo registry cache) the first time
//! `clap` needs to be fetched for the scratch project.

use clap_cli_maker::codegen;
use clap_cli_maker::model::{ArgDef, CommandNode, FlattenGroup, FlattenRef, Project, RustType};
use std::process::Command;

fn build_comprehensive_project() -> Project {
    let mut project = Project::new("Kitchen Sink".to_string());
    project.root.name = "kitchen-sink".to_string();
    project.root.about = "Exercises every codegen feature.".to_string();
    project.version = Some("1.2.3".to_string());
    project
        .extra_uses
        .push("std::collections::HashMap".to_string());
    project.root.args_conflicts_with_subcommands = true;
    project.root.subcommand_negates_reqs = true;
    project.root.require_subcommand = false;

    // Root-level named args: plain bool flag, short-only with an explicit
    // char, and a short+long `Vec<T>` multiple.
    let mut verbose = ArgDef::new_named("verbose");
    verbose.ty = RustType::Bool;
    verbose.short = true;
    verbose.help = "Enable verbose output".to_string();
    project.root.args.push(verbose);

    let mut level = ArgDef::new_named("level");
    level.ty = RustType::U8;
    level.short = true;
    level.short_char = Some('L');
    level.long = false;
    level.default_value = Some("1".to_string());
    project.root.args.push(level);

    let mut tag = ArgDef::new_named("tag");
    tag.short = true;
    tag.multiple = true;
    project.root.args.push(tag);

    // Two shared flatten groups. "Common Args" is deliberately reused by
    // both the root command and the "config" subcommand, to exercise
    // dedup (it must only be generated once).
    let mut common = FlattenGroup::new("Common Args");
    let mut keygrip = ArgDef::new_named("keygrip");
    keygrip.required = false;
    common.args.push(keygrip);
    let mut bind_ip = ArgDef::new_named("bind-ip");
    bind_ip.ty = RustType::Custom("std::net::IpAddr".to_string());
    bind_ip.default_value = Some("127.0.0.1".to_string());
    common.args.push(bind_ip);
    let common_id = common.id;

    let mut generate = FlattenGroup::new("Generate Args");
    let mut format_arg = ArgDef::new_positional("format");
    format_arg.choices = vec!["csv".into(), "json".into(), "SCREAMING".into()];
    generate.args.push(format_arg);
    let mut amount = ArgDef::new_positional("amount");
    amount.ty = RustType::U32;
    amount.multiple = true;
    amount.required = true;
    generate.args.push(amount);
    let generate_id = generate.id;

    project.root.flattens.push(FlattenRef::new(common_id));
    let mut generate_ref = FlattenRef::new(generate_id);
    generate_ref.optional = true;
    project.root.flattens.push(generate_ref);

    project.flatten_groups.push(common);
    project.flatten_groups.push(generate);

    // "add" subcommand: positional PathBuf, optional f64, a required
    // value-enum, and a nested "add remote" subcommand with an ident
    // override.
    let mut add = CommandNode::new_sub("add");
    add.about = "Add a thing".to_string();
    let mut path_arg = ArgDef::new_positional("path");
    path_arg.ty = RustType::PathBuf;
    add.args.push(path_arg);

    let mut ratio = ArgDef::new_named("ratio");
    ratio.ty = RustType::F64;
    add.args.push(ratio);

    let mut output_format = ArgDef::new_named("output-format");
    output_format.choices = vec!["json".into(), "yaml".into(), "Toml".into()];
    output_format.required = true;
    add.args.push(output_format);

    let mut remote = CommandNode::new_sub("remote");
    remote.about = "Manage remotes".to_string();
    remote.ident_override = Some("RemoteCmd".to_string());
    let mut url = ArgDef::new_named("url");
    url.required = true;
    remote.args.push(url);
    add.subcommands.push(remote);

    project.root.subcommands.push(add);

    // "list": an empty leaf subcommand (unit variant, no wrapping struct).
    let mut list = CommandNode::new_sub("list");
    list.about = "List things".to_string();
    project.root.subcommands.push(list);

    // "config": no args of its own, just the reused "Common Args" flatten
    // group — exercises a wrapping struct that exists purely for a flatten.
    let mut config = CommandNode::new_sub("config");
    config.about = "Configure things".to_string();
    config.flattens.push(FlattenRef::new(common_id));
    project.root.subcommands.push(config);

    project
}

/// Removes the scratch project directory on drop, so a failing/panicking
/// assertion doesn't leave it behind on disk.
struct ScratchDir(std::path::PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generated_code_compiles_and_passes_clap_validation() {
    let project = build_comprehensive_project();
    let source = codegen::format_source(&codegen::generate_source(&project));

    let dir = ScratchDir(std::env::temp_dir().join(format!(
        "clap_cli_maker_codegen_check_{}",
        std::process::id()
    )));
    let dir = &dir.0;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir.join("src")).expect("create scratch project dir");

    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "codegen_check"
version = "0.0.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
"#,
    )
    .expect("write scratch Cargo.toml");

    let main_rs = format!(
        "{source}\n\nfn main() {{\n    use clap::CommandFactory;\n    Cli::command().debug_assert();\n}}\n"
    );
    std::fs::write(dir.join("src/main.rs"), &main_rs).expect("write scratch src/main.rs");

    let build_output = Command::new("cargo")
        .arg("build")
        .current_dir(dir)
        .output()
        .expect("failed to spawn `cargo build` for the scratch project");
    assert!(
        build_output.status.success(),
        "generated code failed to compile:\n--- stdout ---\n{}\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr),
        source,
    );

    let bin_path = dir.join("target/debug").join(if cfg!(windows) {
        "codegen_check.exe"
    } else {
        "codegen_check"
    });
    let run_output = Command::new(&bin_path)
        .output()
        .expect("failed to run the compiled scratch binary");
    assert!(
        run_output.status.success(),
        "clap's own debug_assert() rejected the generated CLI structure:\n--- stdout ---\n{}\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&run_output.stdout),
        String::from_utf8_lossy(&run_output.stderr),
        source,
    );

    let _ = std::fs::remove_dir_all(dir);
}
