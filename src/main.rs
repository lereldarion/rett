extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
// TODO impl serde support for graph (direct)

///*****************************************************************************
/// A sparse vector, where objects are accessed by indexes.
mod slot_vec {
    use std::mem;
    use std::ops;

    enum Slot<T> {
        Used(T),
        Unused(Option<usize>), // Element of a free list of unused Slots
    }
    pub struct SlotVec<T> {
        slots: Vec<Slot<T>>,
        next_unused_slot_id: Option<usize>, // Free list head
        nb_objects: usize,
    }

    impl<T> SlotVec<T> {
        /// Create an empty SlotVec.
        pub fn new() -> Self {
            SlotVec {
                slots: Vec::new(),
                next_unused_slot_id: None,
                nb_objects: 0,
            }
        }

        /// Number of stored objects.
        pub fn len(&self) -> usize {
            self.nb_objects
        }
        /// Number of slots (and maximum index).
        pub fn nb_slots(&self) -> usize {
            self.slots.len()
        }

        /// access a slot (returns none if empty slot).
        pub fn get(&self, index: usize) -> Option<&T> {
            match self.slots[index] {
                Slot::Used(ref value) => Some(value),
                _ => None,
            }
        }
        /// access a slot (returns none if empty slot): mut version.
        pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
            match self.slots[index] {
                Slot::Used(ref mut value) => Some(value),
                _ => None,
            }
        }

        /// Insert an object in any slot, returns the new object index.
        pub fn insert(&mut self, value: T) -> usize {
            let new_id = {
                if let Some(unused_id) = self.next_unused_slot_id {
                    // Pop unused slot from free list
                    let unused_slot = &mut self.slots[unused_id];
                    if let Slot::Unused(next_unused_slot_id) = *unused_slot {
                        self.next_unused_slot_id = next_unused_slot_id;
                        *unused_slot = Slot::Used(value);
                        unused_id
                    } else {
                        panic!("Used Slot in free list");
                    }
                } else {
                    // Allocate new slot
                    let end_of_vec_id = self.nb_slots();
                    self.slots.push(Slot::Used(value));
                    end_of_vec_id
                }
            };
            self.nb_objects += 1;
            new_id
        }
        /// Remove the object at the given index. Return the object that was removed.
        pub fn remove(&mut self, index: usize) -> Option<T> {
            let slot = &mut self.slots[index];
            if let Slot::Used(_) = *slot {
                let old_next_unused_slot_id =
                    mem::replace(&mut self.next_unused_slot_id, Some(index));
                let old_value = match mem::replace(slot, Slot::Unused(old_next_unused_slot_id)) {
                    Slot::Used(value) => value,
                    _ => panic!("Slot was used"),
                };
                self.nb_objects -= 1;
                Some(old_value)
            } else {
                None
            }
        }
    }

    /// Indexation with []: panics on invalid index.
    impl<T> ops::Index<usize> for SlotVec<T> {
        type Output = T;
        fn index(&self, index: usize) -> &T {
            self.get(index).expect("invalid index")
        }
    }
    impl<T> ops::IndexMut<usize> for SlotVec<T> {
        fn index_mut(&mut self, index: usize) -> &mut T {
            self.get_mut(index).expect("invalid index")
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn basic_api() {
            let mut sv = super::SlotVec::new();
            assert_eq!(sv.len(), 0);
            assert_eq!(sv.nb_slots(), 0);

            let id_42 = sv.insert(42);
            assert_eq!(sv.get(id_42), Some(&42));
            assert_eq!(sv[id_42], 42);
            assert_eq!(sv.len(), 1);
            assert_eq!(sv.nb_slots(), 1);

            let id_12 = sv.insert(12);
            assert_ne!(id_42, id_12);
            assert_eq!(sv.len(), 2);
            assert_eq!(sv.nb_slots(), 2);

            assert_eq!(sv.remove(id_42), Some(42));
            assert_eq!(sv.len(), 1);
            assert_eq!(sv.get(id_42), None);
            assert_eq!(sv.nb_slots(), 2);

            // Check reuse
            let id_34 = sv.insert(34);
            assert_eq!(id_34, id_42);
            assert_ne!(id_42, id_12);
            assert_eq!(sv.nb_slots(), 2);

            sv[id_34] = 0;
        }
    }
}

///*****************************************************************************
/// Define a knowledge graph
mod graph {
    use std::hash::Hash;
    use std::collections::HashMap;
    use slot_vec::SlotVec;

