use crate::domain::models::{Profile, HistoryEntry};
use std::sync::Arc;

/// Domain events represent significant occurrences in the system
#[derive(Debug, Clone)]
pub enum Event {
    /// A profile was created
    ProfileCreated(Profile),
    /// A profile was updated
    ProfileUpdated(Profile),
    /// A profile was removed
    ProfileRemoved(String),
    /// A connection was established
    ConnectionStarted(Profile),
    /// A connection has ended
    ConnectionEnded(HistoryEntry),
    /// A plugin was enabled
    PluginEnabled(String),
    /// A plugin was disabled
    PluginDisabled(String),
}

/// Event listener trait for components that need to react to events
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &Event);
}

/// Event bus for publishing events to registered listeners
#[derive(Default)]
pub struct EventBus {
    listeners: Vec<Arc<dyn EventListener>>,
}

impl EventBus {
    /// Create a new empty event bus
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    /// Register a new event listener
    pub fn register(&mut self, listener: Arc<dyn EventListener>) {
        self.listeners.push(listener);
    }

    /// Publish an event to all registered listeners
    pub fn publish(&self, event: Event) {
        for listener in &self.listeners {
            listener.on_event(&event);
        }
    }
}

// Simple implementation of an event handler that logs events
#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    pub struct TestEventListener {
        pub events: Mutex<Vec<Event>>,
    }

    impl TestEventListener {
        pub fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        pub fn events(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventListener for TestEventListener {
        fn on_event(&self, event: &Event) {
            self.events.lock().unwrap().push(event.clone());
        }
    }
}