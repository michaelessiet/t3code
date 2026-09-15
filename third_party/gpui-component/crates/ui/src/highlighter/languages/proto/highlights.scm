; Native protobuf highlighting for the bundled grammar, which does not
; expose a Rust HIGHLIGHTS_QUERY constant.
(comment) @comment
(string) @string
(int_lit) @number
(float_lit) @number
[(true) (false)] @boolean
[(message_name) (enum_name) (service_name) (type) (key_type)] @type
(rpc_name) @function
["syntax" "edition" "package" "import" "option" "message" "enum"
 "service" "rpc" "returns" "oneof" "repeated" "reserved" "to"] @keyword
["{" "}" "[" "]" "(" ")"] @punctuation.bracket
[";" "," "."] @punctuation.delimiter
"=" @operator
