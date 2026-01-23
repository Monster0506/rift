use crate::render::components::{Layer, Rect, Renderable};
use std::collections::{HashMap, HashSet};

/// Unique identifier for an entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub(crate) u64);

/// Generic storage for components
#[derive(Debug, Clone)]
pub struct ComponentStorage<T> {
    components: HashMap<EntityId, T>,
}

impl<T> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self {
            components: HashMap::new(),
        }
    }
}

impl<T> ComponentStorage<T> {
    pub fn insert(&mut self, entity: EntityId, component: T) {
        self.components.insert(entity, component);
    }

    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.components.get(&entity)
    }

    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        self.components.get_mut(&entity)
    }

    pub fn remove(&mut self, entity: EntityId) -> Option<T> {
        self.components.remove(&entity)
    }

    pub fn clear(&mut self) {
        self.components.clear();
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, EntityId, T> {
        self.components.iter()
    }
}

/// The World that holds all entities and components
///
/// This is an ephemeral world that is rebuilt every frame for rendering.
#[derive(Debug, Default)]
pub struct World {
    next_entity_id: u64,
    entities: HashSet<EntityId>,

    // Component Storages
    pub renderables: ComponentStorage<Renderable>,
    pub rects: ComponentStorage<Rect>,
    pub layers: ComponentStorage<Layer>,
}

impl World {
    /// Create a new empty world
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the world completely
    pub fn clear(&mut self) {
        self.next_entity_id = 0;
        self.entities.clear();
        self.renderables.clear();
        self.rects.clear();
        self.layers.clear();
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
            self.renderables.insert(entity, renderable);
        }
    }

    /// Helper to add a rect component
    pub fn add_rect(&mut self, entity: EntityId, rect: Rect) {
        if self.entities.contains(&entity) {
            self.rects.insert(entity, rect);
        }
    }

    /// Helper to add a layer component
    pub fn add_layer(&mut self, entity: EntityId, layer: Layer) {
        if self.entities.contains(&entity) {
            self.layers.insert(entity, layer);
        }
    }

    /// Get all entities
    pub fn entities(&self) -> &HashSet<EntityId> {
        &self.entities
    }
}
