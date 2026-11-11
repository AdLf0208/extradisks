// Extra Disks slice 1 for Horizon: the 1024k item storage's pure model.
//
// Ported from Extra Disks 1.21.1-4.0.15 (ChaoticTrials/ExtraDisks, Apache-2.0);
// this is a clean-room rewrite from the mod's observed behaviour, its data files
// and its public documentation (see FINDINGS.md). Local-only until publishing is
// agreed.
//
// This module is the standalone, unit-tested core: the rung ladder and its
// capacity, a storage's resource identity, insert/extract with upstream's
// partial-insert rule, and the encoding a block entity's compound or a disk's
// item component keeps. Everything that needs the game (block entities, item
// components, the right-click and the commands) lives in `support.rs` and is
// injected as `crate::game::extradisks_state`.

/// The 1024k rung's capacity in items: `ExtraItemStorageVariant.TIER_6(1024L)`
/// × 1024 (FINDINGS.md, "The exact capacity of the 1024k rung").
pub const CAPACITY_1024K: u64 = 1024 * 1024;
/// The most distinct resources one container tracks. Upstream's resource list
/// has no cap; this port keeps the parent port's bound so one item component
/// (and one block entity compound) cannot grow without limit.
pub const MAX_RESOURCES: usize = 256;
/// The separator between an entry's amount, id and component key. Holds no
/// item id and is split only for the first two fields, so a component may
/// contain it.
pub const FIELD: char = '|';

/// One rung of Extra Disks' item ladder, in upstream order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant {
    /// 256k (262,144 items).
    K256,
    /// 1024k (1,048,576 items).
    K1024,
    /// 4096k (4,194,304 items).
    K4096,
    /// 16384k (16,777,216 items).
    K16384,
    /// 65536k (67,108,864 items).
    K65536,
    /// 262144k (268,435,456 items).
    K262144,
    /// 1048576k (1,073,741,824 items).
    K1048576,
    /// The creative rung: no capacity at all (`TIER_12(null)`).
    Infinite,
}

impl Variant {
    /// Every rung, in upstream order.
    pub const ALL: [Variant; 8] = [
        Variant::K256,
        Variant::K1024,
        Variant::K4096,
        Variant::K16384,
        Variant::K65536,
        Variant::K262144,
        Variant::K1048576,
        Variant::Infinite,
    ];

    /// The rung's own name, as upstream's `getName`/`getSerializedName` give it.
    pub fn name(self) -> &'static str {
        match self {
            Variant::K256 => "256k",
            Variant::K1024 => "1024k",
            Variant::K4096 => "4096k",
            Variant::K16384 => "16384k",
            Variant::K65536 => "65536k",
            Variant::K262144 => "262144k",
            Variant::K1048576 => "1048576k",
            Variant::Infinite => "infinite",
        }
    }

    /// The rung a name stands for, none for anything else.
    pub fn from_name(name: &str) -> Option<Variant> {
        Variant::ALL.into_iter().find(|rung| rung.name() == name)
    }

    /// The capacity in items; none for the creative rung, which upstream
    /// leaves unlimited (`getCapacity` returns null).
    pub fn capacity(self) -> Option<u64> {
        match self {
            Variant::K256 => Some(256 * 1024),
            Variant::K1024 => Some(CAPACITY_1024K),
            Variant::K4096 => Some(4096 * 1024),
            Variant::K16384 => Some(16384 * 1024),
            Variant::K65536 => Some(65536 * 1024),
            Variant::K262144 => Some(262_144 * 1024),
            Variant::K1048576 => Some(1_048_576 * 1024),
            Variant::Infinite => None,
        }
    }

    /// The rung's storage block id (`extradisks:1024k_item_storage_block`).
    pub fn block_id(self) -> String {
        format!("extradisks:{}_item_storage_block", self.name())
    }

    /// The rung's disk id (`extradisks:1024k_item_storage_disk`).
    pub fn disk_id(self) -> String {
        format!("extradisks:{}_item_storage_disk", self.name())
    }

    /// The rung a storage block id stands for.
    pub fn from_block_id(id: &str) -> Option<Variant> {
        let name = id
            .strip_prefix("extradisks:")?
            .strip_suffix("_item_storage_block")?;
        Variant::from_name(name)
    }

    /// The rung a disk id stands for.
    pub fn from_disk_id(id: &str) -> Option<Variant> {
        let name = id
            .strip_prefix("extradisks:")?
            .strip_suffix("_item_storage_disk")?;
        Variant::from_name(name)
    }
}

