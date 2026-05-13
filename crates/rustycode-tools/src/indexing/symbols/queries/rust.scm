(struct_item name: (type_identifier) @symbol.name) @symbol.kind.struct
(enum_item name: (type_identifier) @symbol.name) @symbol.kind.enum
(trait_item name: (type_identifier) @symbol.name) @symbol.kind.trait
(macro_definition name: (identifier) @symbol.name) @symbol.kind.macro

;; Catch functions first
(function_item name: (identifier) @symbol.name) @symbol.kind.function

;; Override functions inside impl blocks to be methods
(impl_item 
  (function_item name: (identifier) @symbol.name) @symbol.kind.method)

(impl_item @symbol.kind.impl)

;; Support for modules (nested structure)
(mod_item name: (identifier) @symbol.name) @symbol.kind.module

;; Constants
(const_item name: (identifier) @symbol.name) @symbol.kind.constant

;; Type aliases
(type_item name: (type_identifier) @symbol.name) @symbol.kind.type

;; Imports
(use_declaration argument: (_) @import)

;; Capture macro invocations as symbols (useful for DSLs and large macros)
(macro_invocation 
  macro: (identifier) @symbol.name) @symbol.kind.macro

;; Doc comments
((line_comment) @symbol.doc)
((block_comment) @symbol.doc)
