use super::serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::convert::AsRef;
use std::{error, fmt, mem, ops};

// TODO update / rename elements semantics

/// Index for graph elements.
pub type Index = usize;

/// Graph operation errors.
#[derive(Debug)]
pub enum Error {
    InvalidIndex,
    CannotRemoveLinked,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::InvalidIndex => "invalid index".fmt(f),
            Error::CannotRemoveLinked => "cannot remove a referenced object".fmt(f),
        }
    }
}
impl error::Error for Error {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub enum Atom {
    Text(String),
    Integer(i32),
}
impl Atom {
    pub fn text<T: Into<String>>(text: T) -> Self {
        Atom::Text(text.into())
    }
}
impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Atom::Text(ref s) => s.fmt(f),
            Atom::Integer(i) => i.fmt(f),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Link {
    pub from: Index,
    pub to: Index,
}
impl Link {
    pub fn new(from: Index, to: Index) -> Self {
        Link { from: from, to: to }
    }
}
impl From<(Index, Index)> for Link {
    fn from(pair: (Index, Index)) -> Link {
        Link::new(pair.0, pair.1)
    }
}

/** Object of the graph:
 * All objects are identified by their index in the graph (which is constant after creation).
 * All objects can be pointed to/from by a link.
 *
 * Atom: a basic piece of concrete data.
 * Must be hashmap compatible (comparable).
 * In a graph, atoms are unique, and can be searched from their value.
 *
 * Link: a directed arrow between two graph objects.
 * Links are also unique and can be searched from their value.
 * Links can link any two elements of the graph (no restriction).
 * It is up to the user to give semantics to a link.
 * A common pattern is to "annotate a link with an atom" with an atom representing a relation type.
 * It consists of creating another link from the atom to the annotated link.
 *
 * Abstract: abstract graph object with no data.
 * Abstract objects are exclusively defined by their links (relations with other objects).
 * They are not comparable, and must be searched by pattern matching of their relation.
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Object {
    Atom(Atom),
    Link(Link),
    Abstract,
}

/** Data for each graph object.
 * In addition to the object, store local topology for fast traversal.
 * in_links/out_links: indexes of links pointing to/from this link.
 * Description: raw text field that is not part of topology.
 */
#[derive(Serialize, Deserialize)]
struct ObjectData {
    object: Object,
    description: String,
    #[serde(skip)]
    in_links: Vec<Index>,
    #[serde(skip)]
    out_links: Vec<Index>,
}
impl ObjectData {
    fn new(object: Object) -> Self {
        ObjectData {
            object: object,
            description: String::new(),
            in_links: Vec::new(),
            out_links: Vec::new(),
        }
    }
}

pub struct Graph {
    objects: Vec<Option<ObjectData>>,
    atom_indexes: HashMap<Atom, Index>,
    link_indexes: HashMap<Link, Index>,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            objects: Vec::new(),
            atom_indexes: HashMap::new(),
            link_indexes: HashMap::new(),
        }
    }

    pub fn valid(&self, index: Index) -> bool {
        index < self.objects.len() && self.objects[index].is_some()
    }
    pub fn get_object<'a>(&'a self, index: Index) -> Result<ObjectRef<'a>, Error> {
        match self.objects.get(index) {
            Some(&Some(ref object_data)) => Ok(ObjectRef {
                index: index,
                object_data: object_data,
                graph: self,
            }),
            _ => Err(Error::InvalidIndex),
        }
    }
    pub fn object<'a>(&'a self, index: Index) -> ObjectRef<'a> {
        self.get_object(index).unwrap()
    }

    /// Iterate on valid objects
    pub fn objects<'a>(&'a self) -> OrderedObjectIterator<'a> {
        OrderedObjectIterator {
            next_index: 0,
            graph: self,
        }
    }

    pub fn get_atom_index(&self, atom: &Atom) -> Option<Index> {
        self.atom_indexes.get(&atom).cloned()
    }
    pub fn get_link_index(&self, link: &Link) -> Option<Index> {
        self.link_indexes.get(&link).cloned()
    }
    pub fn get_atom<'a>(&'a self, atom: &Atom) -> Option<ObjectRef<'a>> {
        self.get_atom_index(atom).map(|i| self.object(i))
    }
    pub fn get_link<'a>(&'a self, link: &Link) -> Option<ObjectRef<'a>> {
        self.get_link_index(link).map(|i| self.object(i))
    }

    pub fn set_description(&mut self, index: Index, text: String) -> Result<(), Error> {
        if self.valid(index) {
            self.objects[index].as_mut().unwrap().description = text;
            Ok(())
        } else {
            Err(Error::InvalidIndex)
        }
    }

    /// Get the index of an atom, inserting it if not found.
    pub fn use_atom(&mut self, atom: Atom) -> Index {
        match self.get_atom_index(&atom) {
            Some(index) => index,
            None => {
                let new_index = self.insert_object(Object::Atom(atom.clone()));
                self.register_atom(new_index, atom);
                new_index
            }
        }
    }
    /// Get the index of an atom, inserting it if not found.
    pub fn use_link(&mut self, link: Link) -> Result<Index, Error> {
        if self.valid(link.from) && self.valid(link.to) {
            match self.get_link_index(&link) {
                Some(index) => Ok(index),
                None => {
                    let new_index = self.insert_object(Object::Link(link.clone()));
                    self.register_link(new_index, link);
                    Ok(new_index)
                }
            }
        } else {
            Err(Error::InvalidIndex)
        }
    }
    /// Create a new abstract object, return its index.
    pub fn create_abstract(&mut self) -> Index {
        self.insert_object(Object::Abstract)
    }

    /// Delete object
    pub fn remove_object(&mut self, index: Index) -> Result<(), Error> {
        {
            // Filter: valid objects which are not linked
            let object = self.get_object(index)?;
            if object.is_link() && !(object.in_links().is_empty() && object.out_links().is_empty())
            {
                return Err(Error::CannotRemoveLinked);
            }
        }
        let object_data = mem::replace(&mut self.objects[index], None).unwrap();
        match object_data.object {
            Object::Atom(ref a) => {
                self.atom_indexes.remove_entry(a);
            }
            Object::Link(ref l) => {
                self.link_indexes.remove_entry(l);
                let p = |i: &Index| *i != index;
                self.objects[l.from].as_mut().unwrap().out_links.retain(p);
                self.objects[l.to].as_mut().unwrap().in_links.retain(p);
            }
            Object::Abstract => (),
        }
        Ok(())
    }

    fn insert_object(&mut self, object: Object) -> Index {
        // Find unused index
        for index in 0..self.objects.len() {
            let mut cell = &mut self.objects[index];
            if cell.is_none() {
                *cell = Some(ObjectData::new(object));
                return index;
            }
        }
        // Or allocate new one
        let index = self.objects.len();
        self.objects.push(Some(ObjectData::new(object)));
        index
    }
    fn register_atom(&mut self, index: Index, atom: Atom) {
        let old = self.atom_indexes.insert(atom, index);
        assert_eq!(old, None);
    }
    fn register_link(&mut self, index: Index, link: Link) {
        self.objects[link.from]
            .as_mut()
            .unwrap()
            .out_links
            .push(index);
        self.objects[link.to].as_mut().unwrap().in_links.push(index);
        let old = self.link_indexes.insert(link, index);
        assert_eq!(old, None);
    }
}

