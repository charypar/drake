(class_declaration
    declaration_kind: _ @kind
    name: (type_identifier) @name
)

(protocol_declaration
    name: (type_identifier) @name
)

(class_declaration
    declaration_kind: _ @kind
    name: (user_type (type_identifier) @name)
)
