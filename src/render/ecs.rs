use crate::render::components::{Layer, Rect, Renderable};
use std::collections::{HashMap, HashSet};

/// Unique identifier for an entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub(crate) u64);

#[derive(Debug, Clone)]
struct Entry<T> {
    data: T,
    version: u64,
}

/// Generic storage for components
#[derive(Debug, Clone)]
pub struct ComponentStorage<T> {
    components: HashMap<EntityId, Entry<T>>,
}

impl<T> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self {
            components: HashMap::new(),
        }
    }
}

impl<T: PartialEq> ComponentStorage<T> {
    pub fn insert(&mut self, entity: EntityId, component: T, version: u64) {
        match self.components.get_mut(&entity) {
            Some(entry) => {
                if entry.data != component {
                    entry.data = component;
                    entry.version = version;
                }
            }
            None => {
                self.components.insert(
                    entity,
                    Entry {
                        data: component,
                        version,
                    },
                );
            }
        }
    }
}

impl<T> ComponentStorage<T> {
    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.components.get(&entity).map(|e| &e.data)
    }

    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        self.components.get_mut(&entity).map(|e| &mut e.data)
    }

    pub fn get_version(&self, entity: EntityId) -> Option<u64> {
        self.components.get(&entity).map(|e| e.version)
    }

    pub fn remove(&mut self, entity: EntityId) -> Option<T> {
        self.components.remove(&entity).map(|e| e.data)
    }

    pub fn clear(&mut self) {
        self.components.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&EntityId, &T)> {
        self.components.iter().map(|(id, entry)| (id, &entry.data))
    }
}

/// The World that holds all entities and components
///
/// This is a persistent world that tracks versions of components.
#[derive(Debug)]
pub struct World {
    next_entity_id: u64,
    entities: HashSet<EntityId>,
    pub current_version: u64,

    // Component Storages
    pub renderables: ComponentStorage<Renderable>,
    pub rects: ComponentStorage<Rect>,
    pub layers: ComponentStorage<Layer>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            next_entity_id: 0,
            entities: HashSet::new(),
            current_version: 1, // Start at 1
            renderables: ComponentStorage::default(),
            rects: ComponentStorage::default(),
            layers: ComponentStorage::default(),
        }
    }
}

impl World {
    /// Create a new empty world
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the world completely (resetting versions too)
    pub fn clear(&mut self) {
        self.next_entity_id = 0;
        self.current_version = 1;
        self.entities.clear();
        self.renderables.clear();
        self.rects.clear();
        self.layers.clear();
    }

    /// Advance the world version, signifying a new frame/update cycle
    pub fn tick(&mut self) {
        self.current_version += 1;
    }

    /// Create a new entity
    pub fn create_entity(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id += 1;
        self.entities.insert(id);
        id
    }

    /// Destroy an entity and all its components
    pub fn destroy_entity(&mut self, entity: EntityId) {
        if self.entities.remove(&entity) {
            self.renderables.remove(entity);
            self.rects.remove(entity);
            self.layers.remove(entity);
        }
    }

    /// Helper to add a renderable component
    pub fn add_renderable(&mut self, entity: EntityId, renderable: Renderable) {
        if self.entities.contains(&entity) {
            self.renderables
                .insert(entity, renderable, self.current_version);
        }
    }

    /// Helper to add a rect component
    pub fn add_rect(&mut self, entity: EntityId, rect: Rect) {
        if self.entities.contains(&entity) {
            self.rects.insert(entity, rect, self.current_version);
        }
    }

    /// Helper to add a layer component
    pub fn add_layer(&mut self, entity: EntityId, layer: Layer) {
        if self.entities.contains(&entity) {
            self.layers.insert(entity, layer, self.current_version);
        }
    }

    /// Get all entities
    pub fn entities(&self) -> &HashSet<EntityId> {
        &self.entities
    }
}
