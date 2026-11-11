# extradisks (Horizon port)

A Horizon port of [extradisks](ChaoticTrials/ExtraDisks) — Apache-2.0.

**Status: first slices.** What is below is what the port actually does today.

## Implemented so far

- | Id | What it is | Holds |
- |---|---|---|
- | `extradisks:1024k_item_storage_block` | the storage block (a block entity) | 1,048,576 items in up to 256 resources |
- | `extradisks:1024k_item_storage_disk` | the storage disk (an item) | the same, in the `extradisks:storage` item component |
- Both use one engine (`runtime.rs`), so the capacity rule, the resource identity
- and the encoding cannot drift apart:
- `1024k` = `TIER_6(1024L) × 1024` = **1,048,576 items**
- (`ExtraItemStorageVariant`; see [`FINDINGS.md`](FINDINGS.md)).

## Not yet ported

- No network.** The storage block neither joins a Refined Storage network nor
- answers the parent's grid: upstream it is a network node
- (`StorageBlock` → `updateActiveness`/`doWork`). A player sees standalone
- storage in this port, not storage that a Grid can browse.
- No energy.** Upstream the 1024k block consumes 14 FE/t
- (`ModConfig.storageblocks.item.1024k`) and the disk has no buffer. No FE hook
- is used, and no energy is modelled.
- No GUI.** Upstream opens `Menus.getItemStorage()` with insert slots, a

## Install

Build the bundle with a native-mod capable launcher, or add the file from the
registry once it is listed. Pinned to Horizon `1d63e0a` and horizon-mixins
`5a64af93` for game 26.3.

## Build from source

```sh
cargo test
python3 tools/horizon.py check --horizon <horizon checkout>
python3 tools/horizon.py build --horizon <horizon checkout>
python3 tools/package.py
```

## Credits and licence

Original mod: ChaoticTrials/ExtraDisks. Port: AdLf0208. Licence: Apache-2.0 — see [LICENSE](LICENSE);
the original notices are preserved.