/// What one stored entry is: the item id and the resource's component key.
/// Upstream's `ItemResource` compares the item and its components, so two
/// otherwise identical stacks with different components are two resources.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Resource {
    id: String,
    extra: String,
}

impl Resource {
    /// A resource of a bare item id (no components beyond the defaults).
    pub fn new(id: impl Into<String>) -> Self {
        Resource {
            id: id.into(),
            extra: String::new(),
        }
    }

    /// A resource of an item id and a component key.
    pub fn with_extra(id: impl Into<String>, extra: impl Into<String>) -> Self {
        Resource {
            id: id.into(),
            extra: extra.into(),
        }
    }

    /// The item id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The component key; empty when the item carries none.
    pub fn extra(&self) -> &str {
        &self.extra
    }
}

/// One resource and how much of it is stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub resource: Resource,
    pub amount: u64,
}

/// A capacity-bounded set of (resource, amount) pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Storage {
    capacity: u64,
    entries: Vec<Entry>,
}

impl Storage {
    /// An empty storage of `capacity` items.
    pub fn new(capacity: u64) -> Self {
        Storage {
            capacity,
            entries: Vec::new(),
        }
    }

    /// The capacity in items.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Everything stored, one entry per resource, in insertion order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The items stored.
    pub fn total(&self) -> u64 {
        self.entries.iter().map(|entry| entry.amount).sum()
    }

    /// The items the storage can still take.
    pub fn free(&self) -> u64 {
        self.capacity.saturating_sub(self.total())
    }

    /// The distinct resources stored.
    pub fn resources(&self) -> usize {
        self.entries.len()
    }

    /// Whether the storage holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many of `resource` are stored.
    pub fn amount(&self, resource: &Resource) -> u64 {
        self.entries
            .iter()
            .filter(|entry| &entry.resource == resource)
            .map(|entry| entry.amount)
            .sum()
    }

    /// Whether one more of `resource` may be stored: it is already stored, or
    /// there is room for another resource.
    pub fn accepts(&self, resource: &Resource) -> bool {
        self.amount(resource) > 0 || self.entries.len() < MAX_RESOURCES
    }

    /// Insert up to `amount` of `resource`, answering how many the storage took.
    /// Upstream's `LimitedStorageImpl.insert` takes `min(amount, free)` and takes
    /// nothing at all once the storage is full.
    pub fn insert(&mut self, resource: Resource, amount: u64) -> u64 {
        if !self.accepts(&resource) {
            return 0;
        }
        let took = amount.min(self.free());
        if took == 0 {
            return 0;
        }
        match self.entries.iter_mut().find(|e| e.resource == resource) {
            Some(entry) => entry.amount += took,
            None => self.entries.push(Entry {
                resource,
                amount: took,
            }),
        }
        took
    }

    /// Take up to `want` of `resource`, answering how many came out. An entry
    /// that reaches zero is dropped.
    pub fn extract(&mut self, resource: &Resource, want: u64) -> u64 {
        let Some(index) = self.entries.iter().position(|e| &e.resource == resource) else {
            return 0;
        };
        let took = want.min(self.entries[index].amount);
        if took == 0 {
            return 0;
        }
        if took == self.entries[index].amount {
            self.entries.remove(index);
        } else {
            self.entries[index].amount -= took;
        }
        took
    }

