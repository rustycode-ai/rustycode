(function_definition
  name: (identifier) @symbol.name) @symbol.kind.function

(class_definition
  name: (identifier) @symbol.name) @symbol.kind.class

(decorated_definition
  definition: (function_definition name: (identifier) @symbol.name)) @symbol.kind.function

(decorated_definition
  definition: (class_definition name: (identifier) @symbol.name)) @symbol.kind.class

(import_from_statement
  module_name: (_) @import)

(import_statement
  name: (_) @import)
