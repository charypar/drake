mod index;
mod parser;
mod worker_pool;

use std::{fs, path::PathBuf};

use anyhow::{anyhow, bail};

use ignore::{types::TypesBuilder, WalkBuilder};
use index::{Declaration, Index, IndexItem};
use parser::{Definition, Tree};
use tree_sitter::Point;

use crate::index::{IndexCursor, Kind, Type, TypeId, TypeOrigin};

// Package definition
#[derive(Debug)]
pub struct Package {
    name: String,
    prefix: PathBuf,
}

#[derive(Default)]
pub struct Drake {
    index: Index,
}

// TODO make this API work in a library use-case

impl Drake {
    pub fn new() -> Self {
        Self {
            index: Index::new(),
        }
    }

    pub fn print(&mut self, path: &str, decl: bool, refs: bool, full: bool) -> anyhow::Result<()> {
        let mut builder = TypesBuilder::new();
        builder.add_defaults();

        let matcher = builder.select("swift").build()?;
        let walk = WalkBuilder::new(path).types(matcher).build_parallel();

        let results = worker_pool::process_files(walk, move |path, parser| {
            let source = fs::read_to_string(path)?;
            let tree = parser.parse(source)?;

            print(&path.to_string_lossy(), tree, decl, refs, full)
        });

        let mut count = 0;

        for file in results {
            count += 1;

            match file {
                Ok(out) => println!("{out}"),
                Err(e) => eprintln!("Could not process file: {e}"),
            }
        }

        println!("Done. Processed {count} files.");

        Ok(())
    }

    pub fn print_dependencies(
        &self,
        type_name: &str,
        include_external: bool,
    ) -> anyhow::Result<()> {
        let mut current_declaration: Option<&Declaration> = None;

        for (item, depth) in self.index.walk(type_name)? {
            let d = depth - (depth / 3);
            let prefix = "  ".repeat(d);

            match item {
                IndexItem::Type(id, name, origin) => {
                    let postfix = match origin {
                        TypeOrigin::External => " (external)",
                        TypeOrigin::Local => ":",
                    };

                    if origin == TypeOrigin::External && !include_external {
                        continue;
                    }

                    if let Some(declaration) = current_declaration {
                        let locations = if let Some(points) = declaration.dependencies().get(&id) {
                            let ps = points
                                .iter()
                                .map(|point| format!("{}:{}", point.row, point.column))
                                .collect::<Vec<_>>()
                                .join(", ");

                            format!(" ({})", ps)
                        } else {
                            "".to_string()
                        };

                        println!("{}- {}{}{}", prefix, name, locations, postfix);
                    } else {
                        println!("{}- {}{}", prefix, name, postfix);
                    }
                }
                IndexItem::Declaration(declaration) => {
                    current_declaration = Some(declaration);

                    let kind = match declaration.kind {
                        Kind::Struct => "struct declared",
                        Kind::Enum => "enum declared",
                        Kind::Class => "class declared",
                        Kind::Protocol => "protocol declared",
                        Kind::Extension => "extended",
                    };
                    let point = format!("{}:{}", declaration.point.row, declaration.point.column);
                    let path = self
                        .index
                        .file_path(declaration)
                        .expect("index refers to an unknown file");

                    println!("{}{} in {} {}, using types:", prefix, kind, path, point)
                }
                _ => (),
            }
        }

        Ok(())
    }

    // Builds the type index
    pub fn scan(&mut self, path: &str) -> anyhow::Result<()> {
        let mut builder = TypesBuilder::new();
        builder.add_defaults();

        let matcher = builder.select("swift").build()?;
        let walk = WalkBuilder::new(path).types(matcher).build_parallel();

        let results = worker_pool::process_files(walk, move |path, parser| {
            let source = fs::read_to_string(path)?;
            let tree = parser.parse(source)?;

            Ok((path.to_string_lossy().to_string(), tree.declarations()?))
        });

        let mut declaration_count = 0;
        let mut references_count = 0;

        for result in results {
            match result {
                Ok((file_path, declarations)) => {
                    for declaration in declarations {
                        declaration_count += 1;

                        let (name, kind) = match declaration.definition {
                            Definition::Class { kind, name } => match kind {
                                "class" => (name, Kind::Class),
                                "struct" => (name, Kind::Struct),
                                "enum" => (name, Kind::Enum),
                                x => {
                                    eprintln!("Unknown type kind {x}");
                                    unreachable!();
                                }
                            },
                            Definition::Protocol { name } => (name, Kind::Protocol),
                            Definition::Extension { name } => (name, Kind::Extension),
                        };
                        let point = declaration.location;
                        let references: Vec<_> = declaration
                            .references
                            .iter()
                            .map(|r| {
                                references_count += 1;

                                (r.name.as_str(), &r.location)
                            })
                            .collect();

                        self.index
                            .add_declaration(&name, kind, &file_path, point, &references);
                    }
                }
                Err(e) => eprintln!("Could not process file: {e}"),
            }
        }

        // FIXME get these stats from the Index
        println!("Searching {declaration_count} declarations and {references_count} references.");

        Ok(())
    }

    // TODO reuse later?
    pub fn package_name(&mut self, path: &str) -> anyhow::Result<()> {
        let mut builder = TypesBuilder::new();
        builder
            .add_defaults()
            .add("swiftpackage", "Package.swift")?;

        let matcher = builder.select("swiftpackage").build()?;
        let walk = WalkBuilder::new(path).types(matcher).build_parallel();

        let packages = worker_pool::process_files(walk, move |path, parser| {
            let source = fs::read_to_string(path)?;
            let tree = parser.parse(source)?;
            let name = tree.package_name()?;

            Ok(Package {
                name: name.to_string(),
                prefix: path
                    .parent()
                    .ok_or_else(|| anyhow!("Package manifest has no parent directory??"))?
                    .to_owned(),
            })
        });

        for package in packages {
            match package {
                Ok(package) => self
                    .index
                    .add_package(&package.name, &package.prefix.to_string_lossy()),
                Err(e) => eprintln!("Could not process file: {e}"),
            }
        }

        println!("Done. Index: {:#?}", self.index);

        Ok(())
    }
}

// TODO improve this
fn print(path: &str, tree: Tree, decl: bool, refs: bool, full: bool) -> anyhow::Result<String> {
    let mut out = String::new();

    out.push_str(&format!("# File {}\n", path));

    if decl {
        for declaration in tree.declarations()? {
            let loc = declaration.location;

            match declaration.definition {
                Definition::Class { kind, name } => {
                    out.push_str(&format!(
                        "\n{} {} at {}:{}\n",
                        kind, name, loc.row, loc.column
                    ));
                }
                Definition::Protocol { name } => {
                    out.push_str(&format!(
                        "\nprotocol {} at {}:{}\n",
                        name, loc.row, loc.column
                    ));
                }
                Definition::Extension { name } => {
                    out.push_str(&format!(
                        "\nextension {} at {}:{}\n",
                        name, loc.row, loc.column
                    ));
                }
            }

            if !refs {
                continue;
            }

            for reference in declaration.references {
                let loc = reference.location;

                out.push_str(&format!(
                    "- {} at {}:{}\n",
                    reference.name, loc.row, loc.column
                ));
            }
        }
    }

    if full {
        out.push_str("\n## Parse tree\n\n");
        out.push_str(&format!("{}\n", tree));
    }

    Ok(out)
}
