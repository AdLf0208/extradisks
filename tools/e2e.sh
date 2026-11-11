#!/usr/bin/env bash
# In-game check for the Extra Disks slice: the 1024k item storage block and disk.
#
# The End arena (stone floor at y=63, the block at y=64), headless, creative:
#  - the block places and reports an empty 1,048,576-item storage;
#  - a held stack right-clicked onto it goes in (and leaves the hand), and an
#    empty-hand right-click takes a stack back out;
#  - `/extradisks insert` fills it to exactly 1,048,576 items;
#  - the control: one more insert is refused and the total does not change, and
#    a new resource is refused too;
#  - a partial insert takes only what fits (576 of 1,000), so the storage is
#    full again with two resources;
#  - the held 1024k disk carries its own 1,048,576 items in its item component,
#    is refused one item past capacity, and gives 64 back to the inventory;
#  - a vanilla barrel is not a storage block;
#  - the block keeps its exact contents and resources across `save` + relaunch.
#
# SELFTEST=1 asserts that the refused insert did grow the storage (it must FAIL),
# which proves the scenario can fail.
set -euo pipefail
export LC_ALL=C

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/extradisks
CTL=${CTL:-$PORT/target/horizon-cargo/fast/horizon-ctl}
GAME=${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}
RES=${RES:-${HORIZON_RES:-${HORIZON_RES:-$HOME/.local/horizon-client/26.3}}}
SOCK=${SOCK:-/tmp/opencode/extradisks-e2e.sock}
SELFTEST=${SELFTEST:-0}
KEEP=${KEEP:-0}
# CONTROL=1 runs the embedded leg with an unusable `.horizon-cache` (a plain
# file where the bundle writes its content): the ids must then be unknown, which
# proves the block and the disk came from the bundle and not from the game.
CONTROL=${CONTROL:-0}
WORLD=ExtraDisksE2E
BLOCK=extradisks:1024k_item_storage_block
DISK=extradisks:1024k_item_storage_disk
# EMBEDDED=1 launches the native bundle alone (no --dev-mod): the shipped shape.
EMBEDDED=${EMBEDDED:-0}
DEV_ARGS=()
[ "$EMBEDDED" = 1 ] || DEV_ARGS=(--dev-mod "$PORT/content")

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d /tmp/opencode/extradisks-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
say() { ctl chat "/$1" >/dev/null; }
fail() { echo "FAIL: $*" >&2; ctl logs --lines 60 >&2 || true; exit 1; }

# The chat lines the game saw since a marker, so an unrelated line (an
# advancement, a join) cannot pass for the command's answer.
seq_mark() { ctl events | jq -r '.last'; }
new_chat() { # new_chat SEQ
    ctl events --since "$1" | jq -r '[.events[]? | select(.event == "chat") | .text] | join(" / ")'
}
expect_line() { # expect_line "expected text" COMMAND...
    local want=$1 before got
    shift
    before=$(seq_mark)
    say "$*"
    ctl wait 4 >/dev/null
    got=$(new_chat "$before")
    echo "  /$* -> $got"
    case "$got" in
        *"$want"*) ;;
        *) fail "expected '$want', got '$got'" ;;
    esac
}
count_of() { # count_of ITEM
    ctl inventory | jq -r --arg item "$1" \
        '[((.hotbar // [])[] | select(.item == $item) | .count), ((.main // [])[] | select(.item == $item) | .count)] | add // 0'
}
held() { ctl state | jq -r '.held.item // "nothing"'; }
expect_playing() {
    ctl screen | jq -e '.screen == "playing"' >/dev/null \
        || fail "not in the world: $(ctl screen | jq -c '{screen}')"
}

cleanup() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "$KEEP" = 1 ]; then
        echo "kept scratch: $SCRATCH" >&2
    else
        rm -rf "$SCRATCH" 2>/dev/null || true
    fi
    echo "EXIT:$status"
}
trap cleanup EXIT

