(function_declaration name: (identifier) @symbol.name) @symbol.kind.function
(method_definition name: (property_identifier) @symbol.name) @symbol.kind.method
(class_declaration name: (identifier) @symbol.name) @symbol.kind.class
(lexical_declaration (variable_declarator name: (identifier) @symbol.name value: (arrow_function))) @symbol.kind.function
(import_statement source: (string) @import)
