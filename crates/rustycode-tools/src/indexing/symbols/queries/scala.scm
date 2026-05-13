(class_definition name: (identifier) @symbol.name) @symbol.kind.class
(object_definition name: (identifier) @symbol.name) @symbol.kind.class
(trait_definition name: (identifier) @symbol.name) @symbol.kind.interface
(function_definition name: (identifier) @symbol.name) @symbol.kind.method
(import_declaration (identifier) @import)
(import_declaration (scoped_identifier) @import)
