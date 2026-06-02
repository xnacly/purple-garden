; keywords
[
  "import"
  "let"
  "fn"
  "match"
  "as"
  "true"
  "false"
  "Str"
  "Int"
  "Double"
  "Bool"
  "Void"
  "Foreign"
  "Option"
  "Array"
] @keyword

; identifiers
(identifier) @variable
(field_suffix (identifier) @property)
(function_declaration (identifier) @function)
(parameter (identifier) @variable.parameter)
(type_identifier) @type

; literals
(string) @string
(number) @number

; operators and punctuation
[
  "="
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  ">"
  ":"
  "."
] @operator

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
  "<"
  ">"
] @punctuation.bracket

(comment) @comment
