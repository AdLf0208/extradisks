// Game-side glue for Extra Disks slice 1: the 1024k item storage.
//
// Injected into `crate::game` as `extradisks_state`; it needs the game, so it is
// not part of the standalone library (only `runtime.rs` is).
//
// The slice: the **1024k item storage block** (a block entity holding 1,048,576
// items: `extradisks:1024k_item_storage_block`) and the **1024k storage disk**
// (the same storage, portable, in the item's `extradisks:storage` component).
//
// Upstream both are a Refined Storage network node with a GUI. This port has no
// network and no screen for a mod menu, so the block's interface is a
// right-click (hold a stack to put it in; empty hand to take a stack out) plus
// the `/extradisks` commands, and the disk's is the same command family on the
// held disk. The capacity rule, the resource identity and the saved encoding are
// the runtime's; this module only moves them in and out of the game.

use horizon_client::containers::ModBlockEntity;
use horizon_client::items::ItemStack;
use horizon_client::text::Text;
use horizon_world::nbt::{Compound, JavaString, Tag};

use crate::game::extradisks::{CAPACITY_1024K, Entry, Resource, Storage, Variant};
use crate::game::native_server::{NativeCommand, NativeServerPlayer};

/// The rung this slice implements; everything else in the jar is a gap.
const RUNGS: [Variant; 1] = [Variant::K1024];
/// The item component that carries a disk's contents (`native_component_types`).
const COMPONENT: &str = "extradisks:storage";
/// The saved compound key a storage block's contents live under.
const SAVED: &str = "ExtradisksStorage";

fn saved_key() -> JavaString {
    JavaString::from(SAVED)
}

// --- stacks and resources --------------------------------------------------

/// The resource a stack is: its item and its components (`item_data`'s saved
/// patch, damage included), without its count. Upstream's `ItemResource` is the
/// same pair, which is why two named diamonds are two resources.
fn resource_of(stack: &ItemStack) -> Resource {
    let extra = horizon_client::item_data::encode_components(stack)
        .map(|components| horizon_world::snbt_text::to_snbt(&components))
        .unwrap_or_default();
    Resource::with_extra(stack.item.to_string(), extra)
}

/// A stack of a stored resource, at `count`. A component key that no longer
/// parses gives a plain stack rather than nothing: the resource is still the
/// item, only its decoration was damaged.
fn stack_of(resource: &Resource, count: u32) -> ItemStack {
    let (damage, extra) = if resource.extra().is_empty() {
        (0, None)
    } else {
        match horizon_world::nbt::parse_snbt(resource.extra()) {
            Ok((tag, _)) => horizon_client::item_data::decode_components(Some(&tag)),
            Err(_) => (0, None),
        }
    };
    ItemStack {
        item: std::sync::Arc::from(resource.id()),
        count,
        damage,
        extra,
    }
}

/// The capacity of a rung; every rung in this slice has one.
fn capacity(rung: Variant) -> u64 {
    rung.capacity().unwrap_or(CAPACITY_1024K)
}

// --- block entity storage --------------------------------------------------

/// The storage a block entity's saved compound holds.
fn read_block(entity: &ModBlockEntity) -> Storage {
    let capacity = capacity(Variant::K1024);
    let Some(Tag::List(lines)) = entity.nbt.get(&saved_key()) else {
        return Storage::new(capacity);
    };
    let saved: Vec<String> = lines
        .iter()
        .filter_map(|line| match line {
            Tag::String(text) => Some(text.to_string_lossy()),
            _ => None,
        })
        .collect();
    Storage::parse(capacity, &saved)
}

/// Write a storage into a block entity's saved compound; an empty one keeps no
/// key at all, so a fresh block entity's compound stays empty.
fn write_block(entity: &mut ModBlockEntity, storage: &Storage) {
    if storage.is_empty() {
        entity.nbt.remove(&saved_key());
        return;
    }
    let lines = storage
        .encode()
        .into_iter()
        .map(|line| Tag::String(JavaString::from(line.as_str())))
        .collect();
    entity.nbt.insert(saved_key(), Tag::List(lines));
}

// --- the disk's item component ---------------------------------------------

/// The storage a disk item carries; an empty one when it has no component (a
/// freshly made or creative-given disk).
fn read_disk(game: &crate::game::Game<'_>, stack: &ItemStack) -> Storage {
    let capacity = capacity(Variant::K1024);
    let Ok(Some(Tag::List(lines))) = game.native_component(stack, COMPONENT) else {
        return Storage::new(capacity);
    };
    let saved: Vec<String> = lines
        .iter()
        .filter_map(|line| match line {
            Tag::String(text) => Some(text.to_string_lossy()),
            _ => None,
        })
        .collect();
    Storage::parse(capacity, &saved)
}

