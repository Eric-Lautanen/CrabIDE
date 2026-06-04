//! Embedded tree-sitter highlight queries for all built-in languages.
//!
//! Each query uses the standard tree-sitter capture naming convention
//! (`@keyword`, `@string`, `@comment`, etc.) compatible with Helix-style
//! scope names. The [`highlights_query_for`] function returns the right
//! query source for a given language ID.
//!
//! NOTE: These are draft queries. If a grammar's node type names differ from
//! what's written here, the highlight engine will log a warning and fall back
//! to no highlighting for that language rather than panicking.

use crabide_core::types::Language;

// ── Rust ──────────────────────────────────────────────────────────────────────

pub const RUST_HIGHLIGHTS: &str = r#"
(line_comment) @comment
(block_comment) @comment

(string_literal) @string
(raw_string_literal) @string
(char_literal) @string.special

(integer_literal) @number
(float_literal) @number.float
(boolean_literal) @boolean

; In tree-sitter-rust these are named nodes, not anonymous keyword tokens.
(crate) @keyword
(mutable_specifier) @keyword
(super) @keyword

(type_identifier) @type
(primitive_type) @type.builtin
(field_identifier) @variable.member
(shorthand_field_identifier) @variable.member

(macro_invocation macro: [(identifier) (scoped_identifier)] @function.macro)

(function_item name: (identifier) @function)
(function_signature_item name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (field_expression field: (field_identifier) @function.call))
(call_expression function: (scoped_identifier name: (identifier) @function.call))

(attribute_item) @attribute
(inner_attribute_item) @attribute

(self) @variable.builtin

[
  "as" "async" "await" "break" "const" "continue" "dyn" "else" "enum"
  "extern" "fn" "for" "if" "impl" "in" "let" "loop" "match" "mod" "move"
  "pub" "ref" "return" "static" "struct" "trait" "type" "union"
  "unsafe" "use" "where" "while" "yield"
] @keyword

(lifetime) @label

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&" "||"
  "!" "&" "|" "^" "<<" ">>" "+=" "-=" "*=" "/=" "%=" "&=" "|="
  "^=" "<<=" ">>=" "->" "=>" "?" "@" "_"
] @operator

["(" ")" "{" "}" "[" "]" "|"] @punctuation.bracket
["," ";" ":" "::" "." ".."] @punctuation.delimiter

(escape_sequence) @string.escape
"#;

// ── Python ────────────────────────────────────────────────────────────────────

pub const PYTHON_HIGHLIGHTS: &str = r#"
(comment) @comment
(string) @string
(interpolation) @string.special

(integer) @number
(float) @number.float

(true) @boolean
(false) @boolean
(none) @constant.builtin

(function_definition name: (identifier) @function)
(call function: (identifier) @function.call)
(call function: (attribute attribute: (identifier) @function.call))
(class_definition name: (identifier) @type)

(decorator) @attribute

(identifier) @variable

[
  "def" "class" "return" "if" "elif" "else" "while" "for" "in" "import"
  "from" "as" "pass" "break" "continue" "raise" "try" "except" "finally"
  "with" "yield" "lambda" "and" "or" "not" "is" "async" "await" "del"
  "global" "nonlocal" "assert" "match" "case"
] @keyword

[
  "=" "==" "!=" "<" "<=" ">" ">=" "+" "-" "*" "/" "//" "%" "**"
  "&" "|" "^" "~" "<<" ">>" "+=" "-=" "*=" "/=" "%="
  "->" ":=" "@"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ":" ";"] @punctuation.delimiter
"#;

// ── JavaScript ────────────────────────────────────────────────────────────────

pub const JAVASCRIPT_HIGHLIGHTS: &str = r#"
(comment) @comment
(string) @string
(template_string) @string
(regex) @string.special

