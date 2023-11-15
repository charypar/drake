use std::fmt::{Display, Write};

use anyhow::{anyhow, bail};
use tree_sitter::{Point, QueryCursor};

use super::Parser;

pub struct Tree<'a, 'parser> {
    pub parser: &'parser Parser,
    pub source: &'a str,
    pub tree: tree_sitter::Tree,
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

impl Tree<'_, '_> {
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

    pub fn declarations<'a>(&self, source: &'a str) -> anyhow::Result<Vec<Declaration<'a>>> {
        let query = &self.parser.queries.declaration;
        let mut query_cursor = QueryCursor::new();

        let mut declarations = vec![];

        let kind_index = query
            .capture_index_for_name("kind")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;
        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, self.tree.root_node(), source.as_bytes());

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
        let query = &self.parser.queries.reference;

        let mut query_cursor = QueryCursor::new();

        let mut references = vec![];

        let name_index = query
            .capture_index_for_name("name")
            .ok_or_else(|| anyhow!("Failed parsing captures"))?;

        let matches = query_cursor.matches(query, self.tree.root_node(), source.as_bytes());

        for a_match in matches {
            let name_node = a_match.nodes_for_capture_index(name_index).next().unwrap();

            references.push(Reference {
                name: &source[name_node.byte_range()],
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

impl Display for Tree<'_, '_> {
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