/// Reference to link from/to as ObjectRef.
#[derive(Clone, Copy)]
pub struct LinkRef<'a> {
    pub from: ObjectRef<'a>,
    pub to: ObjectRef<'a>,
}

/// Reference an object and its data. Has AsRef and Deref to behave like an Object.
#[derive(Clone, Copy)]
pub struct ObjectRef<'a> {
    index: Index,
    object_data: &'a ObjectData,
    graph: &'a Graph,
}
impl<'a> ObjectRef<'a> {
    pub fn index(&self) -> Index {
        self.index
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    pub fn description(&self) -> &str {
        &self.object_data.description
    }
    pub fn as_link(&self) -> Option<LinkRef<'a>> {
        match self.object_data.object {
            Object::Link(ref l) => Some(LinkRef {
                from: self.graph.object(l.from),
                to: self.graph.object(l.to),
            }),
            _ => None,
        }
    }
    pub fn in_links_index(&self) -> &[Index] {
        &self.object_data.in_links
    }
    pub fn out_links_index(&self) -> &[Index] {
        &self.object_data.out_links
    }
    pub fn in_links(&self) -> ObjectRefSlice<'a> {
        ObjectRefSlice {
            indexes: &self.object_data.in_links,
            graph: self.graph,
        }
    }
    pub fn out_links(&self) -> ObjectRefSlice<'a> {
        ObjectRefSlice {
            indexes: &self.object_data.out_links,
            graph: self.graph,
        }
    }
}
impl<'a> AsRef<Object> for ObjectRef<'a> {
    fn as_ref(&self) -> &Object {
        &self.object_data.object
    }
}
impl<'a> ops::Deref for ObjectRef<'a> {
    type Target = Object;
    fn deref(&self) -> &Object {
        &self.object_data.object
    }
}

