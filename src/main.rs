use std::env;
use tree_sitter::Node;

use drake::Drake;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];

    let mut drake = Drake::new();

    println!("Scanning path {}", path);
    drake.scan(path)
}

// Helpers

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
