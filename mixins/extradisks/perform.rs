// The `/extradisks` command bodies.
fn perform_command(
    game: &mut crate::game::Game<'_>,
    player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    crate::game::extradisks_state::on_perform_command(game, player, command, ran);
}
