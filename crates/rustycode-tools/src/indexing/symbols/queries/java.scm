(class_declaration name: (identifier) @symbol.name) @symbol.kind.class
(interface_declaration name: (identifier) @symbol.name) @symbol.kind.interface
(enum_declaration name: (identifier) @symbol.name) @symbol.kind.enum
(method_declaration name: (identifier) @symbol.name) @symbol.kind.method
(constructor_declaration name: (identifier) @symbol.name) @symbol.kind.method
(import_declaration (scoped_identifier) @import)
(import_declaration (identifier) @import)
