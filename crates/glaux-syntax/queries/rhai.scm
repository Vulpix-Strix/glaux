; Patterns run general to specific. When two captures cover the same node, the
; later pattern wins, and a capture on a smaller node always beats a larger one.

(ident) @variable

[
  "as" "break" "catch" "const" "do" "else" "export" "fn" "for" "if" "import"
  "in" "let" "loop" "private" "return" "switch" "throw" "try" "until" "while"
] @keyword

(ExprContinue) @keyword

((ident) @keyword
  (#eq? @keyword "this"))

(binop) @operator

((binop) @keyword
  (#eq? @keyword "in"))

[
  "(" ")" "{" "}" "[" "]" "#{" "${" ";" "," "=>"
] @operator

(lit_int) @number
(lit_float) @number
(lit_str) @string
(lit_char) @string
(str_template_str_content) @string
(lit_bool) @constant

((ident) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]+$"))

(ExprCall
  fn_name:
    (Expr
      (ExprPath
        (Path
          (ident) @function
          .))))

(ExprCall
  fn_name:
    (Expr
      (ExprDotAccess
        (Expr
          (ExprPath
            (Path
              (ident) @function
              .)))
        .)))

(ExprFn
  fn_name:
    (FnDeclName
      (ident) @function))

[
  (comment_line)
  (comment_block)
  (comment_line_doc)
  (comment_block_doc)
] @comment