impl Object {
    pub fn is_atom(&self) -> bool {
        match *self {
            Object::Atom(_) => true,
            _ => false,
        }
    }
    pub fn is_link(&self) -> bool {
        match *self {
            Object::Link(_) => true,
            _ => false,
        }
    }
    pub fn is_abstract(&self) -> bool {
        match *self {
            Object::Abstract => true,
            _ => false,
        }
    }
}

/// Iterate on objects in order of increasing indexes.
pub struct OrderedObjectIterator<'a> {
    next_index: usize,
    graph: &'a Graph,
}
impl<'a> Iterator for OrderedObjectIterator<'a> {
    type Item = ObjectRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let current_index = self.next_index;
            if current_index >= self.graph.objects.len() {
                return None;
            };
            self.next_index = current_index + 1;
            if let Ok(object_ref) = self.graph.get_object(current_index) {
                return Some(object_ref);
            }
        }
    }
}

/// Slice of link refs (in/out links are always links).
#[derive(Clone, Copy)]
pub struct ObjectRefSlice<'a> {
    indexes: &'a [Index],
    graph: &'a Graph,
}
impl<'a> ObjectRefSlice<'a> {
    pub fn len(&self) -> usize {
        self.indexes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }
    pub fn at(&self, i: usize) -> ObjectRef<'a> {
        self.graph.object(self.indexes[i])
    }
    pub fn first(&self) -> Option<ObjectRef<'a>> {
        if self.indexes.len() > 0 {
            Some(self.at(0))
        } else {
            None
        }
    }
}

/// Iteration capability for object ref slice.
impl<'a> IntoIterator for ObjectRefSlice<'a> {
    type Item = ObjectRef<'a>;
    type IntoIter = ObjectRefSliceIterator<'a>;
    fn into_iter(self) -> Self::IntoIter {
        ObjectRefSliceIterator { i: 0, slice: self }
    }
}
pub struct ObjectRefSliceIterator<'a> {
    i: usize,
    slice: ObjectRefSlice<'a>,
}
impl<'a> Iterator for ObjectRefSliceIterator<'a> {
    type Item = ObjectRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let i = self.i;
        if i < self.slice.len() {
            self.i += 1;
            Some(self.slice.at(i))
        } else {
            None
        }
    }
}

/******************************************************************************
 * IO using serde.
 * The graph is serialized as a sequence of Option<ObjectData>.
 * ObjectData only contains object and description when serialized.
 * Atoms, topology and indexes are conserved.
 */
impl Serialize for Graph {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.objects.serialize(serializer)
    }
}

impl<'d> Deserialize<'d> for Graph {
    fn deserialize<D: Deserializer<'d>>(deserializer: D) -> Result<Self, D::Error> {
        let mut graph = Graph::new();
        graph.objects = Vec::<Option<ObjectData>>::deserialize(deserializer)?;

        // Restore in_links/out_links, maps, and validate
        for index in 0..graph.objects.len() {
            let maybe_object: Option<Object> = graph.objects[index]
                .as_ref()
                .map(|obj_data| obj_data.object.clone());
            match maybe_object {
                Some(Object::Atom(atom)) => graph.register_atom(index, atom),
                Some(Object::Link(link)) => {
                    if !(graph.valid(link.from) && graph.valid(link.to)) {
                        use serde::de::Error;
                        return Err(D::Error::custom(format!(
                            "link at index {} holds an invalid graph index",
                            index
                        )));
                    }
                    graph.register_link(index, link)
                }
                _ => (),
            }
        }
        Ok(graph)
    }
}

/******************************************************************************
 * Tests.
 */
#[cfg(test)]
mod tests {
    use super::super::serde_json;
    use super::*;

    // This equality operator is for test only. Abstract objects are supposed to be non-comparable.
    impl PartialEq for Object {
        fn eq(&self, other: &Object) -> bool {
            match (self, other) {
                (Object::Atom(ref l), Object::Atom(ref r)) => l == r,
                (Object::Link(ref l), Object::Link(ref r)) => l == r,
                (Object::Abstract, Object::Abstract) => true,
                _ => false,
            }
        }
    }

    #[test]
    fn io() {
        // Dummy graph
        let mut graph = Graph::new();
        let i0 = graph.create_abstract();
        let i1 = graph.use_atom(Atom::text("Abstract"));
        let i2 = graph.use_link(Link::new(i1, i0)).unwrap();
        let _i3 = graph.use_link(Link::new(i0, i2)).unwrap();
        // Serialize and deserialize
        let serialized = serde_json::to_string(&graph).expect("Serialization failure");
        let deserialized: Graph =
            serde_json::from_str(&serialized).expect("Deserialization failure");
        // Compare
        for object in graph.objects() {
            let deserialized_object = deserialized.object(object.index());
            assert_eq!(*object, *deserialized_object);
        }
    }
}
