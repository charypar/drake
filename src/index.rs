use std::collections::HashMap;

use patricia_tree::GenericPatriciaMap;
use tree_sitter::Point;

// TODO consider pros/cons of using Paths and PathBufs

// IDs are an offset into the list of known packages
pub type PackageId = usize;
pub type FileId = usize;
pub type TypeId = usize;

#[derive(Debug)]
pub struct Type {
    pub name: String,
    pub declaration: Option<Declaration>,
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
#[derive(Debug)]
pub struct Declaration {
    /// Declaration kind
    pub kind: Kind,
    /// Location within the file
    pub point: Point,
    // File in which the declaration is
    file: FileId,
    // Types the declaration uses and their locations
    dependencies: Vec<(TypeId, Point)>,
}

impl Declaration {
    fn file_path(&self) -> String {
        todo!()
    }

    fn package_name(&self) -> Option<String> {
        todo!()
    }

    fn package_path(&self) -> Option<String> {
        todo!()
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

// TODO do I need a builder...?

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

        // FIXME
        // Extensions need to get indexed separately, they make the
        // name:declaration relationship a 1:many
        if kind == Kind::Extension {
            return self.add_reference(name);
        }

        // Create or update the type declaration

        match self.type_ids.get(name) {
            Some(&type_id) => {
                println!("Updating type {} with declarations", name);
                self.types[type_id].declaration = Some(declaration);

                type_id
            }
            None => {
                println!("Creating type {} with declaration", name);
                let t = Type {
                    name: name.to_string(),
                    declaration: Some(declaration),
                };

                self.types.push(t);

                self.types.len() - 1
            }
        }
    }

    pub fn add_reference(&mut self, name: &str) -> TypeId {
        match self.type_ids.get(name) {
            Some(&type_id) => type_id,
            None => {
                println!("Creating type {} from reference", name);

                let t = Type {
                    name: name.to_string(),
                    declaration: None,
                };

                self.types.push(t);

                let type_id = self.types.len() - 1;

                self.type_ids.insert(name.to_string(), type_id);

                type_id
            }
        }
    }
}