/// Write a storage into a disk item's component (an empty disk loses the
/// component rather than keeping an empty list).
fn write_disk(game: &crate::game::Game<'_>, stack: &mut ItemStack, storage: &Storage) {
    if storage.is_empty() {
        let _ = game.native_remove_component(stack, COMPONENT);
        return;
    }
    let lines = storage
        .encode()
        .into_iter()
        .map(|line| Tag::String(JavaString::from(line.as_str())))
        .collect();
    let _ = game.native_set_component(stack, COMPONENT, Tag::List(lines));
}

// --- reading the world -----------------------------------------------------

/// The block id at `pos`, none when it is not loaded.
fn block_at(game: &crate::game::Game<'_>, pos: [i32; 3]) -> Option<String> {
    if !game.world.loaded(pos[0], pos[2]) {
        return None;
    }
    let state = game.world.block(pos[0], pos[1], pos[2]);
    Some(game.res.facts().get(state).name.to_string())
}

/// The rung of the storage block at `pos`, none for any other block.
fn rung_at(game: &crate::game::Game<'_>, pos: [i32; 3]) -> Option<Variant> {
    let block = block_at(game, pos)?;
    let rung = Variant::from_block_id(&block)?;
    RUNGS.contains(&rung).then_some(rung)
}

/// A count with thousands separators, as the feedback lines and the tooltip show
/// it (`1,048,576`).
fn grouped(amount: u64) -> String {
    let digits = amount.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// One line about a storage: what it holds, of what, and how full it is.
fn summary(where_: &str, storage: &Storage) -> String {
    format!(
        "{where_}: {} / {} items in {} resources, {} free",
        grouped(storage.total()),
        grouped(storage.capacity()),
        storage.resources(),
        grouped(storage.free())
    )
}

/// The first resource a storage holds (its insertion order), with its amount.
fn first(storage: &Storage) -> Option<Entry> {
    storage.entries().first().cloned()
}

/// A resource of an item id, checked against the loaded items, with the item's
/// own component key (empty: the command names a bare id).
fn resource_of_id(game: &crate::game::Game<'_>, id: &str) -> Option<Resource> {
    game.items().get(id)?;
    Some(Resource::new(id))
}

/// Give up to `count` of `resource` to the acting player, answering how many fit;
/// what did not fit stays where it was.
fn give(game: &mut crate::game::Game<'_>, resource: &Resource, count: u64) -> u64 {
    let uuid = game.native_acting_id();
    let per = u64::from(game.max_stack(&stack_of(resource, 1))).max(1);
    let mut given = 0;
    while given < count {
        let piece = stack_of(resource, (count - given).min(per) as u32);
        let asked = u64::from(piece.count);
        match game.native_give_stack(uuid, piece) {
            None => given += asked,
            Some(rest) => {
                given += asked - u64::from(rest.count);
                break;
            }
        }
    }
    given
}

// --- the hooks -------------------------------------------------------------

/// A storage block declares a block entity (`native_block_entity`, API 15).
pub fn on_block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if Variant::from_block_id(block).is_some_and(|rung| RUNGS.contains(&rung)) {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

/// A right-click on a storage block (`native_right_click_block`, API 21): a held
/// stack goes in, an empty hand takes a stack of the first resource out.
#[allow(
    private_interfaces,
    reason = "the hook helper sits one module away, inside the game crate"
)]
pub fn on_right_click_block(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    _face: [i32; 3],
    canceled: &mut bool,
) {
    let Some(rung) = rung_at(game, pos) else {
        return;
    };
    *canceled = true;
    let held = game.held().cloned().filter(|stack| stack.count > 0);
    match held {
        Some(stack) => {
            let resource = resource_of(&stack);
            let took = game
                .native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    let took = storage.insert(resource, u64::from(stack.count));
                    write_block(entity, &storage);
                    took
                })
                .unwrap_or(0);
            if took == 0 {
                game.native_feedback(Text::literal(format!(
                    "The {} storage block is full.",
                    rung.name()
                )));
                return;
            }
            let left = stack.count - took as u32;
            let name = stack.item.to_string();
            replace_held(game, (left > 0).then_some(ItemStack { count: left, ..stack }));
            let note = free_note(game, pos);
            game.native_feedback(Text::literal(format!(
                "Stored {} x {} ({} left in hand){}",
                grouped(took),
                name,
                left,
                note
            )));
        }
        None => {
            // Two block-entity calls, no tick between: read what there is, then
            // take one stack of it (the item's stack size needs the game).
            let entry = game
                .native_with_mod_block_entity(pos, |entity| first(&read_block(entity)))
                .flatten();
            let Some(entry) = entry else {
                game.native_feedback(Text::literal(format!(
                    "The {} storage block is empty.",
                    rung.name()
                )));
                return;
            };
            let per = u64::from(game.max_stack(&stack_of(&entry.resource, 1))).max(1);
            let want = entry.amount.min(per);
            let took = game
                .native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    let took = storage.extract(&entry.resource, want);
                    write_block(entity, &storage);
                    took
                })
                .unwrap_or(0);
            if took == 0 {
                game.native_feedback(Text::literal(format!(
                    "The {} storage block is empty.",
                    rung.name()
                )));
                return;
            }
            let given = give(game, &entry.resource, took);
            if given < took {
                // What the player could not hold goes back in.
                let back = took - given;
                let resource = entry.resource.clone();
                let _ = game.native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    storage.insert(resource.clone(), back);
                    write_block(entity, &storage);
                });
            }
            let note = free_note(game, pos);
            game.native_feedback(Text::literal(format!(
                "Took {} x {}{}",
                grouped(given),
                entry.resource.id(),
                note
            )));
        }
    }
}

