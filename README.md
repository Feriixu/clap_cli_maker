# Clap CLI Maker

A desktop app (egui/eframe) for visually designing a [clap](https://docs.rs/clap) derive-style
CLI, complete with commands, subcommands, arguments, and shared arg groups, then generating the
corresponding Rust `struct`/`enum` definitions.

## Running

```sh
cargo run
```

## Workflow

1. **Pick or create a project.** On launch you get a list of saved projects. Create a new one by
   name, open an existing one, or use **Open projects folder** to reveal them in your file
   manager.
2. **Build the command tree.** The left panel shows the command tree (root + nested
   subcommands) and the project's shared arg groups. Select a command to edit it in the middle
   panel, or add subcommands/groups with the `+` buttons.
3. **Edit a command's properties**: binary/subcommand name, an optional Rust identifier override,
   an about string (becomes a `///` doc comment), whether a subcommand is required, and its
   arguments.
4. **Read the generated code** in the live, syntax-highlighted right panel. Copy it to the
   clipboard or save it straight to a `.rs` file.

Projects are saved as JSON under your OS data directory (via the [`dirs`](https://docs.rs/dirs)
crate), e.g. `~/.local/share/clap_cli_maker/projects` on Linux, `~/Library/Application
Support/clap_cli_maker/projects` on macOS, `%APPDATA%\clap_cli_maker\projects` on Windows.
Generated code saved from the editor goes to a `generated` subfolder next to it.

## Argument features

Each argument can be positional or a named option/flag, with:

- Any common scalar type (`String`, integers, floats, `bool`, `PathBuf`, `OsString`), or an
  arbitrary custom type (e.g. `std::net::IpAddr`) typed in directly.
- Required / optional / default value.
- `Vec<T>` for values that can be given multiple times.
- Short and/or long flag, with overridable names.
- An environment variable fallback (`#[arg(env = "...")]`). Codegen adds a reminder comment
  that this needs clap's `env` feature enabled.
- A fixed set of choices, which generates a `#[derive(ValueEnum)]` type. Variant names only get
  an explicit `#[value(name = "...")]` when the choice isn't already clap's kebab-case default.
- `conflicts_with` / `conflicts_with_all` against other arguments in the same list (a command's
  own args, or a shared group's args).

## Shared arg groups (`#[command(flatten)]`)

Define a reusable set of args once (in the left panel's "Shared arg groups" section) and embed it
into any command, optionally wrapped in `Option<T>`. The same group can be reused across multiple
commands. Its struct is only generated once, wherever it's actually referenced.

## Subcommand interaction settings

Per command: whether a subcommand is required, plus `args_conflicts_with_subcommands` and
`subcommand_negates_reqs`, useful for CLIs where top-level flags/flatten groups and a subcommand
are meant to be alternatives.

## Testing

```sh
cargo test
```

`tests/codegen_compiles.rs` builds a CLI structure exercising every codegen feature, drops the
generated code into a scratch cargo project with real `clap` as a dependency, compiles it, and
runs `Cli::command().debug_assert()`, clap's own way of validating that a derived CLI structure
has no duplicate ids, clashing flags, or contradictory required/default combinations. Requires
network access (or a warm cargo registry cache) the first time it needs to fetch `clap`.

## Project layout

- `src/model.rs`, the data model: `Project`, `CommandNode`, `ArgDef`, `FlattenGroup`/`FlattenRef`.
- `src/codegen.rs`, generates clap-derive Rust source from a `Project`, then runs it through
  `rustfmt`.
- `src/storage.rs`, project directory resolution and load/save/delete, via `dirs` + `opener`.
- `src/ui/picker.rs`, `src/ui/editor.rs`, the two app screens.
- `src/app.rs`, top-level `eframe::App` wiring the two screens together.
