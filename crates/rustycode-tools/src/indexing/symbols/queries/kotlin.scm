(class_declaration name: (type_identifier) @symbol.name) @symbol.kind.class
(object_declaration name: (type_identifier) @symbol.name) @symbol.kind.class
(function_declaration name: (identifier) @symbol.name) @symbol.kind.function
(class_body (function_declaration name: (identifier) @symbol.name)) @symbol.kind.method
(import_header (identifier) @import)
(import_header (dot_qualified_expression) @import)
