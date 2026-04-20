//! Solar system activation - demonstrates nested plugin hierarchy
//!
//! This activation shows the coalgebraic plugin structure where plugins
//! can have children. The solar system is a natural hierarchy:
//! - Sol (star) contains planets
//! - Planets contain moons
//!
//! Each level implements the F-coalgebra structure map via `plugin_schema()`.

use super::celestial::{build_solar_system, CelestialBody, CelestialBodyActivation};
use super::types::{BodyType, SolarEvent};
use crate::plexus::ChildSummary;
use async_stream::stream;
use futures::Stream;

/// Solar system activation - demonstrates nested plugin children
#[derive(Clone)]
pub struct Solar {
    system: CelestialBody,
}

impl Solar {
    pub fn new() -> Self {
        Self {
            system: build_solar_system(),
        }
    }

    /// Find a body by path (e.g., "earth" or "jupiter.io")
    fn find_body(&self, path: &str) -> Option<&CelestialBody> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &self.system;

        for part in parts {
            let normalized = part.to_lowercase();
            if current.name.to_lowercase() == normalized {
                continue;
            }
            current = current.children.iter()
                .find(|c| c.name.to_lowercase() == normalized)?;
        }
        Some(current)
    }

    /// Count all moons in the system
    fn moon_count(&self) -> usize {
        fn count_moons(body: &CelestialBody) -> usize {
            let mine: usize = body.children.iter()
                .filter(|c| c.body_type == BodyType::Moon)
                .count();
            let nested: usize = body.children.iter()
                .map(count_moons)
                .sum();
            mine + nested
        }
        count_moons(&self.system)
    }
}

impl Default for Solar {
    fn default() -> Self {
        Self::new()
    }
}

/// Solar system model - demonstrates nested plugin hierarchy
#[plexus_macros::activation(namespace = "solar", version = "1.0.0")]
impl Solar {
    /// Get an overview of the solar system
    #[plexus_macros::method]
    async fn observe(&self) -> impl Stream<Item = SolarEvent> + Send + 'static {
        let star = self.system.name.clone();
        let planet_count = self.system.children.len();
        let moon_count = self.moon_count();
        let total_bodies = 1 + self.system.descendant_count();

