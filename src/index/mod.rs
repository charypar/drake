mod cursor;

use std::collections::HashMap;

use anyhow::anyhow;
use patricia_tree::GenericPatriciaMap;
use tree_sitter::Point;

pub use cursor::{IndexCursor, IndexItem};

// TODO consider pros/cons of using Paths and PathBufs

// IDs are an offset into the list of known packages
pub type PackageId = usize;
pub type FileId = usize;
pub type TypeId = usize;

#[derive(Debug, PartialEq)]
pub struct Type {
    pub name: String,
    pub declarations: Vec<Declaration>, // A type may be extended in multiple places
}

#[derive(Debug, PartialEq)]
pub enum TypeOrigin {
    Local,
    External,
}

impl Type {
    fn origin(&self) -> TypeOrigin {
        if self.declarations.is_empty() {
            TypeOrigin::External
        } else {
            TypeOrigin::Local
        }
    }
}

#[derive(Debug)]
pub struct Package {
    name: String,
    path_prefix: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Kind {
    Struct,
    Enum,
    Class,
    Protocol,
    Extension,
}

/// Type declaration
#[derive(Debug, PartialEq, Clone)]
pub struct Declaration {
    /// Declaration kind
    pub kind: Kind,
    /// Location within the file
    pub point: Point,
    // File in which the declaration is
    file: FileId,
    // Types the declaration uses and locations of the references
    dependencies: Vec<(TypeId, Point)>,
}

impl Declaration {
    pub fn dependencies(&self) -> HashMap<TypeId, Vec<Point>> {
        let mut deps = HashMap::new();

        for (id, point) in &self.dependencies {
            deps.entry(*id).or_insert(vec![]).push(*point)
        }

        deps
    }
}

#[derive(Debug, Default)]
pub struct Index {
    // Storage
    packages: Vec<Package>,
    files: Vec<String>,
    types: Vec<Type>,

    // Indexes
    file_ids: HashMap<String, FileId>,
    package_ids: HashMap<String, PackageId>,
    type_ids: HashMap<String, TypeId>,
    packages_by_path: GenericPatriciaMap<String, PackageId>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            packages: vec![],
            files: vec![],
            types: vec![],
            package_ids: HashMap::new(),
            type_ids: HashMap::new(),
            packages_by_path: GenericPatriciaMap::new(),
            file_ids: HashMap::new(),
        }
    }

    // Reading from index

    /// Geta type ID for a string name
    pub fn type_id(&self, name: &str) -> Option<TypeId> {
        self.type_ids.get(name).copied()
    }

    /// Get a type definition for a type ID
    pub fn get_type(&self, type_id: TypeId) -> Option<&Type> {
        self.types.get(type_id)
    }

    /// Find a file path where declaration was made
    pub fn file_path(&self, declaration: &Declaration) -> Option<String> {
        self.files.get(declaration.file).cloned()
    }

    pub fn walk(&self, type_name: &str) -> anyhow::Result<IndexCursor> {
        let type_id = self
            .type_id(type_name)
            .ok_or_else(|| anyhow!("Type name {} not found in the index.", type_name))?;

        Ok(IndexCursor::new(self, type_id))
    }

    // Building the index
    // TODO do I need an IndexBuilder...?

    /// Add a package to the index
    pub fn add_package(&mut self, name: &str, path_prefix: &str) {
        let name = name.to_string();
        let path_prefix = path_prefix.to_string();

        let package = Package {
            name: name.to_string(),
            path_prefix: path_prefix.clone(),
        };

        self.packages.push(package);
        let package_id = self.packages.len() - 1;

        self.package_ids.insert(name.to_string(), package_id);
        self.packages_by_path.insert(path_prefix, package_id);
    }

    /// Add a type declaration to the index
    pub fn add_declaration(
        &mut self,
        name: &str,
        kind: Kind,
        file: &str,
        point: Point,
        references: &[(&str, &Point)],
    ) -> TypeId {
        let file_id = *self.file_ids.entry(file.to_string()).or_insert_with(|| {
            self.files.push(file.to_string());

            self.files.len() - 1
        });

        let dependencies: Vec<_> = references
            .iter()
            .map(|(type_name, &ref_point)| {
                let type_id = self.add_reference(type_name);

                (type_id, ref_point)
            })
            .collect();

        let declaration = Declaration {
            kind,
            point,
            file: file_id,
            dependencies,
        };

        // Create or update the type declaration

        match self.type_ids.get(name) {
            Some(&type_id) => {
                self.types[type_id].declarations.push(declaration);

                type_id
            }
            None => {
                let t = Type {
                    name: name.to_string(),
                    declarations: vec![declaration],
                };

                self.types.push(t);

                let type_id = self.types.len() - 1;
                self.type_ids.insert(name.to_string(), type_id);

                type_id
            }
        }
    }

    pub fn add_reference(&mut self, name: &str) -> TypeId {
        match self.type_ids.get(name) {
            Some(&type_id) => type_id,
            None => {
                let t = Type {
                    name: name.to_string(),
                    declarations: vec![],
                };

                self.types.push(t);

                let type_id = self.types.len() - 1;
                self.type_ids.insert(name.to_string(), type_id);

                type_id
            }
        }
    }
}