    /// Opaque Index type for graph elements
    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize, Debug)]
    pub struct Index(usize);

    /// A directed link (edge of the graph)
    #[derive(PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
    pub struct Link {
        pub from: Index,
        pub to: Index,
    }

    /// An abstract graph entity (node of the graph).
    /// Defined only by its relationships: not comparable.
    #[derive(Clone, Serialize, Deserialize, Debug)]
    pub struct Entity;

    /// Object of the graph: Link, Entity, or Atom (parametrized).
    #[derive(Clone, Serialize, Deserialize)]
    pub enum Object<A> {
        Atom(A),
        Link(Link),
        Entity(Entity),
    }

    /// Data for each object.
    /// In addition to the object, stored ids of links pointing from/to the object.
    struct ObjectData<A> {
        object: Object<A>,
        in_links: Vec<Index>,
        out_links: Vec<Index>,
    }

    /// Store a graph.
    pub struct Graph<A> {
        objects: SlotVec<ObjectData<A>>,
        atom_indexes: HashMap<A, Index>,
        link_indexes: HashMap<Link, Index>,
    }

    /// Reference an object and its data
    pub struct ObjectRef<'a, A: 'a> {
        index: Index,
        object_data: &'a ObjectData<A>,
    }

    impl Index {
        /// Access (read only) the underlying value.
        pub fn to_usize(&self) -> usize {
            self.0
        }
    }

    impl<A> ObjectData<A> {
        fn new(object: Object<A>) -> Self {
            ObjectData {
                object: object,
                in_links: Vec::new(),
                out_links: Vec::new(),
            }
        }
    }

    impl<'a, A> ObjectRef<'a, A> {
        pub fn index(&self) -> Index {
            self.index
        }
        pub fn object(&self) -> &'a Object<A> {
            &self.object_data.object
        }
    }

    impl<A: Eq + Hash + Clone> Graph<A> {
        /// Create a new empty graph.
        pub fn new() -> Self {
            Graph {
                objects: SlotVec::new(),
                atom_indexes: HashMap::new(),
                link_indexes: HashMap::new(),
            }
        }

        /// Get object
        pub fn object<'a>(&'a self, index: Index) -> Option<ObjectRef<'a, A>> {
            self.objects
                .get(index.to_usize())
                .map(|object_data| ObjectRef {
                    index: index,
                    object_data: object_data,
                })
        }

        /// Get index of an atom, or None if not found.
        pub fn index_of_atom(&self, atom: &A) -> Option<Index> {
            self.atom_indexes.get(&atom).map(|index_ref| *index_ref)
        }
        /// Get index of a link, or None if not found.
        pub fn index_of_link(&self, link: &Link) -> Option<Index> {
            self.link_indexes.get(&link).map(|index_ref| *index_ref)
        }

        /// Insert a new atom, return its index.
        /// If already present, only return the current index for the atom.
        pub fn insert_atom(&mut self, atom: A) -> Index {
            match self.index_of_atom(&atom) {
                Some(index) => index,
                None => {
                    let new_index = Index(
                        self.objects
                            .insert(ObjectData::new(Object::Atom(atom.clone()))),
                    );
                    self.atom_indexes.insert(atom, new_index);
                    new_index
                }
            }
        }
        /// Insert a new link, return its index.
        /// If already present, only return the current index for the link.
        pub fn insert_link(&mut self, from: Index, to: Index) -> Index {
            let link = Link { from: from, to: to }; // TODO improve ?
            match self.index_of_link(&link) {
                Some(index) => index,
                None => {
                    let new_index = Index(
                        self.objects
                            .insert(ObjectData::new(Object::Link(link.clone()))),
                    );
                    self.objects[link.from.to_usize()].out_links.push(new_index);
                    self.objects[link.to.to_usize()].in_links.push(new_index);
                    self.link_indexes.insert(link, new_index);
                    new_index
                }
            }
        }
        /// Insert a new entity. Return its index.
        pub fn insert_entity(&mut self) -> Index {
            Index(self.objects.insert(ObjectData::new(Object::Entity(Entity))))
        }
    }

    // FIXME tmp
    pub fn make_index(i: usize) -> Index {
        Index(i)
    }
    pub fn max_index<A>(g: &Graph<A>) -> usize {
        g.objects.nb_slots()
    }
}

///*****************************************************************************
/// Atom: represents a basic piece of data (integer, string, etc)
#[derive(PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
enum Atom {
    String(String),
    Integer(i32),
}
impl<'a> From<&'a str> for Atom {
    fn from(text: &'a str) -> Self {
        Atom::String(String::from(text))
    }
}

type Graph = graph::Graph<Atom>;
use graph::Object;

