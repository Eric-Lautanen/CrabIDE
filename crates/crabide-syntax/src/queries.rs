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

// ── HTML ───────────────────────────────────────────────────────────────────────

pub const HTML_HIGHLIGHTS: &str = r#"
(comment) @comment

(doctype) @keyword

(text) @none

(element
  (start_tag (tag_name) @tag)
  (self_closing_tag (tag_name) @tag)
  (end_tag (tag_name) @tag))

(attribute
  (attribute_name) @variable.member
  (quoted_attribute_value (attribute_value) @string))

(script_element
  (raw_text) @none)

(style_element
  (raw_text) @none)

[
  "="
] @operator
"#;

// ── CSS / SCSS / LESS ─────────────────────────────────────────────────────────

pub const CSS_HIGHLIGHTS: &str = r#"
(comment) @comment

(property_name) @variable.member
(property_value) @string

(integer_value) @number
(float_value) @number.float

(unit) @keyword

(color_value) @string.special

(string_value) @string

(class_selector (class_name) @type)
(id_selector (id_name) @type.special
  (#match? @type.special "^[a-zA-Z]"))
(type_selector (tag_name) @tag)
(universal_selector) @operator

(pseudo_class_selector (pseudo_class_name) @function)
(pseudo_element_selector (pseudo_element_name) @function)

(function_name) @function.call

[
  "@import" "@media" "@keyframes" "@font-face" "@supports"
  "@namespace" "@page" "@layer" "@container" "@scope"
] @keyword

[
  "!" "important"
] @keyword

[
  "{" "}" "(" ")" "[" "]"
] @punctuation.bracket

[
  "," "." ";"
] @punctuation.delimiter

[
  ":" ":"
] @operator
"#;

pub const SCSS_HIGHLIGHTS: &str = r#"
(comment) @comment

(property_name) @variable.member
(property_value) @string

(integer_value) @number
(float_value) @number.float

(unit) @keyword

(color_value) @string.special

(string_value) @string

(class_selector (class_name) @type)
(id_selector (id_name) @type.special)
(type_selector (tag_name) @tag)
(universal_selector) @operator

(pseudo_class_selector (pseudo_class_name) @function)
(pseudo_element_selector (pseudo_element_name) @function)

(function_name) @function.call

(variable_name) @variable

(include_statement) @keyword
(mixin_statement name: (identifier) @function)
(function_declaration name: (identifier) @function)

[
  "@import" "@media" "@keyframes" "@font-face" "@supports"
  "@namespace" "@page" "@layer" "@container" "@scope"
  "@mixin" "@include" "@extend" "@at-root" "@use" "@forward"
] @keyword

[
  "!" "important" "default"
] @keyword

[
  "{" "}" "(" ")" "[" "]"
] @punctuation.bracket

[
  "," "." ";"
] @punctuation.delimiter

[
  ":" ":"
] @operator

(interpolation) @string.special
"#;

pub const LESS_HIGHLIGHTS: &str = r#"
(comment) @comment

(property_name) @variable.member
(property_value) @string

(integer_value) @number
(float_value) @number.float

(unit) @keyword

(color_value) @string.special

(string_value) @string

(class_selector (class_name) @type)
(id_selector (id_name) @type.special)
(type_selector (tag_name) @tag)
(universal_selector) @operator

(pseudo_class_selector (pseudo_class_name) @function)
(pseudo_element_selector (pseudo_element_name) @function)

(function_name) @function.call

(variable_name) @variable

(mixin_call) @function.call
(mixin_definition name: (identifier) @function)

[
  "@import" "@media" "@keyframes"
] @keyword

[
  "{" "}" "(" ")" "[" "]"
] @punctuation.bracket

[
  "," "." ";"
] @punctuation.delimiter

[
  ":" ":"
] @operator
"#;

// ── YAML ──────────────────────────────────────────────────────────────────────

pub const YAML_HIGHLIGHTS: &str = r#"
(comment) @comment

(anchor) @label
(alias) @label

(block_mapping_pair key: (flow_node (plain_scalar) @variable.member))
(block_mapping_pair key: (block_mapping_pair key: (flow_node (plain_scalar) @variable.member))

(double_quote_scalar) @string
(single_quote_scalar) @string
(block_scalar) @string
(string_scalar) @string

(integer_scalar) @number
(float_scalar) @number.float
(boolean_scalar) @boolean
(null_scalar) @constant.builtin

[
  "true" "false" "yes" "no" "on" "off"
] @boolean

[
  "null" "~"
] @constant.builtin

[
  "---" "..."
] @punctuation.special

[
  "-" ":"
] @punctuation.delimiter

[
  "&" "*" ">"
] @operator
"#;

// ── Shell/Bash ─────────────────────────────────────────────────────────────────

pub const BASH_HIGHLIGHTS: &str = r#"
(comment) @comment

(string) @string
(string_expansion) @string.special

(command_name) @function.call
(command (command_name) @function.call)

(file_descriptor) @number

(function_definition name: (word) @function)

(variable_name) @variable

(expansion) @string.special

(arithmetic_expansion) @string.special

(process_substitution) @string.special

[
  "if" "then" "else" "elif" "fi" "case" "esac" "for" "while"
  "until" "do" "done" "in" "select" "function" "time" "declare"
  "local" "export" "readonly" "typeset" "unset" "set" "shopt"
] @keyword

[
  "&&" "||" "!" "|" "&" ";"
] @operator

[
  "(" ")" "{" "}" "[" "]"
] @punctuation.bracket

["," "."] @punctuation.delimiter

(redirect) @operator

(heredoc_start) @keyword
(heredoc_body) @string
(heredoc_end) @keyword

(declaration_command name: (word) @function)
(test_command) @function.call
"#;

// ── SQL ────────────────────────────────────────────────────────────────────────

pub const SQL_HIGHLIGHTS: &str = r#"
(comment) @comment
((comment) @comment
  (#match? @comment "^--"))

(string) @string
(escape_sequence) @string.escape

(number_literal) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin

(column_definition name: (identifier) @variable.member)
(table name: (identifier) @type)
(view name: (identifier) @type)
(function name: (identifier) @function)
(procedure name: (identifier) @function)
(trigger name: (identifier) @function)

(call_expression function: (identifier) @function.call)

[
  "SELECT" "FROM" "WHERE" "AND" "OR" "NOT" "IN" "IS" "NULL"
  "LIKE" "BETWEEN" "EXISTS" "ALL" "ANY" "SOME"
  "INSERT" "INTO" "VALUES" "UPDATE" "SET" "DELETE"
  "CREATE" "TABLE" "DROP" "ALTER" "INDEX" "VIEW" "TRIGGER"
  "PROCEDURE" "FUNCTION" "BEGIN" "END" "DECLARE"
  "IF" "ELSE" "THEN" "WHILE" "LOOP" "CASE" "WHEN"
  "JOIN" "LEFT" "RIGHT" "INNER" "OUTER" "CROSS" "ON"
  "ORDER" "BY" "GROUP" "HAVING" "LIMIT" "OFFSET"
  "UNION" "INTERSECT" "EXCEPT" "AS" "DISTINCT" "TOP"
  "PRIMARY" "KEY" "FOREIGN" "REFERENCES" "CONSTRAINT"
  "DEFAULT" "CHECK" "UNIQUE" "CASCADE" "RESTRICT"
  "ASC" "DESC" "NULLS" "FIRST" "LAST"
  "GRANT" "REVOKE" "COMMIT" "ROLLBACK" "SAVEPOINT"
] @keyword

[
  "=" "==" "!=" "<>" "<" "<=" ">" ">="
  "+" "-" "*" "/" "%"
  "||"
] @operator

["(" ")" "[" "]"] @punctuation.bracket
["," "." ";"] @punctuation.delimiter
"#;

// ── Java ───────────────────────────────────────────────────────────────────────

pub const JAVA_HIGHLIGHTS: &str = r#"
(line_comment) @comment
(block_comment) @comment

(string_literal) @string
(char_literal) @string.special

(integer_literal) @number
(float_literal) @number.float
(boolean_literal) @boolean
(null_literal) @constant.builtin

(method_declaration name: (identifier) @function)
(method_invocation name: (identifier) @function.call)

(class_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(record_declaration name: (identifier) @type)
(annotation_type_declaration name: (identifier) @type)

(type_identifier) @type

(identifier) @variable

(this) @variable.builtin
(super) @variable.builtin

(annotation) @attribute

[
  "abstract" "assert" "boolean" "break" "byte" "case" "catch"
  "char" "class" "const" "continue" "default" "do" "double"
  "else" "enum" "extends" "final" "finally" "float" "for"
  "goto" "if" "implements" "import" "instanceof" "int"
  "interface" "long" "native" "new" "package" "private"
  "protected" "public" "return" "short" "static" "strictfp"
  "super" "switch" "synchronized" "this" "throw" "throws"
  "transient" "try" "void" "volatile" "while" "module"
  "requires" "exports" "opens" "uses" "provides" "to" "with"
  "record" "sealed" "permits" "yield" "var"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&"
  "||" "!" "&" "|" "^" "~" "<<" ">>" ">>>" "+=" "-=" "*="
  "/=" "%=" "&=" "|=" "^=" "<<=" ">>=" ">>>=" "->" "::"
  "instanceof"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
"#;

// ── C# ─────────────────────────────────────────────────────────────────────────

pub const CSHARP_HIGHLIGHTS: &str = r#"
(comment) @comment

(string_literal) @string
(char_literal) @string.special
(verbatim_string_literal) @string
(raw_string_literal) @string
(interpolation) @string.special

(integer_literal) @number
(real_literal) @number.float
(boolean_literal) @boolean
(null_literal) @constant.builtin

(method_declaration name: (identifier) @function)
(invocation_expression function: (identifier) @function.call)

(class_declaration name: (identifier) @type)
(struct_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(record_declaration name: (identifier) @type)

(type_identifier) @type

(identifier) @variable

(this_expression) @variable.builtin
(base_expression) @variable.builtin

(attribute) @attribute

(namespace_declaration name: (identifier) @namespace)
(using_directive (identifier) @namespace)

[
  "abstract" "as" "async" "await" "base" "bool" "break" "byte"
  "case" "catch" "char" "checked" "class" "const" "continue"
  "decimal" "default" "delegate" "do" "double" "else" "enum"
  "event" "explicit" "extern" "false" "finally" "fixed" "float"
  "for" "foreach" "goto" "if" "implicit" "in" "int" "interface"
  "internal" "is" "lock" "long" "namespace" "new" "null"
  "object" "operator" "out" "override" "params" "private"
  "protected" "public" "readonly" "record" "ref" "return"
  "sbyte" "sealed" "short" "sizeof" "stackalloc" "static"
  "string" "struct" "switch" "this" "throw" "true" "try"
  "typeof" "uint" "ulong" "unchecked" "unsafe" "ushort"
  "using" "virtual" "void" "volatile" "while"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&"
  "||" "!" "&" "|" "^" "~" "<<" ">>" "+=" "-=" "*=" "/="
  "%=" "&=" "|=" "^=" "<<=" ">>=" "->" "::" "??" "?."
  "=>"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
"#;

// ── Kotlin ─────────────────────────────────────────────────────────────────────

pub const KOTLIN_HIGHLIGHTS: &str = r#"
(comment) @comment

(string_template) @string
(string_literal) @string
(char_literal) @string.special
(interpolation) @string.special

(integer_literal) @number
(float_literal) @number.float
(boolean_literal) @boolean
(null_literal) @constant.builtin

(function_declaration name: (simple_identifier) @function)
(call_expression function: (simple_identifier) @function.call)
(call_expression function: (navigation_expression (simple_identifier) @function.call))

(class_declaration name: (simple_identifier) @type)
(object_declaration name: (simple_identifier) @type)
(type_identifier) @type

(simple_identifier) @variable

(this_expression) @variable.builtin
(super_expression) @variable.builtin

(annotation) @attribute

[
  "abstract" "actual" "annotation" "as" "break" "by" "catch"
  "class" "companion" "const" "constructor" "continue" "crossinline"
  "data" "delegate" "do" "dynamic" "else" "enum" "expect" "external"
  "field" "file" "final" "finally" "for" "fun" "get" "if" "import"
  "in" "infix" "init" "inline" "inner" "interface" "internal"
  "is" "it" "lateinit" "noinline" "object" "open" "operator"
  "out" "override" "package" "param" "private" "property" "protected"
  "public" "receiver" "reified" "return" "sealed" "set" "setparam"
  "super" "suspend" "tailrec" "this" "throw" "try" "typealias"
  "typeof" "val" "var" "vararg" "when" "where" "while"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" "<=" ">" ">=" "&&"
  "||" "!" "&" "|" "^" "~" "<<" ">>" "+=" "-=" "*=" "/="
  "%=" ".." "..." "->" "::" "?:"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
"#;

// ── Ruby ───────────────────────────────────────────────────────────────────────

pub const RUBY_HIGHLIGHTS: &str = r#"
(comment) @comment

(string) @string
(interpolation) @string.special
(heredoc_body) @string
(regex) @string.special

(integer) @number
(float) @number.float

(true) @boolean
(false) @boolean
(nil) @constant.builtin

(method name: (identifier) @function)
(call method: (identifier) @function.call)

(class name: (constant) @type)
(module name: (constant) @namespace)

(constant) @type

(identifier) @variable

(self) @variable.builtin

(symbol) @string.special

[
  "alias" "and" "begin" "break" "case" "class" "def" "defined?"
  "do" "else" "elsif" "end" "ensure" "false" "for" "if" "in"
  "module" "next" "nil" "not" "or" "redo" "rescue" "retry"
  "return" "self" "super" "then" "true" "undef" "unless"
  "until" "when" "while" "yield" "__ENCODING__" "__END__"
  "__FILE__" "__LINE__"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "===" "!=" "=~" "!~"
  "<" "<=" ">" ">=" "<=>" "&&" "||" "!" "&" "|" "^"
  "<<" ">>" "+=" "-=" "*=" "/=" "**" ".." "..." "->"
  "=>" "::" "?"
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";"] @punctuation.delimiter
"#;

// ── PHP ────────────────────────────────────────────────────────────────────────

pub const PHP_HIGHLIGHTS: &str = r#"
(comment) @comment

(string) @string
(encapsed_string) @string
(heredoc) @string
(interpolation) @string.special

(integer) @number
(float) @number.float
(boolean) @boolean
(null) @constant.builtin

(function_definition name: (name) @function)
(method_declaration name: (name) @function)
(function_call name: (name) @function.call)
(scoped_call name: (name) @function.call)
(member_call name: (name) @function.call)

(class_declaration name: (name) @type)
(interface_declaration name: (name) @type)
(trait_declaration name: (name) @type)
(enum_declaration name: (name) @type)

(name) @variable

(this) @variable.builtin

(attribute) @attribute

(php_tag) @keyword
(php_end_tag) @keyword

[
  "abstract" "and" "array" "as" "break" "callable" "case"
  "catch" "class" "clone" "const" "continue" "declare"
  "default" "die" "do" "echo" "else" "elseif" "empty"
  "enddeclare" "endfor" "endforeach" "endif" "endswitch"
  "endwhile" "eval" "exit" "extends" "final" "finally"
  "fn" "for" "foreach" "function" "global" "goto" "if"
  "implements" "include" "include_once" "instanceof"
  "insteadof" "interface" "isset" "list" "match" "namespace"
  "new" "or" "print" "private" "protected" "public" "readonly"
  "require" "require_once" "return" "static" "switch" "throw"
  "trait" "try" "unset" "use" "var" "while" "xor" "yield"
  "enum" "match"
] @keyword

[
  "+" "-" "*" "/" "%" "=" "==" "===" "!=" "!==" "<" "<="
  ">" ">=" "<=>" "&&" "||" "!" "and" "or" "xor" "&" "|"
  "^" "~" "<<" ">>" "+=" "-=" "*=" "/=" "%=" "&=" "|="
  "^=" "<<=" ">>=" ".=" "->" "=>" "::" "..."
] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ";"] @punctuation.delimiter

(namespace_definition name: (name) @namespace)
(use_declaration (name) @namespace)
"#;

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
        "html" => HTML_HIGHLIGHTS,
        "css" => CSS_HIGHLIGHTS,
        "scss" => SCSS_HIGHLIGHTS,
        "less" => LESS_HIGHLIGHTS,
        "yaml" => YAML_HIGHLIGHTS,
        "shell" | "bash" => BASH_HIGHLIGHTS,
        "sql" => SQL_HIGHLIGHTS,
        "java" => JAVA_HIGHLIGHTS,
        "csharp" => CSHARP_HIGHLIGHTS,
        "kotlin" => KOTLIN_HIGHLIGHTS,
        "ruby" => RUBY_HIGHLIGHTS,
        "php" => PHP_HIGHLIGHTS,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_query_for_rust() {
        let q = highlights_query_for(&Language::RUST);
        assert!(!q.is_empty());
        assert!(q.contains("@keyword"));
        assert!(q.contains("@string"));
    }

    #[test]
    fn highlights_query_for_python() {
        let q = highlights_query_for(&Language::PYTHON);
        assert!(!q.is_empty());
        assert!(q.contains("def"));
    }

    #[test]
    fn highlights_query_for_javascript() {
        let q = highlights_query_for(&Language::JAVASCRIPT);
        assert!(!q.is_empty());
        assert!(q.contains("function"));
    }

    #[test]
    fn highlights_query_for_typescript() {
        let q = highlights_query_for(&Language::TYPESCRIPT);
        assert!(!q.is_empty());
        assert!(q.contains("interface"));
    }

    #[test]
    fn highlights_query_for_go() {
        let q = highlights_query_for(&Language::GO);
        assert!(!q.is_empty());
        assert!(q.contains("func"));
    }

    #[test]
    fn highlights_query_for_c() {
        let q = highlights_query_for(&Language::C);
        assert!(!q.is_empty());
        assert!(q.contains("NULL"));
    }

    #[test]
    fn highlights_query_for_cpp() {
        let q = highlights_query_for(&Language::CPP);
        assert!(!q.is_empty());
        assert!(q.contains("class"));
    }

    #[test]
    fn highlights_query_for_json() {
        let q = highlights_query_for(&Language::JSON);
        assert!(!q.is_empty());
        assert!(q.contains("string"));
    }

    #[test]
    fn highlights_query_for_toml() {
        let q = highlights_query_for(&Language::TOML);
        assert!(!q.is_empty());
        assert!(q.contains("table"));
    }

    #[test]
    fn highlights_query_for_markdown() {
        let q = highlights_query_for(&Language::MARKDOWN);
        assert!(!q.is_empty());
        assert!(q.contains("heading"));
    }

    #[test]
    fn highlights_query_for_unknown_returns_empty() {
        let q = highlights_query_for(&Language::PLAIN_TEXT);
        assert!(q.is_empty());
        let q = highlights_query_for(&Language::new("unknown_lang_xyz"));
        assert!(q.is_empty());
    }

    #[test]
    fn highlights_query_for_html() {
        let q = highlights_query_for(&Language::HTML);
        assert!(!q.is_empty());
        assert!(q.contains("@tag"));
    }

    #[test]
    fn highlights_query_for_css() {
        let q = highlights_query_for(&Language::CSS);
        assert!(!q.is_empty());
        assert!(q.contains("property_name"));
    }

    #[test]
    fn highlights_query_for_scss() {
        let q = highlights_query_for(&Language::SCSS);
        assert!(!q.is_empty());
        assert!(q.contains("variable_name"));
    }

    #[test]
    fn highlights_query_for_less() {
        let q = highlights_query_for(&Language::LESS);
        assert!(!q.is_empty());
        assert!(q.contains("mixin"));
    }

    #[test]
    fn highlights_query_for_yaml() {
        let q = highlights_query_for(&Language::YAML);
        assert!(!q.is_empty());
        assert!(q.contains("@boolean"));
    }

    #[test]
    fn highlights_query_for_shell() {
        let q = highlights_query_for(&Language::SHELL);
        assert!(!q.is_empty());
        assert!(q.contains("command_name"));
    }

    #[test]
    fn highlights_query_for_sql() {
        let q = highlights_query_for(&Language::SQL);
        assert!(!q.is_empty());
        assert!(q.contains("SELECT"));
    }

    #[test]
    fn highlights_query_for_java() {
        let q = highlights_query_for(&Language::JAVA);
        assert!(!q.is_empty());
        assert!(q.contains("class_declaration"));
    }

    #[test]
    fn highlights_query_for_csharp() {
        let q = highlights_query_for(&Language::CSHARP);
        assert!(!q.is_empty());
        assert!(q.contains("namespace_declaration"));
    }

    #[test]
    fn highlights_query_for_kotlin() {
        let q = highlights_query_for(&Language::KOTLIN);
        assert!(!q.is_empty());
        assert!(q.contains("function_declaration"));
    }

    #[test]
    fn highlights_query_for_ruby() {
        let q = highlights_query_for(&Language::RUBY);
        assert!(!q.is_empty());
        assert!(q.contains("@string"));
    }

    #[test]
    fn highlights_query_for_php() {
        let q = highlights_query_for(&Language::PHP);
        assert!(!q.is_empty());
        assert!(q.contains("function_definition"));
    }
}
