(function_item
  name: (identifier) @symbol.name) @symbol.kind.function

(impl_item
  type: (_) @symbol.name) @symbol.kind.impl

(struct_item
  name: (type_identifier) @symbol.name) @symbol.kind.struct

(enum_item
  name: (type_identifier) @symbol.name) @symbol.kind.enum

(trait_item
  name: (type_identifier) @symbol.name) @symbol.kind.trait

(function_signature_item
  name: (identifier) @symbol.name) @symbol.kind.function
