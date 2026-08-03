# Clap CLI Maker

A desktop app (egui/eframe) for visual design of a [clap](https://docs.rs/clap) derive-style
CLI. It builds commands, subcommands, arguments, and shared arg groups, then generates the
matching Rust `struct`/`enum` definitions.

## Running

```sh
cargo run
```

## Workflow

1. **Pick or create a project.** On launch, the app shows a list of saved projects. Create a
   new one by name, open an existing one, or use **Open projects folder** to open it in your
   file manager.
2. **Build the command tree.** The left panel shows the command tree (root and nested
   subcommands) and the project's shared arg groups. Select a command to edit it in the middle
   panel. Add subcommands or groups with the `+` buttons.
3. **Edit a command's properties.** Set the binary or subcommand name, an optional Rust
   identifier override, an about string (becomes a `///` doc comment), whether a subcommand is
   required, and its arguments.
4. **Read the generated code** in the live, syntax-highlighted right panel. Copy it to the
   clipboard, or save it directly to a `.rs` file.

The app saves projects as JSON under your OS data directory (via the [`dirs`](https://docs.rs/dirs)
crate). Examples: `~/.local/share/clap_cli_maker/projects` on Linux, `~/Library/Application
Support/clap_cli_maker/projects` on macOS, `%APPDATA%\clap_cli_maker\projects` on Windows.
Generated code saved from the editor goes to a `generated` subfolder next to the project.

## Argument features

Each argument is positional or a named option/flag, with:

- Any common scalar type (`String`, integers, floats, `bool`, `PathBuf`, `OsString`), or a
  custom type (e.g. `std::net::IpAddr`) typed in directly.
- Required, optional, or default value.
- `Vec<T>` for arguments you can repeat on the command line.
- A short flag, a long flag, or both, with overridable names.
- An environment variable fallback (`#[arg(env = "...")]`). Codegen adds a reminder comment
  that this needs clap's `env` feature enabled.
- A fixed set of choices, which generates a `#[derive(ValueEnum)]` type. A variant gets an
  explicit `#[value(name = "...")]` only when the choice name does not match clap's kebab-case
  default.
- `conflicts_with` / `conflicts_with_all` against other arguments in the same list (a command's
  own args, or a shared group's args).

## Shared arg groups (`#[command(flatten)]`)

Define a reusable set of args once, in the left panel's "Shared arg groups" section. Embed it
into any command, optionally wrapped in `Option<T>`. Reuse the same group across multiple
commands. The app generates its struct once, at the first place it is referenced.

## Subcommand interaction settings

Per command, set whether a subcommand is required, plus `args_conflicts_with_subcommands` and
`subcommand_negates_reqs`. Use these for CLIs where a subcommand should replace the top-level
flags and flatten groups, not combine with them.

## Testing

```sh
cargo test
```

`tests/codegen_compiles.rs` builds a CLI structure that exercises every codegen feature. It
drops the generated code into a scratch cargo project with real `clap` as a dependency, then
compiles it. It then runs `Cli::command().debug_assert()`, clap's own check for a derived CLI
structure. This check catches duplicate ids, clashing flags, and contradictory required/default
combinations. The first run needs network access, or a warm cargo registry cache, to fetch
`clap`.

## Project layout

- `src/model.rs`: the data model. `Project`, `CommandNode`, `ArgDef`, `FlattenGroup`/`FlattenRef`.
- `src/codegen.rs`: generates clap-derive Rust source from a `Project`, then runs it through
  `rustfmt`.
- `src/storage.rs`: project directory resolution and load/save/delete, via `dirs` + `opener`.
- `src/ui/picker.rs`, `src/ui/editor.rs`: the two app screens.
- `src/app.rs`: top-level `eframe::App` that wires the two screens together.