/// " … 1,048,576 free" for a feedback line, empty when the block is gone.
fn free_note(game: &mut crate::game::Game<'_>, pos: [i32; 3]) -> String {
    game.native_with_mod_block_entity(pos, |entity| {
        format!(
            "; {} of {} free",
            grouped(read_block(entity).free()),
            grouped(capacity(Variant::K1024))
        )
    })
    .unwrap_or_default()
}

/// Replace the acting player's held stack (their state is swapped in).
fn replace_held(game: &mut crate::game::Game<'_>, stack: Option<ItemStack>) {
    let slot = game.held_slot();
    *game.held_mut() = stack;
    if game.lan_acting.is_none() {
        game.hand_stack_replaced(slot);
    }
}

// --- commands --------------------------------------------------------------

/// Register the disk's item component before content loads (`component_types`,
/// API 65).
pub fn register_component_types(types: &mut Vec<String>) {
    types.push(COMPONENT.to_owned());
}

/// `/extradisks …` (`register_commands`, API 8). The block commands are the
/// admin interface the capacity is proved with; the disk commands work on the
/// held disk.
pub fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("extradisks info <pos:block_pos>"));
    commands.push(NativeCommand::new(
        "extradisks insert <pos:block_pos> <item:word> <count:integer>",
    ));
    commands.push(NativeCommand::new(
        "extradisks extract <pos:block_pos> <item:word> <count:integer>",
    ));
    commands.push(NativeCommand::new("extradisks disk info"));
    commands.push(NativeCommand::new(
        "extradisks disk insert <item:word> <count:integer>",
    ));
    commands.push(NativeCommand::new(
        "extradisks disk extract <item:word> <count:integer>",
    ));
}

fn parse_pos(words: &[&str]) -> Option<[i32; 3]> {
    Some([
        words.first()?.parse().ok()?,
        words.get(1)?.parse().ok()?,
        words.get(2)?.parse().ok()?,
    ])
}

/// A count argument; a negative one is refused.
fn parse_count(text: &str) -> Option<u64> {
    let count: i64 = text.parse().ok()?;
    u64::try_from(count).ok()
}

