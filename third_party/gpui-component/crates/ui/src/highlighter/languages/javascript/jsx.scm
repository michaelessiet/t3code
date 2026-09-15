; JSX-specific rules are appended after the ordinary identifier rules so
; elements and attributes get their more specific styles.
(jsx_opening_element name: (identifier) @tag)
(jsx_closing_element name: (identifier) @tag)
(jsx_self_closing_element name: (identifier) @tag)

(jsx_opening_element name: (member_expression) @constructor)
(jsx_closing_element name: (member_expression) @constructor)
(jsx_self_closing_element name: (member_expression) @constructor)

(jsx_attribute (property_identifier) @attribute)
(jsx_namespace_name) @tag

(jsx_opening_element ["<" ">"] @punctuation.bracket)
(jsx_closing_element ["</" ">"] @punctuation.bracket)
(jsx_self_closing_element ["<" "/>"] @punctuation.bracket)
(jsx_expression ["{" "}"] @punctuation.special)
