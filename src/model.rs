use heck::ToPascalCase;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    /// Display name for this project in the picker. Independent from the
    /// generated binary name (`root.name`).
    pub name: String,
    /// Leave empty to have the generated `Cli` pull the version from
    /// `CARGO_PKG_VERSION` via a bare `#[command(version)]`.
    pub version: Option<String>,
    pub root: CommandNode,
    /// Extra `use` statements the user wants prepended to the generated
    /// file, e.g. for a custom type used as an argument's type.
    pub extra_uses: Vec<String>,
}

impl Project {
    pub fn new(name: String) -> Self {
        let bin_name = name.to_pascal_case().to_lowercase();
        let bin_name = if bin_name.is_empty() { "cli".to_string() } else { bin_name };
        Project {
            id: Uuid::new_v4(),
            root: CommandNode::new_root(&bin_name),
            name,
            version: Some("0.1.0".to_string()),
            extra_uses: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandNode {
    pub id: Uuid,
    /// The literal name clap will show for this command (binary name at the
    /// root, subcommand name everywhere else).
    pub name: String,
    /// Overrides the PascalCase identifier derived from `name` for the
    /// generated struct/enum/variant. Leave empty to derive it from `name`.
    pub ident_override: Option<String>,
    /// Becomes a `///` doc comment, which clap derive reads as the about text.
    pub about: String,
    pub require_subcommand: bool,
    pub args: Vec<ArgDef>,
    pub subcommands: Vec<CommandNode>,
}

impl CommandNode {
    pub fn new_root(name: &str) -> Self {
        CommandNode {
            id: Uuid::new_v4(),
            name: name.to_string(),
            ident_override: None,
            about: String::new(),
            require_subcommand: true,
            args: Vec::new(),
            subcommands: Vec::new(),
        }
    }

    pub fn new_sub(name: &str) -> Self {
        CommandNode {
            id: Uuid::new_v4(),
            name: name.to_string(),
            ident_override: None,
            about: String::new(),
            require_subcommand: true,
            args: Vec::new(),
            subcommands: Vec::new(),
        }
    }

    pub fn display_ident(&self) -> String {
        match &self.ident_override {
            Some(s) if !s.trim().is_empty() => s.to_pascal_case(),
            _ => self.name.to_pascal_case(),
        }
    }

    pub fn find_mut(&mut self, id: Uuid) -> Option<&mut CommandNode> {
        if self.id == id {
            return Some(self);
        }
        self.subcommands.iter_mut().find_map(|c| c.find_mut(id))
    }

    /// Removes the descendant with the given id, wherever it is in the tree.
    /// Returns true if something was removed. No-op if `id` is this node itself.
    pub fn remove_child(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.subcommands.iter().position(|c| c.id == id) {
            self.subcommands.remove(pos);
            return true;
        }
        self.subcommands.iter_mut().any(|c| c.remove_child(id))
    }

    /// Moves the descendant with the given id up (`delta < 0`) or down
    /// (`delta > 0`) among its siblings.
    pub fn move_child(&mut self, id: Uuid, delta: i32) -> bool {
        if let Some(pos) = self.subcommands.iter().position(|c| c.id == id) {
            let new_pos = pos as i32 + delta;
            if new_pos >= 0 && (new_pos as usize) < self.subcommands.len() {
                self.subcommands.swap(pos, new_pos as usize);
            }
            return true;
        }
        self.subcommands.iter_mut().any(|c| c.move_child(id, delta))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgKind {
    Positional,
    Named,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RustType {
    String,
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
    F32,
    F64,
    PathBuf,
    OsString,
    Custom(String),
}

impl RustType {
    pub fn basic_types() -> Vec<RustType> {
        vec![
            RustType::String,
            RustType::Bool,
            RustType::I8,
            RustType::I16,
            RustType::I32,
            RustType::I64,
            RustType::I128,
            RustType::Isize,
            RustType::U8,
            RustType::U16,
            RustType::U32,
            RustType::U64,
            RustType::U128,
            RustType::Usize,
            RustType::F32,
            RustType::F64,
            RustType::PathBuf,
            RustType::OsString,
        ]
    }

    pub fn label(&self) -> String {
        match self {
            RustType::String => "String".to_string(),
            RustType::Bool => "bool".to_string(),
            RustType::I8 => "i8".to_string(),
            RustType::I16 => "i16".to_string(),
            RustType::I32 => "i32".to_string(),
            RustType::I64 => "i64".to_string(),
            RustType::I128 => "i128".to_string(),
            RustType::Isize => "isize".to_string(),
            RustType::U8 => "u8".to_string(),
            RustType::U16 => "u16".to_string(),
            RustType::U32 => "u32".to_string(),
            RustType::U64 => "u64".to_string(),
            RustType::U128 => "u128".to_string(),
            RustType::Usize => "usize".to_string(),
            RustType::F32 => "f32".to_string(),
            RustType::F64 => "f64".to_string(),
            RustType::PathBuf => "PathBuf".to_string(),
            RustType::OsString => "OsString".to_string(),
            RustType::Custom(_) => "Custom...".to_string(),
        }
    }

    /// The Rust type token used in generated code. Falls back to `String`
    /// for an empty custom type so codegen never emits an invalid field.
    pub fn type_token(&self) -> String {
        match self {
            RustType::Custom(s) if s.trim().is_empty() => "String".to_string(),
            RustType::Custom(s) => s.trim().to_string(),
            other => other.label(),
        }
    }

    pub fn needs_import(&self) -> Option<&'static str> {
        match self {
            RustType::PathBuf => Some("std::path::PathBuf"),
            RustType::OsString => Some("std::ffi::OsString"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgDef {
    pub id: Uuid,
    /// The struct field name. Normalized to snake_case at codegen time.
    pub name: String,
    pub kind: ArgKind,
    pub ty: RustType,
    /// Becomes a `///` doc comment, which clap derive reads as the arg's help text.
    pub help: String,
    pub required: bool,
    /// Generates a `Vec<T>` field that can be given multiple times.
    pub multiple: bool,
    pub default_value: Option<String>,
    pub short: bool,
    /// Explicit short letter. `None` while `short` is true means clap derives
    /// it from the first letter of the field name (bare `#[arg(short)]`).
    pub short_char: Option<char>,
    pub long: bool,
    /// Explicit long name override. `None` while `long` is true means clap
    /// derives it (kebab-cased) from the field name (bare `#[arg(long)]`).
    pub long_name: Option<String>,
    /// Non-empty enables a generated `#[derive(ValueEnum)]` type for this
    /// arg, one variant per choice, instead of using `ty`.
    pub choices: Vec<String>,
}

impl ArgDef {
    pub fn new_named(name: &str) -> Self {
        ArgDef {
            id: Uuid::new_v4(),
            name: name.to_string(),
            kind: ArgKind::Named,
            ty: RustType::String,
            help: String::new(),
            required: false,
            multiple: false,
            default_value: None,
            short: false,
            short_char: None,
            long: true,
            long_name: None,
            choices: Vec::new(),
        }
    }

    pub fn new_positional(name: &str) -> Self {
        ArgDef {
            id: Uuid::new_v4(),
            name: name.to_string(),
            kind: ArgKind::Positional,
            ty: RustType::String,
            help: String::new(),
            required: true,
            multiple: false,
            default_value: None,
            short: false,
            short_char: None,
            long: false,
            long_name: None,
            choices: Vec::new(),
        }
    }

    pub fn is_plain_bool_flag(&self) -> bool {
        matches!(self.kind, ArgKind::Named) && matches!(self.ty, RustType::Bool) && !self.multiple
    }
}
