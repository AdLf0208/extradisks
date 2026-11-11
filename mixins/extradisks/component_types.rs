// Before content loads: register `extradisks:storage`, which carries a disk's
// contents in the item.
fn register_component_types(types: &mut Vec<String>) {
    crate::game::extradisks_state::register_component_types(types);
}
