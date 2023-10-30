use std::{env, fs};

use anyhow::{anyhow, bail};
use ignore::{types::TypesBuilder, WalkBuilder, WalkState};
use tree_sitter::{Node, Parser, Query, QueryCursor};

const TEST_SOURCE: &str = r#"
// swift-tools-version: 5.7
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "AppFeatures",
    products: [
        // Products define the executables and libraries a package produces, and make them visible to other packages.
        .library(
            name: "AppFeatures",
            targets: ["AppFeatures"]),
    ],
    dependencies: [
        .package(path: "ResourceDownloader"),
        .package(path: "UserFeature")
    ],
    targets: [
        // Targets are the basic building blocks of a package. A target can define a module or a test suite.
        // Targets can depend on other targets in this package, and on products in packages this package depends on.
        .target(
            name: "AppFeatures",
            dependencies: ["ResourceDownloader", "UserFeature"]),
        .testTarget(
            name: "AppFeaturesTests",
            dependencies: ["AppFeatures"]),
    ]
)
"#;

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];

    let mut builder = TypesBuilder::new();
    builder
        .add_defaults()
        .add("swiftpackage", "Package.swift")
        .expect("Can't add package.swift matcher");

    let matcher = builder
        .select("swiftpackage")
        .build()
        .expect("can't build swift matcher");

    let walk = WalkBuilder::new(path).types(matcher).build_parallel();

    walk.run(|| {
        Box::new(move |result| {
            if let Ok(dent) = result {
                if let Some(ftype) = dent.file_type() {
                    if !ftype.is_dir() {
                        let source = fs::read_to_string(dent.path()).expect("Can't read file");
                        let name = get_package_name(&source).expect("Cant't work out package name");

                        println!(
                            "Package '{}' with path prefix: {:?}",
                            name,
                            dent.path().parent().unwrap()
                        );
                    }
                }
            }

            WalkState::Continue
        })
    })
}

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
            return Ok(source[capture.node.byte_range()].to_string());
        }
    }

    bail!("No matches for Package declaration")
}

fn print_node(node: Node, source: &str) {
    let mut depth = 0;
    let mut cursor = node.walk();

    loop {
        let node = cursor.node();

        print!("{}({}):", prefix(depth), node.kind());

        if node.child_count() < 1 {
            println!(
                " '{}' {} .. {}",
                &source[node.byte_range()],
                node.start_position(),
                node.end_position(),
            );
        } else {
            println!()
        }

        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }

        if cursor.goto_next_sibling() {
            continue;
        }

        // can't go any deeper or further, go up

        loop {
            if !cursor.goto_parent() {
                // back at root
                return;
            }
            depth -= 1;

            if cursor.goto_next_sibling() {
                // There's another sibling to visit
                break;
            }
        }
    }
}

fn prefix(depth: usize) -> String {
    "  ".repeat(depth).to_string()
}
