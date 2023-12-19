use std::collections::{HashMap, HashSet};

use tree_sitter::Point;

use super::{Declaration, Index, Type, TypeId, TypeOrigin};

#[derive(Debug, PartialEq)]
pub enum IndexItem<'a> {
    Type(TypeId, &'a str, TypeOrigin),
    Declaration(&'a Declaration),
    Dependency(TypeId, &'a str, Point),
}

/// A stateful object representing a search through the index graph
/// Walking the Index prevents following back edges, but will not prevent re-visiting
/// types that have been visited before, but are now reached via a new path
pub struct IndexCursor<'a> {
    index: &'a Index,
    /// Path in the graph from the entry point
    /// Each item is a single type and optionally a path into the declarations and their dependencies
    /// e.g. (321, Some((2, 4))) is the 4th dependency in the 2nd declaration of type #321
    path: Vec<Segment>,
    /// Set of types we have seen already, to prevent revisiting types
    visited_types: HashSet<TypeId>,
}

enum Segment {
    Type(TypeId),
    Declaration(usize),
    Dependency(usize),
}

// Used to produce output like this
//
// Type AppDelegate:
// - declared in ./DemoSPMModularArchitect/App/AppDelegate.swift 12:6, using types:
//   LoginViewController (at 19:17):
//   - declared in ./DemoSPMModularArchitect/Packages/Features/UserFeature/Sources/UserFeature/LoginViewController.swift 11:13, using types:
//     BaseViewControler (at 11:34):
//     - declared in ./DemoSPMModularArchitect/Packages/CoreModules/Core/Sources/Core/BaseViewController.swift 10:11, using types:
//   Bundle (at 19:59):
//   - extended in ./DemoSPMModularArchitect/Packages/CoreModules/CoreUtils/Sources/CoreUtils/Extensions/BundleExtensions.swift 9:17, using types:

impl<'a> IndexCursor<'a> {
    pub fn new(index: &'a Index, type_id: TypeId) -> Self {
        Self {
            index,
            path: vec![Segment::Type(type_id)],
            visited_types: HashSet::new(),
        }
    }

    pub fn next_item(&mut self) -> Option<(IndexItem<'a>, usize)> {
        loop {
            let Some(top) = self.path.last() else {
                return None;
            };
            let Some(current_type) = self.current_type() else {
                return None;
            };
            let parent = self.parent_item();
            let depth = self.path.len() - 1;

            match top {
                Segment::Type(type_id) => {
                    let type_id = *type_id;

                    if self.visited_types.contains(&type_id) {
                        self.path.pop();

                        if let Some(Segment::Dependency(idx)) = self.path.pop() {
                            self.path.push(Segment::Dependency(idx + 1));
                        }
                        continue;
                    }

                    self.visited_types.insert(type_id);

                    if !current_type.declarations.is_empty() {
                        self.path.push(Segment::Declaration(0));
                    } else {
                        self.path.pop();

                        if let Some(Segment::Dependency(idx)) = self.path.pop() {
                            self.path.push(Segment::Dependency(idx + 1));
                        }
                    }

                    return Some((
                        IndexItem::Type(type_id, current_type.name.as_ref(), current_type.origin()),
                        depth,
                    ));
                }
                Segment::Declaration(idx) => {
                    let Some(declaration) = current_type.declarations.get(*idx) else {
                        // Declaration index has run over, backtrack
                        self.path.pop();
                        continue;
                    };

                    if !declaration.dependencies.is_empty() {
                        self.path.push(Segment::Dependency(0));
                    } else if current_type.declarations.len() > *idx + 1 {
                        let next_declaration_index = idx + 1;

                        self.path.pop();
                        self.path.push(Segment::Declaration(next_declaration_index));
                    } else {
                        self.path.pop();
                    }

                    return Some((IndexItem::Declaration(declaration), depth));
                }
                Segment::Dependency(idx) => {
                    let Some(Segment::Declaration(dec_idx)) = parent else {
                        unreachable!("Parent of a dependency is not a declaration!");
                    };

                    let Some(declaration) = current_type.declarations.get(*dec_idx) else {
                        unreachable!("Cannot find a declaration while visiting a dependency");
                    };

                    let Some((type_id, point)) = declaration.dependencies.get(*idx) else {
                        // Dependency index has run over, backtrack
                        let next_declaration_index = dec_idx + 1;
                        self.path.pop();
                        self.path.pop();

                        self.path.push(Segment::Declaration(next_declaration_index));
                        continue;
                    };

                    let Some(type_ref) = self.index.get_type(*type_id) else {
                        unreachable!("Cannot find type {} while visiting a dependency", type_id);
                    };

                    if !self.visited_types.contains(type_id) {
                        // Visit the type of the dependency
                        self.path.push(Segment::Type(*type_id));
                    } else {
                        let next_dependency_index = idx + 1;

                        self.path.pop();
                        self.path.push(Segment::Dependency(next_dependency_index))
                    }

                    return Some((
                        IndexItem::Dependency(*type_id, type_ref.name.as_ref(), *point),
                        depth,
                    ));
                }
            };
        }
    }

    fn current_type(&self) -> Option<&'a Type> {
        self.path.iter().rev().find_map(|it| match it {
            Segment::Type(type_id) => self.index.get_type(*type_id),
            Segment::Declaration(_) => None,
            Segment::Dependency(_) => None,
        })
    }

    fn parent_item(&self) -> Option<&Segment> {
        if self.path.len() < 2 {
            return None;
        }

        self.path.get(self.path.len() - 2)
    }
}

