; Patterns run general to specific. When two captures cover the same node, the
; later pattern wins, and a capture on a smaller node always beats a larger one.

(identifier) @variable

[
  "local" "type" "export" "return" "do" "end" "while" "repeat" "until"
  "if" "elseif" "else" "then" "for" "in" "function"
  "and" "or" "not" "typeof"
] @keyword

[
  (break_statement)
  (continue_statement)
  (hash_bang_line)
] @keyword

[
  "+" "-" "*" "/" "%" "^" "#" "==" "~=" "<=" ">=" "<" ">" "=" "&" "|" "?"
  "//" ".." "+=" "-=" "*=" "/=" "%=" "^=" "..=" "//=" "->"
  ";" ":" "::" "," "." "(" ")" "[" "]" "{" "}"
] @operator

(number) @number

(string) @string
(escape_sequence) @constant

[
  (true)
  (false)
  (nil)
] @constant

((identifier) @constant
  (#eq? @constant "_VERSION"))

((identifier) @keyword
  (#eq? @keyword "self"))

(vararg_expression) @keyword

((identifier) @type
  (#match? @type "^[A-Z]"))

(parameter
  .
  (identifier)
  (identifier) @type)

(generic_type
  (identifier) @type)

(optional_type
  (identifier) @type)

(cast_expression
  (identifier) @type
  .)

(builtin_type) @type

(type_definition
  name: (identifier) @type)

; Written after the type heuristic so that ALL_CAPS names read as constants.
((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]+$"))

(function_call
  name: (identifier) @function)

(function_call
  name:
    (dot_index_expression
      field: (identifier) @function))

(function_call
  name:
    (method_index_expression
      method: (identifier) @function))

(function_declaration
  name: (identifier) @function)

(function_declaration
  name:
    (dot_index_expression
      field: (identifier) @function))

(function_declaration
  name:
    (method_index_expression
      method: (identifier) @function))

(comment) @comment