        stream! {
            yield SolarEvent::System {
                star,
                planet_count,
                moon_count,
                total_bodies,
            };
        }
    }

    /// Get detailed information about a celestial body at `path`
    /// (e.g., `"earth"`, `"jupiter.io"`, `"saturn.titan"`).
    #[plexus_macros::method]
    async fn info(
        &self,
        path: String,
    ) -> impl Stream<Item = SolarEvent> + Send + 'static {
        let body = self.find_body(&path).cloned();

        stream! {
            if let Some(b) = body {
                yield SolarEvent::Body {
                    name: b.name,
                    body_type: b.body_type,
                    mass_kg: b.mass_kg,
                    radius_km: b.radius_km,
                    orbital_period_days: b.orbital_period_days,
                    parent: b.parent,
                };
            }
        }
    }

    /// Look up a celestial body by name (case-insensitive).
    #[plexus_macros::child(list = "body_names")]
    async fn body(&self, name: &str) -> Option<CelestialBodyActivation> {
        let normalized = name.to_lowercase();
        self.system
            .children
            .iter()
            .find(|c| c.name.to_lowercase() == normalized)
            .map(|c| CelestialBodyActivation::new(c.clone()))
    }

    /// Stream the configured planet names for `ChildRouter::list_children`.
    async fn body_names(&self) -> impl Stream<Item = String> + Send + '_ {
        let names: Vec<String> = self
            .system
            .children
            .iter()
            .map(|c| c.name.clone())
            .collect();
        futures::stream::iter(names)
    }

    /// Hand-written child summary list.
    ///
    /// Preserved (not macro-synthesized) so each planet's `hash` field carries
    /// the deterministic digest computed from that planet's own sub-schema.
    /// The macro's synthesis path only covers static `#[child]` methods and
    /// would emit empty-string hashes; Solar's children are dynamic.
    pub fn plugin_children(&self) -> Vec<ChildSummary> {
        self.system.children.iter()
            .map(|planet| planet.to_child_summary())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plexus::{Activation, DynamicHub};

    #[test]
    fn solar_is_hub_with_planets() {
        let solar = Solar::new();
        let schema = solar.plugin_schema();

        assert!(schema.is_hub(), "solar should be a hub");
        let children = schema.children.as_ref().expect("solar should have children");
        assert_eq!(children.len(), 8, "solar should have 8 planets");

        // Children are summaries - check namespace and description
        let jupiter = children.iter().find(|c| c.namespace == "jupiter").unwrap();
        assert!(jupiter.description.contains("planet"));
        assert!(!jupiter.hash.is_empty());
    }

    #[test]
    fn solar_registered_with_dynamic_hub() {
        let hub = DynamicHub::new("plexus").register(Solar::new());
        let schema = hub.plugin_schema();

        // DynamicHub is a hub
        assert!(schema.is_hub());
        let children = schema.children.as_ref().unwrap();

        // Solar should be one of the children (as a summary)
        let solar = children.iter().find(|c| c.namespace == "solar").unwrap();
        assert!(solar.description.contains("Solar system"));
        assert!(!solar.hash.is_empty());
    }

    #[test]
    fn solar_hash_changes_with_structure() {
        let solar1 = Solar::new();
        let solar2 = Solar::new();

        // Same structure = same hash
        assert_eq!(
            solar1.plugin_schema().hash,
            solar2.plugin_schema().hash
        );
    }

    #[test]
    fn print_solar_schema() {
        let solar = Solar::new();
        let schema = solar.plugin_schema();
        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("Solar system schema:\n{}", json);
    }

    #[tokio::test]
    async fn test_nested_routing_mercury() {
        let solar = Solar::new();
        let result = Activation::call(&solar, "mercury.info", serde_json::json!({}), None, None).await;
        assert!(result.is_ok(), "mercury.info should be callable: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_nested_routing_jupiter_io() {
        let solar = Solar::new();

        // Call solar.call("jupiter.io.info", {}) - should route jupiter → io
        let result = Activation::call(&solar, "jupiter.io.info", serde_json::json!({}), None, None).await;
        assert!(result.is_ok(), "jupiter.io.info should be callable");
    }

    #[tokio::test]
    async fn test_nested_routing_earth_luna() {
        let solar = Solar::new();

        // Call solar.call("earth.luna.info", {}) - should route earth → luna
        let result = Activation::call(&solar, "earth.luna.info", serde_json::json!({}), None, None).await;
        assert!(result.is_ok(), "earth.luna.info should be callable");
    }

    #[tokio::test]
    async fn test_nested_routing_invalid_child() {
        let solar = Solar::new();

        // Call with invalid child
        let result = Activation::call(&solar, "pluto.info", serde_json::json!({}), None, None).await;
        assert!(result.is_err(), "pluto.info should fail - not a planet");
    }

    // CHILD-7: `list_children` capability exposes configured planet names.
    #[tokio::test]
    async fn solar_list_children_returns_configured_planets() {
        use crate::plexus::ChildRouter;
        use futures::StreamExt;

        let solar = Solar::new();
        let listed: Vec<String> = solar
            .list_children()
            .await
            .expect("list_children must be Some for Solar (CHILD-4 opt-in)")
            .collect()
            .await;

        assert!(!listed.is_empty(), "list_children must yield at least one name");
        assert_eq!(listed.len(), 8, "Solar has 8 configured planets");
        for expected in [
            "Mercury", "Venus", "Earth", "Mars",
            "Jupiter", "Saturn", "Uranus", "Neptune",
        ] {
            assert!(
                listed.iter().any(|n| n == expected),
                "list_children should include {expected}; got {listed:?}"
            );
        }
    }

    // CHILD-7: case-insensitive child lookup is preserved through the
    // migrated `#[child]` method body.
    #[tokio::test]
    async fn solar_get_child_case_insensitive() {
        use crate::plexus::ChildRouter;

        let solar = Solar::new();
        assert!(
            solar.get_child("Mercury").await.is_some(),
            "`Mercury` should resolve"
        );
        assert!(
            solar.get_child("mercury").await.is_some(),
            "`mercury` should resolve (case-insensitive)"
        );
        assert!(
            solar.get_child("MERCURY").await.is_some(),
            "`MERCURY` should resolve (case-insensitive)"
        );
        assert!(
            solar.get_child("pluto").await.is_none(),
            "`pluto` should not resolve"
        );
    }
}
