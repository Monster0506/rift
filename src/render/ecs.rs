use crate::render::components::{Layer, Rect, Renderable};
use std::collections::{HashMap, HashSet};

/// Unique identifier for an entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct EntityId(pub(crate) u64);

#[derive(Debug, Clone)]
struct Entry<T> {
    data: T,
    version: u64,
}

/// Generic storage for components
#[derive(Debug, Clone)]
pub(super) struct ComponentStorage<T> {
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
    fn insert(&mut self, entity: EntityId, component: T, version: u64) {
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
    #[must_use]
    pub(super) fn get(&self, entity: EntityId) -> Option<&T> {
        self.components.get(&entity).map(|e| &e.data)
    }

    #[must_use]
    pub(super) fn get_version(&self, entity: EntityId) -> Option<u64> {
        self.components.get(&entity).map(|e| e.version)
    }

    fn remove(&mut self, entity: EntityId) -> Option<T> {
        self.components.remove(&entity).map(|e| e.data)
    }

    fn clear(&mut self) {
        self.components.clear();
    }

    #[cfg(test)]
    pub(super) fn iter(&self) -> impl Iterator<Item = (&EntityId, &T)> {
        self.components.iter().map(|(id, entry)| (id, &entry.data))
    }
}

/// Persistent world holding all entities and components, tracking component versions.
#[derive(Debug)]
pub(super) struct World {
    next_entity_id: u64,
    entities: HashSet<EntityId>,
    pub(super) current_version: u64,

    // Component Storages
    pub(super) renderables: ComponentStorage<Renderable>,
    rects: ComponentStorage<Rect>,
    pub(super) layers: ComponentStorage<Layer>,
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
    #[must_use]
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Clear the world completely (resetting versions too)
    pub(super) fn clear(&mut self) {
        self.next_entity_id = 0;
        self.current_version = 1;
        self.entities.clear();
        self.renderables.clear();
        self.rects.clear();
        self.layers.clear();
    }

    /// Advance the world version, signifying a new frame/update cycle
    pub(super) const fn tick(&mut self) {
        self.current_version += 1;
    }

    /// Create a new entity
    pub(super) fn create_entity(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id += 1;
        self.entities.insert(id);
        id
    }

    /// Destroy an entity and all its components
    pub(super) fn destroy_entity(&mut self, entity: EntityId) {
        if self.entities.remove(&entity) {
            self.renderables.remove(entity);
            self.rects.remove(entity);
            self.layers.remove(entity);
        }
    }

    /// Helper to add a renderable component
    pub(super) fn add_renderable(&mut self, entity: EntityId, renderable: Renderable) {
        if self.entities.contains(&entity) {
            self.renderables
                .insert(entity, renderable, self.current_version);
        }
    }

    /// Helper to add a rect component
    pub(super) fn add_rect(&mut self, entity: EntityId, rect: Rect) {
        if self.entities.contains(&entity) {
            self.rects.insert(entity, rect, self.current_version);
        }
    }

    /// Helper to add a layer component
    pub(super) fn add_layer(&mut self, entity: EntityId, layer: Layer) {
        if self.entities.contains(&entity) {
            self.layers.insert(entity, layer, self.current_version);
        }
    }

    /// Get all entities
    #[must_use]
    pub(super) const fn entities(&self) -> &HashSet<EntityId> {
        &self.entities
    }
}
