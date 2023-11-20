# Drake

A tree-sitter based codebase dependency explorer.

## About

Drake (as in Sir Francis) is a static analysis tool to map and search
dependencies in a codebase by finding declarations and references and building
a graph.

Drake currently support Swift, but it is based on tree-sitter, and may support  other languages in the future.

## Usage

### As CLI

Drake can be installed with `cargo install`.

In the current version it supports two tasks

- `drake scan [PATH]` finds all swift packages in the given path
- `drake print [PATH]` prints the declarations and references in each file.

### As a library

Cargo docs coming soon.

## License

Drake is licensed under the MIT license. See [LICENSE](LICENSE) for more.
