mod tree;

use anyhow::anyhow;
use tree_sitter::{Language, Query};

use tree::Tree;
pub use tree::{Declaration, Definition, Reference};

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

    pub fn parse<'a, 'p>(&'p self, source: &'a str) -> anyhow::Result<Tree<'a, 'p>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(self.language)?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow!("Could not parse Swift source"))?;

        Ok(Tree {
            source,
            tree,
            parser: self,
        })
    }
}