    /// Replace the contents with saved ones: merged, emptied entries dropped,
    /// cut to capacity and to [`MAX_RESOURCES`].
    pub fn set_entries(&mut self, saved: Vec<Entry>) {
        self.entries.clear();
        for Entry { resource, amount } in saved {
            if amount == 0 || !self.accepts(&resource) {
                continue;
            }
            let took = amount.min(self.free());
            if took == 0 {
                break;
            }
            self.insert(resource, took);
        }
    }

    /// The storage as the strings a saved compound or an item component keeps:
    /// `"<amount>|<id>|<component key>"` per entry, in insertion order.
    pub fn encode(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| {
                format!(
                    "{}{FIELD}{}{FIELD}{}",
                    entry.amount, entry.resource.id, entry.resource.extra
                )
            })
            .collect()
    }

    /// A storage read back from saved strings. Unreadable entries are skipped
    /// (an item id and a component key are both free text, so a cut-off entry is
    /// data damage, not a reason to refuse the whole container), and the rest is
    /// cut to `capacity`.
    pub fn parse(capacity: u64, saved: &[String]) -> Self {
        let mut storage = Storage::new(capacity);
        let entries = saved.iter().filter_map(|line| decode_entry(line)).collect();
        storage.set_entries(entries);
        storage
    }
}

