use std::collections::HashMap;

use patricia_tree::GenericPatriciaMap;

use crate::Package;

// PackageId is an offset into the list of known packages
pub type PackageId = usize;

#[derive(Debug)]
pub struct Index {
    // Known packages. Offset is used as PackageId
    packages: Vec<Package>,
    // Find a package by name (e.g. for import)
    packages_by_name: HashMap<String, PackageId>,
    // Find a package by file path prefix
    packages_by_path: GenericPatriciaMap<String, PackageId>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            packages: vec![],
            packages_by_name: HashMap::new(),
            packages_by_path: GenericPatriciaMap::new(),
        }
    }

    pub fn add_package(&mut self, package: Package) {
        let name = package.name.clone();
        let path = package
            .prefix
            .to_str()
            .expect("path should convert to string")
            .to_string();

        self.packages.push(package);
        let package_id = self.packages.len() - 1;

        self.packages_by_name.insert(name.clone(), package_id);
        self.packages_by_path.insert(path, package_id);

        println!("Indexed package {} under ID #{}", name, package_id);
    }
}
