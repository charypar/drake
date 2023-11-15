use anyhow::{anyhow, bail};
use tree_sitter::{Language, Node, Point, Query, QueryCursor, Tree};

// Matches a package name in a Package.swift file
const PACKAGE_NAME_QUERY: &str = include_str!("package_name.scm");
const DECLARATIONS_QUERY: &str = include_str!("declarations.scm");
const REFERENCES_QUERY: &str = include_str!("references.scm");

pub struct Parser {
    language: Language,
    queries: Queries,
}

struct Queries {
    package_name: Query,
    declaration: Query,
    reference: Query,
}

#[derive(Debug)]
pub enum Definition<'a> {
    Class { kind: &'static str, name: &'a str }, // Swift classes, enums and structs all capture as Class
    Protocol { name: &'a str },
    Extension { name: &'a str },
}

#[derive(Debug)]
pub struct Declaration<'a> {
    pub definition: Definition<'a>,
    pub location: Point,
}

#[derive(Debug)]
pub struct Reference<'a> {
    pub name: &'a str,
    pub location: Point,
}

impl Parser {
    pub fn new() -> Self {
        let language = tree_sitter_swift::language();

        let queries = Queries {
            package_name: Query::new(language, PACKAGE_NAME_QUERY)
                .expect("Failed to parse package name query"),
            declaration: Query::new(language, DECLARATIONS_QUERY)
                .expect("Failed to parse package name query"),
            reference: Query::new(language, REFERENCES_QUERY)
                .expect("Failed to parse package name query"),
        };

        Self { language, queries }
    }

    pub fn package_name<'a>(&self, source: &'a str) -> anyhow::Result<&'a str> {
        let tree = self.parse(source)?;
        let mut query_cursor = QueryCursor::new();

        let first_match = query_cursor
            .matches(
                &self.queries.package_name,
                tree.root_node(),
                source.as_bytes(),
            )
            .next()
            .ok_or_else(|| anyhow!("No matches for Package declaration"))?;

        for capture in first_match.captures {
            if capture.index == 2 {
                return Ok(&source[capture.node.byte_range()]);
            }
        }

        bail!("No matches for Package declaration")
    }

    pub fn declarations<'a>(&self, source: &'a str) -> anyhow::Result<Vec<Declaration<'a>>> {
        let tree = self.parse(source)?;
        let query = &self.queries.declaration;
        let mut query_cursor = QueryCursor::new();

        let mut declarations = vec![];

        let kind_index = query
            .capture_index_for_name("kind")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;
        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, tree.root_node(), source.as_bytes());

        for a_match in matches {
            let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();
            let kind_node = a_match.nodes_for_capture_index(kind_index).next();

            let definition = match a_match.pattern_index {
                0 => Definition::Class {
                    kind: kind_node.unwrap().kind(),
                    name: &source[name_node.byte_range()],
                },
                1 => Definition::Protocol {
                    name: &source[name_node.byte_range()],
                },
                2 => Definition::Extension {
                    name: &source[name_node.byte_range()],
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

    pub fn references<'a>(&self, source: &'a str) -> anyhow::Result<Vec<Reference<'a>>> {
        let tree = self.parse(source)?;
        let query = &self.queries.reference;

        let mut query_cursor = QueryCursor::new();

        let mut references = vec![];

        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, tree.root_node(), source.as_bytes());

        for a_match in matches {
            let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();

            references.push(Reference {
                name: &source[name_node.byte_range()],
                location: name_node.start_position(),
            })
        }

        Ok(references)
    }

    fn parse(&self, source: &str) -> anyhow::Result<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(self.language)?;

        parser
            .parse(source, None)
            .ok_or_else(|| anyhow!("Could not parse Swift source"))
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
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
