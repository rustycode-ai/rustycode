(namespace_definition name: (identifier) @symbol.name) @symbol.kind.module
(class_specifier name: (type_identifier) @symbol.name) @symbol.kind.class
(struct_specifier name: (type_identifier) @symbol.name) @symbol.kind.struct
(function_definition declarator: (function_declarator declarator: (identifier) @symbol.name)) @symbol.kind.function
(function_definition declarator: (function_declarator declarator: (field_identifier) @symbol.name)) @symbol.kind.method
(preproc_def name: (identifier) @symbol.name) @symbol.kind.macro
(preproc_function_def name: (identifier) @symbol.name) @symbol.kind.macro
(preproc_include path: (string_literal) @import)
(preproc_include path: (system_lib_string) @import)