///*****************************************************************************
/// Output graph as dot.
fn output_as_dot(g: &Graph) {
    use std::collections::HashMap;
    use std::fmt;

    /* Link arrow color selection.
     *
     * Links are encoded as two consecutive arrows in Graphviz, with a dummy node in the middle.
     * To improve readability, each link is given an arrow color different from neighbor links.
     * This color is used for both Graphviz arrows.
     *
     * Colors are selected from a fixed palette (generating one is complex).
     * This should be sufficient as theere are few conflicts in practice.
     *
     * Colors are chosen by a greedy algorithm:
     * color (link) = min unused index among lower index link neighbors.
     */
    let color_palette = [
        "#332288", "#88CCEE", "#44AA99", "#117733", "#999933", "#DDCC77", "#CC6677", "#882255",
        "#AA4499",
    ];

    /* TODO
    let mut link_color_indexes = HashMap::new();
    let mut nb_colors = 0;

    // Step 2
    let mut link_color_indexes = HashMap::new();
    for id in objects
        .into_iter()
        .filter_map(|(id, ref elem)| if elem.is_link() { Some(id) } else { None })
    {
        let color_index = match lower_index_neighbors.get(&id) {
            Some(ref link_neighbors) => {
                // Get colors of all neighbors of lower indexes
                let neighbour_color_indexes = link_neighbors
                    .iter()
                    .map(|n| link_color_indexes[n])
                    .collect::<Vec<_>>();
                // Select first unused color index
                let mut color_index = 0;
                while neighbour_color_indexes.contains(&color_index) {
                    color_index += 1
                }
                color_index
            }
            None => 0,
        };
        nb_colors = max(nb_colors, color_index + 1);
        link_color_indexes.insert(id, color_index);
    }

    // Step 3
    assert!(
        nb_colors <= color_palette.len(),
        "output_as_dot: nb_colors = {} exceeds the color palette size ({})",
        nb_colors,
        color_palette.len()
    );*/

    // Print graph
    impl fmt::Display for graph::Index {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.to_usize().fmt(f)
        }
    }
    impl fmt::Display for Atom {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self {
                &Atom::String(ref s) => write!(f, "\\\"{}\\\"", s),
                &Atom::Integer(i) => i.fmt(f),
            }
        }
    }
    println!("digraph {{");
    for index in 0..graph::max_index(&g) {
        if let Some(object_data) = g.object(graph::make_index(index)) {
            match object_data.object() {
                &Object::Atom(ref a) => {
                    println!("\t{0} [shape=box,label=\"{0} = {1}\"];", index, a);
                }
                &Object::Link(ref link) => {
                    println!(
                    "\t{0} [shape=none,fontcolor=grey,margin=0.02,height=0,width=0,label=\"{0}\"];",
                    index
                );
                    let color = "red"; //color_palette[link_color_indexes[&index]];
                    println!("\t{0} -> {1} [color=\"{2}\"];", link.from, index, color);
                    println!("\t{0} -> {1} [color=\"{2}\"];", index, link.to, color);
                }
                &Object::Entity(_) => {
                    println!("\t{0} [shape=box,label=\"{0}\"];", index);
                }
            }
        }
    }
    println!("}}");
}

/*******************************************************************************
 * TODO queries, with hash map for referencing
 */

/*******************************************************************************
 * Test
 */
fn create_name_prop(g: &mut Graph) -> graph::Index {
    let name_entity = g.insert_entity();
    let name_text = g.insert_atom(Atom::from("name"));
    let name_entity_description = g.insert_link(name_text, name_entity);
    let _name_entity_description_description = g.insert_link(name_entity, name_entity_description);
    name_entity
}

fn create_named_entity(g: &mut Graph, name_entity: graph::Index, text: &str) -> graph::Index {
    let entity = g.insert_entity();
    let atom = g.insert_atom(Atom::from(text));
    let link = g.insert_link(atom, entity);
    let _link_description = g.insert_link(name_entity, link);
    entity
}

fn set_test_data(g: &mut Graph) {
    let name = create_name_prop(g);

    let joe = create_named_entity(g, name, "joe");
    let bob = create_named_entity(g, name, "bob");

    let pj = create_named_entity(g, name, "pj");
    g.insert_link(pj, joe);
    g.insert_link(pj, bob);

    let fight = create_named_entity(g, name, "fight");
    let joe_in_fight = g.insert_link(joe, fight);
    let bob_in_fight = g.insert_link(bob, fight);

    let was_present = create_named_entity(g, name, "was_present");
    g.insert_link(was_present, joe_in_fight);
    g.insert_link(was_present, bob_in_fight);

    let win = create_named_entity(g, name, "win");
    g.insert_link(win, bob_in_fight);

    let date = create_named_entity(g, name, "date");
    let some_date = g.insert_atom(Atom::Integer(2018));
    let fight_date = g.insert_link(some_date, fight);
    g.insert_link(date, fight_date);
}

fn main() {
    let mut graph = Graph::new();
    set_test_data(&mut graph);
    output_as_dot(&graph);
}