impl<'a> Iterator for IndexCursor<'a> {
    // An item in the index and the length of the current path from the starting type
    type Item = (IndexItem<'a>, usize);

    fn next(&mut self) -> Option<(IndexItem<'a>, usize)> {
        self.next_item()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::index::{Index, Kind};

    #[test]
    fn emits_a_single_reference() {
        let mut index = Index::new();
        index.add_reference("MyType");

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![(IndexItem::Type(0, "MyType", TypeOrigin::External), 0)];

        assert_eq!(actual, expected)
    }

    #[test]
    fn emits_type_with_one_declaration() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[],
        );

        let declaration = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(0, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration), 1),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn emits_type_with_two_declarations() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Struct,
            "./SomeFile.swift",
            Point::new(10, 20),
            &[],
        );
        let declaration = Declaration {
            kind: Kind::Struct,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![],
        };

        index.add_declaration(
            "MyType",
            Kind::Extension,
            "./SomeOtherFile.swift",
            Point::new(5, 10),
            &[],
        );
        let extension = Declaration {
            kind: Kind::Extension,
            point: Point::new(5, 10),
            file: 1,
            dependencies: vec![],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(0, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration), 1),
            (IndexItem::Declaration(&extension), 1),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn emits_type_with_one_declaration_and_an_unknown_dependency() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[("OtherType", &Point::new(3, 10))],
        );

        let declaration = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![(0, Point::new(3, 10))],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(1, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration), 1),
            (IndexItem::Dependency(0, "OtherType", Point::new(3, 10)), 2),
            (IndexItem::Type(0, "OtherType", TypeOrigin::External), 3),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn emits_type_with_one_declaration_and_a_known_dependency() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[("OtherType", &Point::new(3, 10))],
        );
        index.add_declaration(
            "OtherType",
            Kind::Struct,
            "./OtherType.swift",
            Point::new(10, 20),
            &[],
        );

        let other_type_id = index.type_id("OtherType").unwrap();
        let declaration_1 = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![(other_type_id, Point::new(3, 10))],
        };
        let declaration_2 = Declaration {
            kind: Kind::Struct,
            point: Point::new(10, 20),
            file: 1,
            dependencies: vec![],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(1, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration_1), 1),
            (IndexItem::Dependency(0, "OtherType", Point::new(3, 10)), 2),
            (IndexItem::Type(0, "OtherType", TypeOrigin::Local), 3),
            (IndexItem::Declaration(&declaration_2), 4),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn emits_type_with_one_declaration_and_multiple_unknown_dependencies() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[
                ("OtherType", &Point::new(3, 10)),
                ("YetAnotherType", &Point::new(7, 10)),
            ],
        );

        let other_type_id = index.type_id("OtherType").unwrap();
        let yet_another_type_id = index.type_id("YetAnotherType").unwrap();
        let declaration = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![
                (other_type_id, Point::new(3, 10)),
                (yet_another_type_id, Point::new(7, 10)),
            ],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(2, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration), 1),
            (IndexItem::Dependency(0, "OtherType", Point::new(3, 10)), 2),
            (IndexItem::Type(0, "OtherType", TypeOrigin::External), 3),
            (
                IndexItem::Dependency(1, "YetAnotherType", Point::new(7, 10)),
                2,
            ),
            (
                IndexItem::Type(1, "YetAnotherType", TypeOrigin::External),
                3,
            ),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn walks_a_straight_path_with_unknown_references() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[
                ("ExternalType", &Point::new(3, 10)),
                ("OtherType", &Point::new(8, 10)),
            ],
        );
        index.add_declaration(
            "OtherType",
            Kind::Struct,
            "./OtherType.swift",
            Point::new(10, 20),
            &[
                ("ExternalType", &Point::new(4, 10)),
                ("OneMoreType", &Point::new(5, 10)),
                ("AnotherExternalType", &Point::new(6, 10)),
            ],
        );
        index.add_declaration(
            "OneMoreType",
            Kind::Struct,
            "./OneMoreType.swift",
            Point::new(10, 20),
            &[("AnotherExternalType", &Point::new(6, 10))],
        );

        let declaration_1 = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![
                (index.type_id("ExternalType").unwrap(), Point::new(3, 10)),
                (index.type_id("OtherType").unwrap(), Point::new(8, 10)),
            ],
        };
        let declaration_2 = Declaration {
            kind: Kind::Struct,
            point: Point::new(10, 20),
            file: 1,
            dependencies: vec![
                (index.type_id("ExternalType").unwrap(), Point::new(4, 10)),
                (index.type_id("OneMoreType").unwrap(), Point::new(5, 10)),
                (
                    index.type_id("AnotherExternalType").unwrap(),
                    Point::new(6, 10),
                ),
            ],
        };
        let declaration_3 = Declaration {
            kind: Kind::Struct,
            point: Point::new(10, 20),
            file: 2,
            dependencies: vec![(
                index.type_id("AnotherExternalType").unwrap(),
                Point::new(6, 10),
            )],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(2, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration_1), 1),
            (
                IndexItem::Dependency(0, "ExternalType", Point::new(3, 10)),
                2,
            ),
            (IndexItem::Type(0, "ExternalType", TypeOrigin::External), 3),
            (IndexItem::Dependency(1, "OtherType", Point::new(8, 10)), 2),
            (IndexItem::Type(1, "OtherType", TypeOrigin::Local), 3),
            (IndexItem::Declaration(&declaration_2), 4),
            (
                IndexItem::Dependency(0, "ExternalType", Point::new(4, 10)),
                5,
            ),
            (
                IndexItem::Dependency(3, "OneMoreType", Point::new(5, 10)),
                5,
            ),
            (IndexItem::Type(3, "OneMoreType", TypeOrigin::Local), 6),
            (IndexItem::Declaration(&declaration_3), 7),
            (
                IndexItem::Dependency(4, "AnotherExternalType", Point::new(6, 10)),
                8,
            ),
            (
                IndexItem::Type(4, "AnotherExternalType", TypeOrigin::External),
                9,
            ),
            (
                IndexItem::Dependency(4, "AnotherExternalType", Point::new(6, 10)),
                5,
            ),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn walks_a_tree_with_unknown_references() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[("OtherType", &Point::new(8, 10))],
        );
        index.add_declaration(
            "MyType",
            Kind::Extension,
            "./Extension.swift",
            Point::new(12, 20),
            &[
                ("ExternalType", &Point::new(4, 10)),
                ("OneMoreType", &Point::new(5, 10)),
                ("AnotherExternalType", &Point::new(6, 10)),
            ],
        );
        index.add_declaration(
            "OneMoreType",
            Kind::Struct,
            "./OneMoreType.swift",
            Point::new(10, 20),
            &[("ExternalType", &Point::new(6, 10))],
        );

        let declaration_1 = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![(index.type_id("OtherType").unwrap(), Point::new(8, 10))],
        };
        let declaration_2 = Declaration {
            kind: Kind::Extension,
            point: Point::new(12, 20),
            file: 1,
            dependencies: vec![
                (index.type_id("ExternalType").unwrap(), Point::new(4, 10)),
                (index.type_id("OneMoreType").unwrap(), Point::new(5, 10)),
                (
                    index.type_id("AnotherExternalType").unwrap(),
                    Point::new(6, 10),
                ),
            ],
        };
        let declaration_3 = Declaration {
            kind: Kind::Struct,
            point: Point::new(10, 20),
            file: 2,
            dependencies: vec![(index.type_id("ExternalType").unwrap(), Point::new(6, 10))],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(1, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration_1), 1),
            (IndexItem::Dependency(0, "OtherType", Point::new(8, 10)), 2),
            (IndexItem::Type(0, "OtherType", TypeOrigin::External), 3),
            (IndexItem::Declaration(&declaration_2), 1),
            (
                IndexItem::Dependency(2, "ExternalType", Point::new(4, 10)),
                2,
            ),
            (IndexItem::Type(2, "ExternalType", TypeOrigin::External), 3),
            (
                IndexItem::Dependency(3, "OneMoreType", Point::new(5, 10)),
                2,
            ),
            (IndexItem::Type(3, "OneMoreType", TypeOrigin::Local), 3),
            (IndexItem::Declaration(&declaration_3), 4),
            (
                IndexItem::Dependency(2, "ExternalType", Point::new(6, 10)),
                5,
            ),
            (
                IndexItem::Dependency(4, "AnotherExternalType", Point::new(6, 10)),
                2,
            ),
            (
                IndexItem::Type(4, "AnotherExternalType", TypeOrigin::External),
                3,
            ),
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn walks_a_graph_ignoring_back_edges() {
        let mut index = Index::new();
        index.add_declaration(
            "MyType",
            Kind::Enum,
            "./MyType.swift",
            Point::new(10, 20),
            &[
                ("ExternalType", &Point::new(7, 10)),
                ("OtherType", &Point::new(3, 10)),
            ],
        );
        index.add_declaration(
            "OtherType",
            Kind::Enum,
            "./OtherType.swift",
            Point::new(10, 20),
            &[
                ("MyType", &Point::new(7, 10)),
                ("ExternalType", &Point::new(3, 10)),
            ],
        );

        let declaration_1 = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 0,
            dependencies: vec![
                (index.type_id("ExternalType").unwrap(), Point::new(7, 10)),
                (index.type_id("OtherType").unwrap(), Point::new(3, 10)),
            ],
        };

        let declaration_2 = Declaration {
            kind: Kind::Enum,
            point: Point::new(10, 20),
            file: 1,
            dependencies: vec![
                (index.type_id("MyType").unwrap(), Point::new(7, 10)),
                (index.type_id("ExternalType").unwrap(), Point::new(3, 10)),
            ],
        };

        let actual: Vec<_> = index.walk("MyType").unwrap().collect();
        let expected = vec![
            (IndexItem::Type(2, "MyType", TypeOrigin::Local), 0),
            (IndexItem::Declaration(&declaration_1), 1),
            (
                IndexItem::Dependency(0, "ExternalType", Point::new(7, 10)),
                2,
            ),
            (IndexItem::Type(0, "ExternalType", TypeOrigin::External), 3),
            (IndexItem::Dependency(1, "OtherType", Point::new(3, 10)), 2),
            (IndexItem::Type(1, "OtherType", TypeOrigin::Local), 3),
            (IndexItem::Declaration(&declaration_2), 4),
            (IndexItem::Dependency(2, "MyType", Point::new(7, 10)), 5),
            (
                IndexItem::Dependency(0, "ExternalType", Point::new(3, 10)),
                5,
            ),
        ];

        assert_eq!(actual, expected);
    }
}
