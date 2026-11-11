// A stack held onto a storage block goes in; an empty hand takes one out. Every
// other block's click is left alone.
fn observe_right_click_block(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::extradisks_state::on_right_click_block(game, pos, face, canceled);
}