/// The `/extradisks` commands (`perform_command`, API 8).
#[allow(
    private_interfaces,
    reason = "the hook helper sits one module away, inside the game crate"
)]
pub fn on_perform_command(
    game: &mut crate::game::Game<'_>,
    _player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["extradisks", "info", x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                usage(game, "extradisks info <x> <y> <z>");
                return;
            };
            let Some(rung) = rung_at(game, pos) else {
                game.native_feedback(Text::literal("No Extra Disks storage block there."));
                return;
            };
            let line = game
                .native_with_mod_block_entity(pos, |entity| read_block(entity))
                .map(|storage| {
                    summary(
                        &format!(
                            "Extra Disks {} storage block at {},{},{}",
                            rung.name(),
                            pos[0],
                            pos[1],
                            pos[2]
                        ),
                        &storage,
                    )
                })
                .unwrap_or_else(|| "No Extra Disks storage block there.".to_owned());
            game.native_feedback(Text::literal(line));
        }
        ["extradisks", "insert", x, y, z, item, count] => {
            *ran = true;
            let (Some(pos), Some(count)) = (parse_pos(&[*x, *y, *z]), parse_count(count)) else {
                usage(game, "extradisks insert <x> <y> <z> <item> <count>");
                return;
            };
            if rung_at(game, pos).is_none() {
                game.native_feedback(Text::literal("No Extra Disks storage block there."));
                return;
            }
            let Some(resource) = resource_of_id(game, *item) else {
                game.native_feedback(Text::literal(format!("Unknown item: {item}")));
                return;
            };
            let took = game
                .native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    let took = storage.insert(resource.clone(), count);
                    write_block(entity, &storage);
                    took
                })
                .unwrap_or(0);
            let line = if took == 0 {
                format!("Refused: {item} does not fit (the storage takes no more).")
            } else {
                format!("Inserted {} x {item} (refused {}).", grouped(took), grouped(count - took))
            };
            let note = free_note(game, pos);
            game.native_feedback(Text::literal(format!("{line}{note}")));
        }
        ["extradisks", "extract", x, y, z, item, count] => {
            *ran = true;
            let (Some(pos), Some(count)) = (parse_pos(&[*x, *y, *z]), parse_count(count)) else {
                usage(game, "extradisks extract <x> <y> <z> <item> <count>");
                return;
            };
            if rung_at(game, pos).is_none() {
                game.native_feedback(Text::literal("No Extra Disks storage block there."));
                return;
            }
            let resource = Resource::new(*item);
            let took = game
                .native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    let took = storage.extract(&resource, count);
                    write_block(entity, &storage);
                    took
                })
                .unwrap_or(0);
            let given = give(game, &resource, took);
            if given < took {
                let back = took - given;
                let _ = game.native_with_mod_block_entity(pos, |entity| {
                    let mut storage = read_block(entity);
                    storage.insert(resource.clone(), back);
                    write_block(entity, &storage);
                });
            }
            let note = free_note(game, pos);
            game.native_feedback(Text::literal(format!(
                "Extracted {} x {item} (the storage had {}, {} stayed).{note}",
                grouped(given),
                grouped(took),
                grouped(took - given)
            )));
        }
        ["extradisks", "disk", rest @ ..] => {
            *ran = true;
            disk_command(game, rest);
        }
        _ => {}
    }
}

/// A wrong argument list: say how the command is used.
fn usage(game: &mut crate::game::Game<'_>, usage: &str) {
    game.native_feedback(Text::literal(format!("Usage: /{usage}")));
}

/// The `/extradisks disk …` family: the held 1024k storage disk.
fn disk_command(game: &mut crate::game::Game<'_>, words: &[&str]) {
    let Some(held) = game.held().cloned() else {
        game.native_feedback(Text::literal("Hold a 1024k storage disk first."));
        return;
    };
    if Variant::from_disk_id(&held.item) != Some(Variant::K1024) {
        game.native_feedback(Text::literal(format!(
            "Hold a 1024k storage disk first (this is {}).",
            held.item
        )));
        return;
    }
    let mut disk = held.clone();
    let mut storage = read_disk(game, &disk);
    match words {
        ["info"] => {
            game.native_feedback(Text::literal(summary("The held 1024k storage disk", &storage)));
        }
        ["insert", item, count] => {
            let Some(count) = parse_count(*count) else {
                return usage(game, "extradisks disk insert <item> <count>");
            };
            let Some(resource) = resource_of_id(game, *item) else {
                game.native_feedback(Text::literal(format!("Unknown item: {item}")));
                return;
            };
            let took = storage.insert(resource, count);
            write_disk(game, &mut disk, &storage);
            replace_held(game, Some(disk));
            if took == 0 {
                game.native_feedback(Text::literal(format!(
                    "Refused: {item} does not fit (the disk takes no more)."
                )));
            } else {
                game.native_feedback(Text::literal(format!(
                    "Inserted {} x {item} (refused {}). {}",
                    grouped(took),
                    grouped(count - took),
                    summary("The disk", &storage)
                )));
            }
        }
        ["extract", item, count] => {
            let Some(count) = parse_count(*count) else {
                return usage(game, "extradisks disk extract <item> <count>");
            };
            let resource = Resource::new(*item);
            let took = storage.extract(&resource, count);
            write_disk(game, &mut disk, &storage);
            let given = give(game, &resource, took);
            let mut disk = disk;
            if given < took {
                // Put back what the inventory could not hold.
                let mut back = read_disk(game, &disk);
                back.insert(resource.clone(), took - given);
                write_disk(game, &mut disk, &back);
            }
            replace_held(game, Some(disk.clone()));
            let line = if given < took {
                format!(
                    "Extracted {} x {item}; the inventory held no more ({} stayed on the disk).",
                    grouped(given),
                    grouped(took - given)
                )
            } else {
                format!(
                    "Extracted {} x {item}. {}",
                    grouped(given),
                    summary("The disk", &read_disk(game, &disk))
                )
            };
            game.native_feedback(Text::literal(line));
        }
        _ => {
            game.native_feedback(Text::literal(
                "Usage: /extradisks disk info|insert <item> <count>|extract <item> <count>",
            ));
        }
    }
}
