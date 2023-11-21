use std::fmt::{Display, Write};

use anyhow::{anyhow, bail};
use tree_sitter::{Node, Point, QueryCursor};

use super::Parser;

pub struct Tree<'parser> {
    pub parser: &'parser Parser,
    pub source: String,
    pub tree: tree_sitter::Tree,
}

#[derive(Debug)]
pub enum Definition {
    Class { kind: &'static str, name: String }, // Swift classes, enums and structs all capture as Class
    Protocol { name: String },
    Extension { name: String },
}

#[derive(Debug)]
pub struct Declaration {
    pub definition: Definition,
    pub location: Point,
    pub references: Vec<Reference>,
}

#[derive(Debug)]
pub struct Reference {
    pub name: String,
    pub location: Point,
}

impl Tree<'_> {
    pub fn package_name(&self) -> anyhow::Result<&str> {
        let query = &self.parser.queries.package_name;
        let mut query_cursor = QueryCursor::new();

        let first_match = query_cursor
            .matches(query, self.tree.root_node(), self.source.as_bytes())
            .next()
            .ok_or_else(|| anyhow!("No matches for Package declaration"))?;

        for capture in first_match.captures {
            if capture.index == 2 {
                return Ok(&self.source[capture.node.byte_range()]);
            }
        }

        bail!("No matches for Package declaration")
    }

    pub fn declarations(&self) -> anyhow::Result<Vec<Declaration>> {
        let query = &self.parser.queries.declaration;
        let mut query_cursor = QueryCursor::new();

        let mut declarations = vec![];

        let kind_index = query
            .capture_index_for_name("kind")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;
        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;
        let declaration_index = query
            .capture_index_for_name("declaration")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, self.tree.root_node(), self.source.as_bytes());

        for a_match in matches {
            let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();
            let kind_node = a_match.nodes_for_capture_index(kind_index).next();
            let match_node = a_match
                .nodes_for_capture_index(declaration_index)
                .next()
                .unwrap();

            let definition = match a_match.pattern_index {
                0 => Definition::Class {
                    kind: kind_node.unwrap().kind(),
                    name: self.source[name_node.byte_range()].to_string(),
                },
                1 => Definition::Protocol {
                    name: self.source[name_node.byte_range()].to_string(),
                },
                2 => Definition::Extension {
                    name: self.source[name_node.byte_range()].to_string(),
                },
                _ => bail!("Unexpected pattern index"),
            };

            declarations.push(Declaration {
                definition,
                location: name_node.start_position(),
                references: self.references_in(match_node, &self.source)?,
            })
        }

        Ok(declarations)
    }

    pub fn references<'a>(&self, source: &'a str) -> anyhow::Result<Vec<Reference>> {
        self.references_in(self.tree.root_node(), source)
    }

    fn references_in<'a>(&self, node: Node, source: &'a str) -> anyhow::Result<Vec<Reference>> {
        let query = &self.parser.queries.reference;

        let mut query_cursor = QueryCursor::new();

        let mut references = vec![];

        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, node, source.as_bytes());

        for a_match in matches {
            let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();

            references.push(Reference {
                name: source[name_node.byte_range()].to_string(),
                location: name_node.start_position(),
            })
        }

        Ok(references)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for Tree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn prefix(depth: usize) -> String {
            "  ".repeat(depth).to_string()
        }

        let node = self.tree.root_node();

        let mut depth = 0;
        let mut cursor = node.walk();

        loop {
            let node = cursor.node();

            // Print node

            let field_name = cursor.field_name().map(|name| format!("{}: ", name));

            f.write_str(&prefix(depth))?;
            if let Some(name) = field_name {
                f.write_str(&name)?;
            }
            f.write_str(&format!("({}", node.kind()))?;

            if node.child_count() < 1 && node.is_named() {
                f.write_str(&format!(" '{}'", &self.source[node.byte_range()],))?;
            }

            // Move down

            if cursor.goto_first_child() {
                f.write_char('\n')?;
                depth += 1;
                continue;
            }

            // Otherwise move left

            if cursor.goto_next_sibling() {
                f.write_str(")\n")?;

                continue;
            }

            // Otherwise go back

            loop {
                // Go up

                if !cursor.goto_parent() {
                    // back at root
                    return Ok(());
                }

                f.write_char(')')?;
                depth -= 1;

                // Try next sibling

                if cursor.goto_next_sibling() {
                    // There's another sibling to visit
                    f.write_char('\n')?;
                    break;
                }
            }
        }
    }
}