/// One saved entry, or none when the line does not encode one.
fn decode_entry(line: &str) -> Option<Entry> {
    let (amount, rest) = line.split_once(FIELD)?;
    let (id, extra) = rest.split_once(FIELD)?;
    let amount = amount.parse::<u64>().ok()?;
    if id.is_empty() {
        return None;
    }
    Some(Entry {
        resource: Resource::with_extra(id, extra),
        amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone() -> Resource {
        Resource::new("minecraft:stone")
    }

    fn named(name: &str) -> Resource {
        Resource::with_extra("minecraft:diamond", format!("{{custom_name:\"{name}\"}}"))
    }

    fn full() -> Storage {
        let mut storage = Storage::new(CAPACITY_1024K);
        assert_eq!(storage.insert(stone(), CAPACITY_1024K), CAPACITY_1024K);
        storage
    }

    #[test]
    fn the_1024k_rung_holds_one_million_forty_eight_thousand_five_hundred_seventy_six_items() {
        let rung = Variant::from_name("1024k").expect("the 1024k rung exists");
        assert_eq!(rung, Variant::K1024);
        assert_eq!(rung.capacity(), Some(CAPACITY_1024K));
        assert_eq!(CAPACITY_1024K, 1_048_576);
        assert_eq!(rung.name(), "1024k");
    }

    #[test]
    fn the_ladder_is_the_upstream_ladder_with_an_unlimited_top_rung() {
        let ladder: Vec<(Option<u64>, &str)> = Variant::ALL
            .into_iter()
            .map(|rung| (rung.capacity(), rung.name()))
            .collect();
        assert_eq!(
            ladder,
            vec![
                (Some(262_144), "256k"),
                (Some(1_048_576), "1024k"),
                (Some(4_194_304), "4096k"),
                (Some(16_777_216), "16384k"),
                (Some(67_108_864), "65536k"),
                (Some(268_435_456), "262144k"),
                (Some(1_073_741_824), "1048576k"),
                (None, "infinite"),
            ]
        );
        assert_eq!(Variant::from_name("512k"), None);
        assert_eq!(Variant::from_name(""), None);
    }

    #[test]
    fn the_ids_are_the_jars_and_round_trip() {
        assert_eq!(
            Variant::K1024.block_id(),
            "extradisks:1024k_item_storage_block"
        );
        assert_eq!(
            Variant::K1024.disk_id(),
            "extradisks:1024k_item_storage_disk"
        );
        assert_eq!(
            Variant::from_block_id("extradisks:1024k_item_storage_block"),
            Some(Variant::K1024)
        );
        assert_eq!(
            Variant::from_disk_id("extradisks:1024k_item_storage_disk"),
            Some(Variant::K1024)
        );
        assert_eq!(Variant::from_block_id("minecraft:barrel"), None);
        assert_eq!(
            Variant::from_disk_id("extradisks:1024k_item_storage_part"),
            None
        );
        assert_eq!(Variant::from_block_id("refinedstorage:disk_drive"), None);
    }

    #[test]
    fn an_insert_is_taken_up_to_the_free_space() {
        let mut storage = Storage::new(CAPACITY_1024K);
        assert_eq!(storage.free(), 1_048_576);
        assert_eq!(storage.insert(stone(), 1_000_000), 1_000_000);
        assert_eq!(storage.total(), 1_000_000);
        assert_eq!(storage.free(), 48_576);
        assert_eq!(storage.amount(&stone()), 1_000_000);
        assert_eq!(storage.resources(), 1);
    }

    #[test]
    fn a_storage_exactly_full_takes_the_last_item_and_refuses_the_next() {
        let mut storage = full();
        assert_eq!(storage.total(), CAPACITY_1024K);
        assert_eq!(storage.free(), 0);
        assert_eq!(
            storage.insert(stone(), 1),
            0,
            "a full 1024k storage refuses every insert"
        );
        assert_eq!(storage.total(), CAPACITY_1024K, "and never grows past it");
        assert_eq!(
            storage.insert(Resource::new("minecraft:dirt"), 64),
            0,
            "including a resource it does not hold yet"
        );
    }

    #[test]
    fn an_over_capacity_insert_is_partial_not_refused_whole() {
        let mut storage = Storage::new(CAPACITY_1024K);
        assert_eq!(storage.insert(stone(), 1_048_000), 1_048_000);
        assert_eq!(
            storage.insert(Resource::new("minecraft:dirt"), 1_000),
            576,
            "upstream takes min(amount, free)"
        );
        assert_eq!(storage.amount(&Resource::new("minecraft:dirt")), 576);
        assert_eq!(storage.total(), CAPACITY_1024K);
        assert_eq!(
            storage.insert(Resource::new("minecraft:dirt"), 1),
            0,
            "and a resource with no room left takes nothing"
        );
    }

    #[test]
    fn an_extract_is_partial_and_an_emptied_resource_disappears() {
        let mut storage = Storage::new(CAPACITY_1024K);
        storage.insert(stone(), 10);
        assert_eq!(storage.extract(&stone(), 64), 10);
        assert_eq!(storage.extract(&stone(), 1), 0);
        assert!(storage.is_empty());
        assert_eq!(storage.resources(), 0);
    }

    #[test]
    fn an_extract_leaves_the_rest_behind() {
        let mut storage = Storage::new(CAPACITY_1024K);
        storage.insert(stone(), 100);
        assert_eq!(storage.extract(&stone(), 40), 40);
        assert_eq!(storage.amount(&stone()), 60);
        assert_eq!(storage.resources(), 1);
    }

    #[test]
    fn two_stacks_with_different_components_are_two_resources() {
        let mut storage = Storage::new(CAPACITY_1024K);
        assert_eq!(storage.insert(named("one"), 5), 5);
        assert_eq!(storage.insert(named("two"), 7), 7);
        assert_eq!(storage.resources(), 2);
        assert_eq!(storage.amount(&named("one")), 5);
        assert_eq!(storage.amount(&named("two")), 7);
        assert_eq!(storage.total(), 12);
        assert_eq!(storage.extract(&named("one"), 5), 5);
        assert_eq!(storage.resources(), 1, "only the named one left");
    }

    #[test]
    fn a_storage_tracks_at_most_256_resources_but_more_of_a_known_one() {
        let mut storage = Storage::new(CAPACITY_1024K);
        for index in 0..MAX_RESOURCES {
            let resource = Resource::new(format!("test:item{index}"));
            assert_eq!(storage.insert(resource, 1), 1);
        }
        assert_eq!(storage.resources(), MAX_RESOURCES);
        assert_eq!(
            storage.insert(Resource::new("test:one_too_many"), 1),
            0,
            "the 257th resource is refused"
        );
        assert_eq!(
            storage.insert(Resource::new("test:item0"), 10),
            10,
            "an existing resource still takes more"
        );
        assert_eq!(storage.resources(), MAX_RESOURCES);
    }

    #[test]
    fn a_saved_storage_is_cut_to_its_capacity() {
        let storage = Storage::parse(
            CAPACITY_1024K,
            &[
                "600000|minecraft:stone|".to_owned(),
                "600000|minecraft:dirt|".to_owned(),
                "1|minecraft:air|".to_owned(),
            ],
        );
        assert_eq!(storage.total(), CAPACITY_1024K, "a hostile save is cut");
        assert_eq!(storage.amount(&Resource::new("minecraft:stone")), 600_000);
        assert_eq!(storage.amount(&Resource::new("minecraft:dirt")), 448_576);
        assert_eq!(storage.amount(&Resource::new("minecraft:air")), 0);
    }

    #[test]
    fn a_saved_storage_round_trips_in_order() {
        let mut storage = Storage::new(CAPACITY_1024K);
        storage.insert(named("kept"), 3);
        storage.insert(stone(), 1_048_573);
        let encoded = storage.encode();
        assert_eq!(
            encoded,
            vec![
                "3|minecraft:diamond|{custom_name:\"kept\"}".to_owned(),
                "1048573|minecraft:stone|".to_owned(),
            ]
        );
        assert_eq!(Storage::parse(CAPACITY_1024K, &encoded), storage);
    }

    #[test]
    fn an_unreadable_saved_entry_is_skipped_not_fatal() {
        let storage = Storage::parse(
            CAPACITY_1024K,
            &[
                "nonsense".to_owned(),
                "12".to_owned(),
                "12|".to_owned(),
                "|minecraft:stone|".to_owned(),
                "0|minecraft:air|".to_owned(),
                "-4|minecraft:dirt|".to_owned(),
                "5|minecraft:gold_ingot|".to_owned(),
            ],
        );
        assert_eq!(storage.resources(), 1);
        assert_eq!(storage.amount(&Resource::new("minecraft:gold_ingot")), 5);
        assert_eq!(storage.total(), 5);
    }

    #[test]
    fn a_duplicate_saved_resource_is_merged() {
        let storage = Storage::parse(
            CAPACITY_1024K,
            &[
                "5|minecraft:stone|".to_owned(),
                "7|minecraft:stone|".to_owned(),
            ],
        );
        assert_eq!(storage.resources(), 1);
        assert_eq!(storage.amount(&stone()), 12);
    }

    #[test]
    fn a_component_key_may_contain_the_separator() {
        let resource = Resource::with_extra("minecraft:stone", "a|b|c");
        let mut storage = Storage::new(CAPACITY_1024K);
        storage.insert(resource.clone(), 4);
        let encoded = storage.encode();
        assert_eq!(encoded, vec!["4|minecraft:stone|a|b|c".to_owned()]);
        assert_eq!(Storage::parse(CAPACITY_1024K, &encoded), storage);
        assert_eq!(storage.amount(&resource), 4);
    }

    #[test]
    fn a_whole_capacity_round_trip_keeps_every_item() {
        let storage = full();
        let back = Storage::parse(CAPACITY_1024K, &storage.encode());
        assert_eq!(back.total(), CAPACITY_1024K);
        assert_eq!(back, storage);
    }

    #[test]
    fn a_zero_capacity_storage_takes_nothing() {
        let mut storage = Storage::new(0);
        assert_eq!(storage.insert(stone(), 1), 0);
        assert_eq!(storage.free(), 0);
        assert!(storage.is_empty());
    }
}
