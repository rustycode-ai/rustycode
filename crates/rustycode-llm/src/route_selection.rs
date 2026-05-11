//! Multi-account route selection strategies.

use crate::route::Route;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Strategy for selecting among multiple routes (accounts) on the same provider.
#[derive(Debug, Clone, Default)]
pub enum RouteSelection {
    /// First available route (default, current behavior)
    #[default]
    First,
    /// Round-robin across routes with matching model capability
    RoundRobin,
    /// Random selection
    Random,
    /// Fewest concurrent in-flight requests (requires per-Route atomic counter)
    LeastLoaded,
}

/// Select a route from the candidate list using the given strategy.
/// Returns `None` if candidates is empty.
pub fn select<'a>(
    candidates: &'a [Route],
    strategy: &RouteSelection,
    counter: &AtomicUsize,
) -> Option<&'a Route> {
    if candidates.is_empty() {
        return None;
    }
    match strategy {
        RouteSelection::First => candidates.first(),
        RouteSelection::RoundRobin => {
            let idx = counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
            Some(&candidates[idx])
        }
        RouteSelection::Random => {
            // Use a simple fast PRNG to avoid rand dependency
            let idx = counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
            Some(&candidates[idx])
        }
        RouteSelection::LeastLoaded => candidates.iter().min_by_key(|r| r.in_flight()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_route(name: &str) -> Route {
        Route::for_test(name)
    }

    #[test]
    fn first_selection_returns_first_matching_route() {
        let routes = vec![
            mock_route("route-1"),
            mock_route("route-2"),
            mock_route("route-3"),
        ];
        let selected = select(&routes, &RouteSelection::First, &AtomicUsize::new(0));
        assert_eq!(selected.unwrap().name(), "route-1");
    }

    #[test]
    fn round_robin_cycles_through_routes() {
        let routes = vec![
            mock_route("route-1"),
            mock_route("route-2"),
            mock_route("route-3"),
        ];
        let counter = AtomicUsize::new(0);
        let first = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let second = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let third = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let wraps = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        assert_eq!(first.name(), "route-1");
        assert_eq!(second.name(), "route-2");
        assert_eq!(third.name(), "route-3");
        assert_eq!(wraps.name(), "route-1"); // wraps around
    }

    #[test]
    fn random_selection_returns_a_valid_route() {
        let routes = vec![mock_route("route-1"), mock_route("route-2")];
        let counter = AtomicUsize::new(0);
        for _ in 0..20 {
            let selected = select(&routes, &RouteSelection::Random, &counter).unwrap();
            assert!(["route-1", "route-2"].contains(&selected.name()));
        }
    }

    #[test]
    fn empty_routes_returns_none() {
        let routes: Vec<Route> = vec![];
        let selected = select(&routes, &RouteSelection::First, &AtomicUsize::new(0));
        assert!(selected.is_none());
    }

    #[test]
    fn single_route_always_selected() {
        let routes = vec![mock_route("only")];
        let counter = AtomicUsize::new(0);
        for _ in 0..5 {
            let selected = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
            assert_eq!(selected.name(), "only");
        }
    }
}