(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(undefined) @constant.builtin

(function_declaration name: (identifier) @function)
(function_expression name: (identifier) @function)
(arrow_function) @function
(method_definition name: (property_identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (member_expression property: (property_identifier) @function.call))

(class_declaration name: (identifier) @type)

(identifier) @variable
(this) @variable.builtin
(super) @variable.builtin

(property_identifier) @variable.member

[
  "function" "return" "var" "let" "const" "if" "else" "while" "for"
  "in" "of" "break" "continue" "switch" "case" "default" "throw"
  "try" "catch" "finally" "new" "delete" "typeof" "instanceof"
  "import" "export" "from" "class" "extends" "async" "await"
  "yield" "void" "do" "debugger" "with"
] @keyword

[
  "=" "==" "===" "!=" "!==" "<" "<=" ">" ">=" "&&" "||" "!" "??"
  "+" "-" "*" "/" "%" "**" "&" "|" "^" "~" "<<" ">>" ">>>"
  "++" "--" "+=" "-=" "*=" "/=" "=>" "..."
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
[";" "," "." ":"] @punctuation.delimiter
"#;

// ── TypeScript ────────────────────────────────────────────────────────────────

pub const TYPESCRIPT_HIGHLIGHTS: &str = r#"
(comment) @comment
(string) @string
(template_string) @string
(regex) @string.special

(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(undefined) @constant.builtin

(function_declaration name: (identifier) @function)
(function_expression name: (identifier) @function)
(arrow_function) @function
(method_definition name: (property_identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (member_expression property: (property_identifier) @function.call))

(class_declaration name: (type_identifier) @type)
(type_identifier) @type

(identifier) @variable
(this) @variable.builtin
(super) @variable.builtin
(property_identifier) @variable.member

[
  "function" "return" "var" "let" "const" "if" "else" "while" "for"
  "in" "of" "break" "continue" "switch" "case" "default" "throw"
  "try" "catch" "finally" "new" "delete" "typeof" "instanceof"
  "import" "export" "from" "class" "extends" "async" "await"
  "yield" "void" "do" "debugger"
  "type" "interface" "enum" "implements" "declare" "abstract"
  "readonly" "namespace" "module" "as" "satisfies"
] @keyword

[
  "=" "==" "===" "!=" "!==" "<" "<=" ">" ">=" "&&" "||" "!" "??"
  "+" "-" "*" "/" "%" "**" "&" "|" "^" "~" "<<" ">>" ">>>"
  "++" "--" "+=" "-=" "*=" "/=" "=>" "..."
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
[";" "," "." ":" "?"] @punctuation.delimiter
"#;

// ── Go ────────────────────────────────────────────────────────────────────────

pub const GO_HIGHLIGHTS: &str = r#"
(comment) @comment

(interpreted_string_literal) @string
(raw_string_literal) @string
(rune_literal) @string.special

(int_literal) @number
(float_literal) @number.float
(imaginary_literal) @number

(true) @boolean
(false) @boolean
(nil) @constant.builtin

(function_declaration name: (identifier) @function)
(method_declaration name: (field_identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (selector_expression field: (field_identifier) @function.call))

(type_spec name: (type_identifier) @type)
(type_identifier) @type

(identifier) @variable
(field_identifier) @variable.member
(package_identifier) @namespace

[
  "break" "case" "chan" "const" "continue" "default" "defer" "else"
  "fallthrough" "for" "func" "go" "goto" "if" "import" "interface"
  "map" "package" "range" "return" "select" "struct" "switch" "type" "var"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&" "||"
  "!" "&" "|" "^" "<<" ">>" "<-" "++" "--" ":=" "+=" "-=" "*=" "/="
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
"#;

// ── C ─────────────────────────────────────────────────────────────────────────

pub const C_HIGHLIGHTS: &str = r#"
(comment) @comment

(string_literal) @string
(char_literal) @string.special
(concatenated_string) @string

(number_literal) @number

(true) @boolean
(false) @boolean
"NULL" @constant.builtin
"nullptr" @constant.builtin

(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function))
(call_expression function: (identifier) @function.call)

(type_identifier) @type
(primitive_type) @type.builtin
(sized_type_specifier) @type.builtin

(preproc_include) @keyword
(preproc_def name: (identifier) @constant)
(preproc_ifdef) @keyword
(preproc_if) @keyword
(preproc_call directive: _ @keyword)

[
  "auto" "break" "case" "const" "continue" "default" "do" "else" "enum"
  "extern" "for" "goto" "if" "inline" "register" "restrict" "return"
  "sizeof" "static" "struct" "switch" "typedef" "union" "unsigned"
  "signed" "volatile" "while"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&" "||"
  "!" "&" "|" "^" "~" "<<" ">>" "+=" "-=" "*=" "/=" "->"
  "++" "--"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
"#;

// ── C++ ───────────────────────────────────────────────────────────────────────

pub const CPP_HIGHLIGHTS: &str = r#"
(comment) @comment

(string_literal) @string
(char_literal) @string.special
(raw_string_literal) @string

(number_literal) @number

["true" "false"] @boolean
["nullptr" "NULL"] @constant.builtin

(function_definition
  declarator: (function_declarator
    declarator: [(identifier) (qualified_identifier)] @function))
(call_expression function: [(identifier) (qualified_identifier)] @function.call)

(type_identifier) @type
(primitive_type) @type.builtin
(namespace_identifier) @namespace

(preproc_include) @keyword
(preproc_def name: (identifier) @constant)

[
  "auto" "break" "case" "catch" "class" "const" "constexpr" "consteval"
  "constinit" "continue" "co_await" "co_return" "co_yield" "default"
  "delete" "do" "else" "enum" "explicit" "extern" "final" "for" "friend"
  "goto" "if" "inline" "mutable" "namespace" "new" "noexcept" "operator"
  "override" "private" "protected" "public" "register" "return" "sizeof"
  "static" "static_assert" "struct" "switch" "template" "this" "thread_local"
  "throw" "try" "typedef" "union" "using" "virtual" "void" "volatile" "while"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&" "||"
  "!" "&" "|" "^" "~" "<<" ">>" "+=" "-=" "*=" "/=" "->" "::"
  "++" "--"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":" "..."] @punctuation.delimiter
"#;

// ── JSON ──────────────────────────────────────────────────────────────────────

pub const JSON_HIGHLIGHTS: &str = r#"
(string) @string
(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin

(pair key: (string) @variable.member)

["{" "}" "[" "]"] @punctuation.bracket
["," ":"] @punctuation.delimiter
"#;

// ── TOML ──────────────────────────────────────────────────────────────────────

pub const TOML_HIGHLIGHTS: &str = r#"
(comment) @comment

(string) @string
(integer) @number
(float) @number.float
(offset_date_time) @string.special
(local_date_time) @string.special
(local_date) @string.special
(local_time) @string.special

["true" "false"] @boolean

(bare_key) @variable.member
(quoted_key) @variable.member

(table (["[" "]"])) @punctuation.bracket
(array_table (["[[" "]]"])) @punctuation.bracket
["[" "]"] @punctuation.bracket
["," "."] @punctuation.delimiter
["="] @operator
"#;

// ── Markdown ──────────────────────────────────────────────────────────────────

pub const MARKDOWN_HIGHLIGHTS: &str = r#"
(atx_heading) @markup.heading
(setext_heading) @markup.heading

(strong_emphasis) @markup.bold
(emphasis) @markup.italic
(strikethrough) @markup.strikethrough

(code_span) @markup.raw.inline
(fenced_code_block) @markup.raw.block
(indented_code_block) @markup.raw.block

(link_text) @markup.link.text
(link_destination) @markup.link.url
(image) @markup.link.image

(block_quote) @markup.quote

(list_item
  marker: _ @markup.list)

(thematic_break) @punctuation.special
(html_block) @markup.raw.block
"#;

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Return the embedded highlight query source for a given language, or `""`
/// if the language has no bundled query.
pub fn highlights_query_for(lang: &Language) -> &'static str {
    match lang.as_str() {
        "rust" => RUST_HIGHLIGHTS,
        "python" => PYTHON_HIGHLIGHTS,
        "javascript" => JAVASCRIPT_HIGHLIGHTS,
        "typescript" => TYPESCRIPT_HIGHLIGHTS,
        "go" => GO_HIGHLIGHTS,
        "c" => C_HIGHLIGHTS,
        "cpp" => CPP_HIGHLIGHTS,
        "json" => JSON_HIGHLIGHTS,
        "toml" => TOML_HIGHLIGHTS,
        "markdown" => MARKDOWN_HIGHLIGHTS,
        _ => "",
    }
}
