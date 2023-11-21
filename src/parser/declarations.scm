(class_declaration
    declaration_kind: _ @kind
    name: (type_identifier) @name
) @declaration

(protocol_declaration
    name: (type_identifier) @name
) @declaration

(class_declaration
    declaration_kind: _ @kind
    name: (user_type (type_identifier) @name)
) @declaration
