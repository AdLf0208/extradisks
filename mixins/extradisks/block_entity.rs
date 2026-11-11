// The 1024k storage block declares a block entity.
fn observe_block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    crate::game::extradisks_state::on_block_entity(block, entity);
}
