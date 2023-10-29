use tree_sitter::{Node, Query, QueryCursor};

const test_source: &str = r#"
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

// Matches a package name in a Package.swift file
const package_name_query: &str = r#"
(call_expression
    (simple_identifier) @call_ident (#eq? @call_ident "Package")
    (call_suffix
        (value_arguments
            (value_argument
                (simple_identifier) @name (#eq? @name "name")
                (line_string_literal
                    (line_str_text) @package_name)))))
"#;

fn main() {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser
        .set_language(swift_language)
        .expect("failed to set swift language");

    // Parse a tree

    let tree = parser
        .parse(test_source, None)
        .expect("Couldn't parse the code");

    print_node(tree.root_node());

    // Test a query

    let query = Query::new(swift_language, package_name_query).expect("failed parsing query");
    let mut query_cursor = QueryCursor::new();

    for a_match in query_cursor.matches(&query, tree.root_node(), test_source.as_bytes()) {
        println!("\n\n# New match:");

        for capture in a_match.captures {
            println!("\n## New capture ({}): ", capture.index);
            print_node(capture.node);
        }
    }
}

fn print_node(node: Node) {
    let mut depth = 0;
    let mut cursor = node.walk();

    loop {
        let node = cursor.node();

        print!("{}({}):", prefix(depth), node.kind());

        if node.child_count() < 1 {
            println!(
                " '{}' {} .. {}",
                &test_source[node.byte_range()],
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
