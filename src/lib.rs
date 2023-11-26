mod index;
mod parser;
mod worker_pool;

use std::{fs, path::PathBuf};

use anyhow::{anyhow, bail};

use ignore::{types::TypesBuilder, WalkBuilder};
use index::Index;
use parser::{Definition, Tree};

use crate::index::{Declaration, Kind};

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

    pub fn print_dependencies(&self, type_name: &str) -> anyhow::Result<()> {
        let Some(root_id) = self.index.type_id(type_name) else {
            bail!("Type {} not found.", type_name)
        };

        let type_record = self
            .index
            .get_type(root_id)
            .ok_or_else(|| anyhow!("Could not find type #{} in the index", root_id))?;

        // Print the headline
        println!(
            "Type {}{}",
            type_record.name,
            if type_record.declarations.is_empty() {
                ": (unknown)"
            } else {
                ":"
            }
        );

        // Print declarations
        for declaration in &type_record.declarations {
            let path = self
                .index
                .file_path(declaration)
                .ok_or_else(|| anyhow!("Could not find path for declaration: {:?}", declaration))?;

            let kind = if declaration.kind == Kind::Extension {
                "extended"
            } else {
                "declared"
            };

            println!(
                "- {} in {} {}:{} depends on:",
                kind, path, declaration.point.row, declaration.point.column
            );

            for (type_id, points) in declaration.dependencies() {
                if type_id == root_id {
                    continue;
                }

                let type_record = self
                    .index
                    .get_type(type_id)
                    .ok_or_else(|| anyhow!("Could not find type #{} in the index", root_id))?;

                let points_str = points
                    .iter()
                    .map(|p| format!("{}:{}", p.row, p.column))
                    .collect::<Vec<_>>()
                    .join(", ");

                // Print the headline
                println!(
                    "  - {} (at {}){}",
                    type_record.name,
                    points_str,
                    if type_record.declarations.is_empty() {
                        ": (unknown)"
                    } else {
                        ":"
                    }
                );
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
