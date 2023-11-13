use std::{fs, path::PathBuf};

use anyhow::{anyhow, bail};
use crossbeam::channel::{Receiver, Sender};
use tree_sitter::{Node, Point, Query, QueryCursor};

use crate::Package;

// A task for a parser worker to perform on a file with a path
pub enum Task {
    // Read Package.swift file and find package name
    PackageName(PathBuf, Sender<TaskResult>),
}

// A result from a parser worker
pub enum TaskResult {
    Package(Package),
}

pub struct Worker {
    task_rx: Receiver<Task>,
}

impl Worker {
    pub fn new(task_rx: Receiver<Task>) -> Self {
        Self { task_rx }
    }

    pub fn start(&self) -> anyhow::Result<()> {
        while let Ok(task) = self.task_rx.recv() {
            match task {
                Task::PackageName(path, result_tx) => {
                    let source = fs::read_to_string(&path)?;
                    let name = get_package_name(&source)?;

                    let package = Package {
                        name,
                        prefix: path
                            .parent()
                            .ok_or_else(|| anyhow!("Package manifest has no parent directory??"))?
                            .to_owned(),
                    };

                    result_tx.send(TaskResult::Package(package))?;
                }
            }
        }

        Ok(())
    }
}

// TODO wrap these so we can reuse initialisation

// Matches a package name in a Package.swift file
const PACKAGE_NAME_QUERY: &str = r#"
(call_expression
    (simple_identifier) @call_ident (#eq? @call_ident "Package")
    (call_suffix
        (value_arguments
            (value_argument
                (simple_identifier) @name_arg (#eq? @name_arg "name")
                (line_string_literal
                    (line_str_text) @package_name)))))
"#;

fn get_package_name(source: &str) -> anyhow::Result<String> {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser
        .set_language(swift_language)
        .expect("failed to set swift language");

    let tree = parser.parse(source, None).expect("Couldn't parse the code");

    // FIXME: No need to do this every time
    let query = Query::new(swift_language, PACKAGE_NAME_QUERY).expect("failed parsing query");
    let mut query_cursor = QueryCursor::new();

    let first_match = query_cursor
        .matches(&query, tree.root_node(), source.as_bytes())
        .next()
        .ok_or_else(|| anyhow!("No matches for Package declaration"))?;

    for capture in first_match.captures {
        if capture.index == 2 {
            // FIXME use (source-text) function
            return Ok(source[capture.node.byte_range()].to_string());
        }
    }

    bail!("No matches for Package declaration")
}

// Type Declarations

const DECLARATIONS_QUERY: &str = r#"
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
"#;

// TODO use &str into the source instead to avoid hundreds of tiny allocations

#[derive(Debug)]
pub enum Definition {
    Class { kind: String, name: String }, // Swift classes, enums and structs all capture as Class
    Protocol { name: String },
    Extension { name: String },
}

#[derive(Debug)]
pub struct Declaration {
    pub definition: Definition,
    pub location: Point,
}

pub fn declarations(source: &str) -> anyhow::Result<Vec<Declaration>> {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser.set_language(swift_language)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("Could not parse the file"))?;

    // FIXME: No need to do this every time
    let query = Query::new(swift_language, DECLARATIONS_QUERY)?;
    let mut query_cursor = QueryCursor::new();

    let mut declarations = vec![];

    let kind_index = query
        .capture_index_for_name("kind")
        .ok_or_else(|| anyhow!("Failed parsing captures"))?;
    let name_index = query
        .capture_index_for_name("name")
        .ok_or_else(|| anyhow!("Failed parsing captures"))?;

    let matches = query_cursor.matches(&query, tree.root_node(), source.as_bytes());

    for a_match in matches {
        let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();
        let kind_node = a_match.nodes_for_capture_index(kind_index).next();

        let definition = match a_match.pattern_index {
            0 => Definition::Class {
                kind: kind_node.unwrap().kind().to_string(),
                name: source[name_node.byte_range()].to_string(),
            },
            1 => Definition::Protocol {
                name: source[name_node.byte_range()].to_string(),
            },
            2 => Definition::Extension {
                name: source[name_node.byte_range()].to_string(),
            },
            _ => bail!("Unexpected pattern index"),
        };

        declarations.push(Declaration {
            definition,
            location: name_node.start_position(),
        })
    }

    Ok(declarations)
}

// Type References

const REFERENCES_QUERY: &str = r#"
((type_identifier) @name (#match? @name "^[A-Z]")) @name

((simple_identifier) @name (#match? @name "^[A-Z]")) @name
"#;

// TODO use &str into the source instead to avoid hundreds of tiny allocations

#[derive(Debug)]
pub struct Reference {
    pub name: String,
    pub location: Point,
}

pub fn references(source: &str) -> anyhow::Result<Vec<Reference>> {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser.set_language(swift_language)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("Could not parse the file"))?;

    // FIXME: No need to do this every time
    let query = Query::new(swift_language, REFERENCES_QUERY)?;
    let mut query_cursor = QueryCursor::new();

    let mut references = vec![];

    let name_index = query
        .capture_index_for_name("name")
        .ok_or_else(|| anyhow!("Failed parsing captures"))?;

    let matches = query_cursor.matches(&query, tree.root_node(), source.as_bytes());

    for a_match in matches {
        let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();

        references.push(Reference {
            name: source[name_node.byte_range()].to_string(),
            location: name_node.start_position(),
        })
    }

    Ok(references)
}

// Printing

pub fn to_sexp(source: &str) -> anyhow::Result<String> {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser.set_language(swift_language)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("Couldn't parse as swift file"))?;

    print_node(tree.root_node(), source)
}

fn print_node(node: Node, source: &str) -> anyhow::Result<String> {
    let mut depth = 0;
    let mut cursor = node.walk();

    let mut output = String::new();

    loop {
        let node = cursor.node();

        let field_name = cursor.field_name().map(|name| format!("{}: ", name));

        output.push_str(&prefix(depth));
        if let Some(name) = field_name {
            output.push_str(&name);
        }
        output.push_str(&format!("({}", node.kind()));

        if node.child_count() < 1 && node.is_named() {
            output.push_str(&format!(" '{}'", &source[node.byte_range()],));
        }

        if cursor.goto_first_child() {
            output.push('\n');
            depth += 1;
            continue;
        }

        if cursor.goto_next_sibling() {
            output.push_str(")\n");

            continue;
        }

        // can't go any deeper or further, go up

        loop {
            if !cursor.goto_parent() {
                // back at root
                return Ok(output);
            }

            output.push(')');
            depth -= 1;

            if cursor.goto_next_sibling() {
                // There's another sibling to visit
                output.push('\n');
                break;
            }
        }
    }
}

fn prefix(depth: usize) -> String {
    "  ".repeat(depth).to_string()
}
