(function_declaration name: (identifier) @symbol.name) @symbol.kind.function
(method_declaration name: (field_identifier) @symbol.name) @symbol.kind.method
(type_declaration (type_spec name: (type_identifier) @symbol.name)) @symbol.kind.struct
(import_spec path: (interpreted_string_literal) @import)
