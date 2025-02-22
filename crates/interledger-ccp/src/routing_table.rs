use crate::packet::{Route, RouteUpdateRequest};
use bytes::Bytes;
use hex;
use lazy_static::lazy_static;
use log::{debug, trace};
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::HashMap;
use std::iter::FromIterator;

lazy_static! {
    static ref RANDOM: SystemRandom = SystemRandom::new();
}

#[derive(Debug)]
struct PrefixMap<T> {
    map: HashMap<Bytes, T>,
}

impl<T> PrefixMap<T> {
    pub fn new() -> Self {
        PrefixMap {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, prefix: Bytes, item: T) -> bool {
        self.map.insert(prefix.clone(), item).is_none()
    }

    pub fn remove(&mut self, prefix: &[u8]) -> bool {
        self.map.remove(prefix).is_some()
    }

    pub fn resolve(&self, prefix: &[u8]) -> Option<&T> {
        // TODO use parallel iterator
        self.map
            .iter()
            .filter(|(p, _)| prefix.starts_with(p))
            .max_by_key(|(p, _)| p.len())
            .map(|(_prefix, item)| item)
    }
}

/// The routing table is identified by an ID (a UUID in array form) and an "epoch".
/// When an Interledger node reloads, it will generate a new UUID for its routing table.
/// Each update applied increments the epoch number, so it acts as a version tracker.
/// This helps peers make sure they are in sync with one another and request updates if not.
#[derive(Debug)]
pub struct RoutingTable<A> {
    id: [u8; 16],
    epoch: u32,
    prefix_map: PrefixMap<(A, Route)>,
}

impl<A> RoutingTable<A>
where
    A: Clone,
{
    pub fn new(id: [u8; 16]) -> Self {
        RoutingTable {
            id,
            epoch: 0,
            prefix_map: PrefixMap::new(),
        }
    }

    #[cfg(test)]
    pub fn set_id(&mut self, id: [u8; 16]) {
        self.id = id;
        self.epoch = 0;
    }

    #[cfg(test)]
    pub fn set_epoch(&mut self, epoch: u32) {
        self.epoch = epoch;
    }

    pub fn id(&self) -> [u8; 16] {
        self.id
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    pub fn increment_epoch(&mut self) -> u32 {
        let epoch = self.epoch;
        self.epoch += 1;
        epoch
    }

    /// Set a particular route, overwriting the one that was there before
    pub fn set_route(&mut self, prefix: Bytes, account: A, route: Route) {
        self.prefix_map.remove(&prefix[..]);
        self.prefix_map.insert(prefix, (account, route));
    }

    /// Remove the route for the given prefix. Returns true if that route existed before
    pub fn delete_route(&mut self, prefix: &[u8]) -> bool {
        self.prefix_map.remove(prefix)
    }

    /// Add the given route. Returns true if that routed did not already exist
    pub fn add_route(&mut self, account: A, route: Route) -> bool {
        self.prefix_map
            .insert(route.prefix.clone(), (account, route))
    }

    /// Get the best route we have for the given prefix
    pub fn get_route(&self, prefix: &[u8]) -> Option<&(A, Route)> {
        self.prefix_map.resolve(prefix)
    }

    pub fn get_simplified_table(&self) -> HashMap<Bytes, A> {
        HashMap::from_iter(
            self.prefix_map
                .map
                .iter()
                .map(|(address, (account, _route))| (address.clone(), account.clone())),
        )
    }

    /// Handle a CCP Route Update Request from the peer this table represents
    pub fn handle_update_request(
        &mut self,
        account: A,
        request: RouteUpdateRequest,
    ) -> Result<Vec<Bytes>, String> {
        if self.id != request.routing_table_id {
            debug!(
                "Saw new routing table. Old ID: {}, new ID: {}",
                hex::encode(&self.id[..]),
                hex::encode(&request.routing_table_id[..])
            );
            self.id = request.routing_table_id;
            self.epoch = 0;
        }

        if request.from_epoch_index > self.epoch {
            return Err(format!(
                "Gap in routing table {}. Expected epoch: {}, got from_epoch: {}",
                hex::encode(&self.id[..]),
                self.epoch,
                request.from_epoch_index
            ));
        }

        if request.to_epoch_index <= self.epoch {
            trace!(
                "Ignoring duplicate routing update for epoch: {}",
                self.epoch
            );
            return Ok(Vec::new());
        }

        if request.new_routes.is_empty() && request.withdrawn_routes.is_empty() {
            trace!(
                "Got heartbeat route update for table ID: {}, epoch: {}",
                hex::encode(&self.id[..]),
                self.epoch
            );
            return Ok(Vec::new());
        }

        let mut changed_prefixes = Vec::new();
        for prefix in request.withdrawn_routes.iter() {
            if self.delete_route(prefix) {
                changed_prefixes.push(prefix.clone());
            }
        }

        for route in request.new_routes.into_iter() {
            let prefix = route.prefix.clone();
            if self.add_route(account.clone(), route) {
                changed_prefixes.push(prefix);
            }
        }

        self.epoch = request.to_epoch_index;
        trace!(
            "Updated routing table {} to epoch: {}",
            hex::encode(&self.id[..]),
            self.epoch
        );

        Ok(changed_prefixes)
    }
}

impl<A> Default for RoutingTable<A>
where
    A: Clone,
{
    fn default() -> RoutingTable<A> {
        let mut id = [0; 16];
        RANDOM.fill(&mut id).expect("Unable to get randomness");
        RoutingTable::new(id)
    }
}

#[cfg(test)]
mod prefix_map {
    use super::*;

    #[test]
    fn doesnt_insert_duplicates() {
        let mut map = PrefixMap::new();
        assert!(map.insert(Bytes::from("example.a"), 1));
        assert!(!map.insert(Bytes::from("example.a"), 1));
    }

    #[test]
    fn removes_entry() {
        let mut map = PrefixMap::new();
        assert!(map.insert(Bytes::from("example.a"), 1));
        assert!(map.remove(&b"example.a"[..]));
        assert!(map.map.is_empty());
    }

    #[test]
    fn resolves_to_longest_matching_prefix() {
        let mut map = PrefixMap::new();
        map.insert(Bytes::from("example.a"), 1);
        map.insert(Bytes::from("example.a.b.c"), 2);
        map.insert(Bytes::from("example.a.b"), 3);

        assert_eq!(map.resolve(b"example.a").unwrap(), &1);
        assert_eq!(map.resolve(b"example.a.b.c").unwrap(), &2);
        assert_eq!(map.resolve(b"example.a.b.c.d.e").unwrap(), &2);
        assert!(map.resolve(b"example.other").is_none());
    }
}

#[cfg(test)]
mod table {
    use super::*;
    use crate::fixtures::*;
    use crate::test_helpers::*;

    #[test]
    fn sets_id_if_update_has_different() {
        let mut table = RoutingTable::new([0; 16]);
        let mut request = UPDATE_REQUEST_SIMPLE.clone();
        request.from_epoch_index = 0;
        table
            .handle_update_request(ROUTING_ACCOUNT.clone(), request.clone())
            .unwrap();
        assert_eq!(table.id, request.routing_table_id);
        assert_eq!(table.epoch, 0);
    }

    #[test]
    fn errors_if_gap_in_epoch_indecies() {
        let mut table = RoutingTable::new([0; 16]);
        let mut request = UPDATE_REQUEST_SIMPLE.clone();
        request.from_epoch_index = 1;
        let result = table.handle_update_request(ROUTING_ACCOUNT.clone(), request);
        assert_eq!(
            result.unwrap_err(),
            "Gap in routing table 21e55f8eabcd4e979ab9bf0ff00a224c. Expected epoch: 0, got from_epoch: 1"
        );
    }

    #[test]
    fn ignores_old_update() {
        let mut table = RoutingTable::new(UPDATE_REQUEST_COMPLEX.routing_table_id);
        table.epoch = 3;
        let mut request = UPDATE_REQUEST_COMPLEX.clone();
        request.from_epoch_index = 0;
        request.to_epoch_index = 1;
        let updated_routes = table
            .handle_update_request(ROUTING_ACCOUNT.clone(), request)
            .unwrap();
        assert_eq!(updated_routes.len(), 0);
    }

    #[test]
    fn ignores_empty_update() {
        let mut table = RoutingTable::new([0; 16]);
        let mut request = UPDATE_REQUEST_SIMPLE.clone();
        request.from_epoch_index = 0;
        request.to_epoch_index = 1;
        let updated_routes = table
            .handle_update_request(ROUTING_ACCOUNT.clone(), request)
            .unwrap();
        assert_eq!(updated_routes.len(), 0);
    }

    #[test]
    fn converts_to_a_simplified_table() {
        let mut table = RoutingTable::new([0; 16]);
        table.add_route(
            TestAccount::new(1, "example.one"),
            Route {
                prefix: Bytes::from("example.one"),
                path: Vec::new(),
                props: Vec::new(),
                auth: [0; 32],
            },
        );
        table.add_route(
            TestAccount::new(2, "example.two"),
            Route {
                prefix: Bytes::from("example.two"),
                path: Vec::new(),
                props: Vec::new(),
                auth: [0; 32],
            },
        );
        let simplified = table.get_simplified_table();
        assert_eq!(simplified.len(), 2);
        assert_eq!(simplified.get(&b"example.one"[..]).unwrap().id, 1);
        assert_eq!(simplified.get(&b"example.two"[..]).unwrap().id, 2);
    }
}
