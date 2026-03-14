use tree_sitter::Language;

/// Configuration for a specific programming language
pub struct LanguageConfig {
    /// The Tree-sitter Language object
    pub language: Language,
    
    /// The identifier string (e.g., "rust")
    pub name: &'static str,
    
    /// Tree-sitter query to extract semantic blocks
    pub query: &'static str,
}

impl LanguageConfig {
    pub fn get(extension: &str) -> Option<Self> {
        match extension {
            "rs" => Some(Self {
                language: tree_sitter_rust::LANGUAGE.into(),
                name: "rust",
                query: RUST_QUERY,
            }),
            "py" => Some(Self {
                language: tree_sitter_python::LANGUAGE.into(),
                name: "python",
                query: PYTHON_QUERY,
            }),
            "ts" | "tsx" => Some(Self {
                language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                name: "typescript",
                query: TYPESCRIPT_QUERY,
            }),
            "js" | "jsx" => Some(Self {
                language: tree_sitter_javascript::LANGUAGE.into(),
                name: "javascript",
                query: JAVASCRIPT_QUERY,
            }),
            "go" => Some(Self {
                language: tree_sitter_go::LANGUAGE.into(),
                name: "go",
                query: GO_QUERY,
            }),
            "java" => Some(Self {
                language: tree_sitter_java::LANGUAGE.into(),
                name: "java",
                query: JAVA_QUERY,
            }),
            _ => None,
        }
    }
}

// ── Language Queries ────────────────────────────────────────────────────────

const RUST_QUERY: &str = r#"
(function_item 
  name: (identifier) @name) @function

(macro_definition
  name: (identifier) @name) @function

(impl_item
  type: (type_identifier) @name) @impl

(trait_item
  name: (type_identifier) @name) @trait

(struct_item
  name: (type_identifier) @name) @struct

(enum_item
  name: (type_identifier) @name) @enum
"#;

const PYTHON_QUERY: &str = r#"
(function_definition
  name: (identifier) @name) @function

(class_definition
  name: (identifier) @name) @class
"#;

const TYPESCRIPT_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @function

(method_definition
  name: (property_identifier) @name) @method

(class_declaration
  name: (type_identifier) @name) @class

(interface_declaration
  name: (type_identifier) @name) @interface

(type_alias_declaration
  name: (type_identifier) @name) @type_alias
"#;

const JAVASCRIPT_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @function

(method_definition
  name: (property_identifier) @name) @method

(class_declaration
  name: (identifier) @name) @class
"#;

const GO_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @function

(method_declaration
  name: (field_identifier) @name) @method

(type_spec
  name: (type_identifier) @name
  type: (struct_type)) @struct

(type_spec
  name: (type_identifier) @name
  type: (interface_type)) @interface
"#;

const JAVA_QUERY: &str = r#"
(method_declaration
  name: (identifier) @name) @method

(class_declaration
  name: (identifier) @name) @class

(interface_declaration
  name: (identifier) @name) @interface
"#;
