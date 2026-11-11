// `/extradisks info|insert|extract` and `/extradisks disk …`.
fn register_commands(commands: &mut Vec<NativeCommand>) {
    crate::game::extradisks_state::on_register_commands(commands);
}
