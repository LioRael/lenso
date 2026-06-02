//! Shared domain plugin contract.
//!
//! A [`DomainDescriptor`] is the single value a business domain exposes to the
//! composition root. It bundles everything an app needs to wire a domain in:
//! its runtime functions, event handlers, story-display metadata, and optional
//! HTTP routes. Apps iterate descriptors instead of hand-wiring each domain, so
//! adding a domain is one list entry rather than edits scattered across apps.

use platform_core::{EventHandler, StoryDisplayDescriptor};
use platform_runtime::RuntimeDescriptor;
use std::sync::Arc;

/// Everything a single domain contributes to a running app's background wiring.
///
/// Carries the context-bound concerns (runtime functions, event handlers) plus
/// console display metadata. HTTP routes are assembled separately via the
/// composition root because they are context-free and OpenAPI-aware.
#[derive(Clone)]
pub struct DomainDescriptor {
    /// Stable domain name, e.g. `"identity"`.
    pub name: &'static str,
    /// Runtime functions, queues, triggers, and flows owned by the domain.
    pub runtime: RuntimeDescriptor,
    /// In-process event handlers the domain registers on the outbox dispatcher.
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
    /// Story-display metadata for the runtime console.
    pub story_display: &'static [StoryDisplayDescriptor],
}

impl DomainDescriptor {
    /// Start building a descriptor for `name` with the given runtime.
    #[must_use]
    pub fn new(name: &'static str, runtime: RuntimeDescriptor) -> Self {
        Self {
            name,
            runtime,
            event_handlers: Vec::new(),
            story_display: &[],
        }
    }

    /// Attach in-process event handlers.
    #[must_use]
    pub fn with_event_handlers(mut self, handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        self.event_handlers = handlers;
        self
    }

    /// Attach story-display metadata for the runtime console.
    #[must_use]
    pub fn with_story_display(mut self, story_display: &'static [StoryDisplayDescriptor]) -> Self {
        self.story_display = story_display;
        self
    }
}

impl std::fmt::Debug for DomainDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DomainDescriptor")
            .field("name", &self.name)
            .field("runtime", &self.runtime)
            .field("event_handlers", &self.event_handlers.len())
            .field("story_display", &self.story_display.len())
            .finish()
    }
}