launch() {
    rm -f "$SOCK"
    cd "$SCRATCH"
    if [ "$CONTROL" = 1 ]; then
        # A plain file where the bundle wants its cache directory: the embedded
        # content cannot be written, so nothing of this mod loads.
        : >"$SCRATCH/.horizon-cache"
    fi
    "$CTL" --socket "$SOCK" launch \
        --game "$GAME" --world "$WORLD" --seed 20261105 --creative --headless --wait \
        --resources "$RES" "${DEV_ARGS[@]}" >/dev/null
    ctl wait-ready 120
    if [ "$EMBEDDED" = 1 ] && [ "$CONTROL" = 0 ]; then
        cache=$(find "$SCRATCH" -maxdepth 3 -name "extradisks-embedded-content.hmod" 2>/dev/null | head -1)
        if [ -n "$cache" ]; then
            cmp -s "$cache" "$PORT/content/extradisks.hmod" \
                || fail "the embedded content differs from content/extradisks.hmod"
            echo "  embedded content: $(wc -c <"$cache" | tr -d ' ') B, identical to content/extradisks.hmod"
        else
            # The game consumes the written package into its own content cache;
            # the loader's startup line is then the proof that it loaded.
            grep -q 'content from 1 mod(s)' "${SOCK}.log" \
                || fail "the bundle's embedded content did not load (no 'content from 1 mod(s)' in ${SOCK}.log)"
            echo "  embedded content: loaded from the bundle (game content cache rebuilt at startup)"
        fi
    fi
    ctl wait 40
}

# The first launch only: a stone floor and a clear volume in The End.
arena() {
    ctl command "execute in minecraft:the_end run tp @s 40.5 64 3.5"
    local loaded=false
    for _ in $(seq 1 120); do
        ctl wait 5
        if [ "$(ctl block 40 63 0 | jq -r '.loaded // false')" = true ]; then
            loaded=true
            break
        fi
    done
    [ "$loaded" = true ] || fail "the End arena chunk never loaded"
    ctl command "execute in minecraft:the_end run fill 32 63 -8 56 63 24 minecraft:stone"
    ctl command "execute in minecraft:the_end run fill 32 64 -8 56 74 24 minecraft:air"
    ctl command "execute in minecraft:the_end run kill @e[type=item]"
    ctl wait 20
}

echo "== launch (The End arena, headless) =="
launch
arena
expect_playing

if [ "$CONTROL" = 1 ]; then
    echo "== the unusable .horizon-cache control: the mod's ids are unknown =="
    ctl command "execute in minecraft:the_end run setblock 40 64 0 $BLOCK"
    ctl wait 6
    ctl messages 6 | jq -r '.chat[]' | grep -q "Unknown block type '$BLOCK'" \
        || fail "the block id was known without the bundle's content: $(ctl messages 6)"
    echo "  setblock: unknown block type (as expected)"
    give=$(ctl give "$DISK" 1 || true)
    echo "  give: $give"
    jq -e --arg i "$DISK" '.error | contains("unknown item " + $i)' <<<"$give" >/dev/null \
        || fail "the disk was known without the bundle's content: $give"
    echo "PASS: with an unusable .horizon-cache the bundle's block and disk are unknown, so the content really comes from inside the .hnmod"
    exit 0
fi

echo "== the 1024k storage block places and starts empty =="
ctl command "execute in minecraft:the_end run setblock 40 64 0 $BLOCK"
ctl wait 12
ctl block 40 64 0 | jq -e --arg b "$BLOCK" '.block == $b' >/dev/null \
    || fail "the block did not place: $(ctl block 40 64 0)"
echo "  block(40,64,0): $(ctl block 40 64 0 | jq -c '{block,state}')"
expect_line "0 / 1,048,576 items in 0 resources, 1,048,576 free" extradisks info 40 64 0

echo "== a held stack right-clicked onto the block goes in, out of the hand =="
ctl give minecraft:diamond 5
ctl wait 4
ctl select minecraft:diamond
ctl wait 4
ctl command "execute in minecraft:the_end run tp @s 40.5 64 2.5"
ctl wait 4
ctl interact 40 64 0
ctl wait 8
[ "$(count_of minecraft:diamond)" = 0 ] \
    || fail "the diamonds stayed in the inventory: $(ctl inventory | jq -c '[.hotbar[] | select(.item != null)]')"
[ "$(held)" = nothing ] || fail "the hand still holds $(held)"
expect_line "5 / 1,048,576 items in 1 resources, 1,048,571 free" extradisks info 40 64 0

echo "== an empty-hand right-click takes a stack back out =="
ctl interact 40 64 0
ctl wait 8
[ "$(count_of minecraft:diamond)" = 5 ] \
    || fail "the diamonds did not come back: $(ctl inventory | jq -c '[.hotbar[] | select(.item != null)]')"
expect_line "0 / 1,048,576 items in 0 resources, 1,048,576 free" extradisks info 40 64 0
ctl command "clear @s"
ctl wait 4

echo "== the storage fills to exactly 1,048,576 items =="
expect_line "Inserted 1,048,576 x minecraft:stone (refused 0).; 0 of 1,048,576 free" \
    extradisks insert 40 64 0 minecraft:stone 1048576
expect_line "1,048,576 / 1,048,576 items in 1 resources, 0 free" extradisks info 40 64 0

echo "== the control: one more insert is refused, a new resource too =="
expect_line "Refused: minecraft:stone does not fit (the storage takes no more).; 0 of 1,048,576 free" \
    extradisks insert 40 64 0 minecraft:stone 64
expect_line "Refused: minecraft:dirt does not fit (the storage takes no more).; 0 of 1,048,576 free" \
    extradisks insert 40 64 0 minecraft:dirt 1
if [ "$SELFTEST" = 1 ]; then
    echo "(selftest: asserting the refused insert grew the storage)"
    expect_line "1,048,640 / 1,048,576 items" extradisks info 40 64 0
fi
expect_line "1,048,576 / 1,048,576 items in 1 resources, 0 free" extradisks info 40 64 0

echo "== a partial insert takes what fits (576 of 1,000) =="
expect_line "Extracted 576 x minecraft:stone (the storage had 576, 0 stayed).; 576 of 1,048,576 free" \
    extradisks extract 40 64 0 minecraft:stone 576
expect_line "Inserted 576 x minecraft:dirt (refused 424).; 0 of 1,048,576 free" \
    extradisks insert 40 64 0 minecraft:dirt 1000
expect_line "1,048,576 / 1,048,576 items in 2 resources, 0 free" extradisks info 40 64 0
ctl command "clear @s"
ctl wait 4

echo "== the held 1024k disk carries its own 1,048,576 items =="
ctl give "$DISK" 1
ctl wait 4
ctl select "$DISK"
ctl wait 4
expect_line "The held 1024k storage disk: 0 / 1,048,576 items in 0 resources, 1,048,576 free" \
    extradisks disk info
expect_line "Inserted 1,048,576 x minecraft:diamond (refused 0). The disk: 1,048,576 / 1,048,576 items in 1 resources, 0 free" \
    extradisks disk insert minecraft:diamond 1048576
expect_line "Refused: minecraft:diamond does not fit (the disk takes no more)." \
    extradisks disk insert minecraft:diamond 1
expect_line "Extracted 64 x minecraft:diamond. The disk: 1,048,512 / 1,048,576 items in 1 resources, 64 free" \
    extradisks disk extract minecraft:diamond 64
[ "$(count_of minecraft:diamond)" = 64 ] \
    || fail "the extracted diamonds are not in the inventory: $(ctl inventory | jq -c '[.hotbar[] | select(.item != null)]')"
# The disk is the only 1024k disk in the hand; its component is what carries the
# 1,048,512 diamonds, so a second read of the same item must agree.
expect_line "The held 1024k storage disk: 1,048,512 / 1,048,576 items in 1 resources, 64 free" \
    extradisks disk info
ctl command "clear @s"
ctl wait 4

echo "== a vanilla barrel is not a storage block =="
ctl command "execute in minecraft:the_end run setblock 56 64 0 minecraft:barrel"
ctl wait 6
expect_line "No Extra Disks storage block there." extradisks info 56 64 0

echo "== the block keeps its exact contents across a relaunch =="
ctl command "execute in minecraft:the_end run tp @s 40.5 64 2.5"
ctl save
ctl wait 10
ctl quit
sleep 3
launch
ctl wait 20
expect_playing
expect_line "1,048,576 / 1,048,576 items in 2 resources, 0 free" extradisks info 40 64 0
# ... and the block still works: take 576 dirt back out.
expect_line "Extracted 576 x minecraft:dirt (the storage had 576, 0 stayed).; 576 of 1,048,576 free" \
    extradisks extract 40 64 0 minecraft:dirt 576
[ "$(count_of minecraft:dirt)" = 576 ] \
    || fail "the dirt did not come back after the relaunch: $(ctl inventory | jq -c '[.hotbar[] | select(.item != null)]')"
expect_line "1,048,000 / 1,048,576 items in 1 resources, 576 free" extradisks info 40 64 0

echo "PASS: the 1024k storage block took a held stack and gave it back, filled to exactly 1,048,576 items, refused one more and a new resource, took 576 of 1,000 in a partial insert, the held 1024k disk carried its own 1,048,576 diamonds and gave 64 back, a vanilla barrel is not a storage block, and the block kept its exact contents across a relaunch"
# The cleanup trap prints EXIT:<status> as the last line.
