#[path = "support/protocol_v3_hidden_single.rs"]
mod protocol_v3_hidden_single;

const COMMITTED_FIXTURE: &str =
    include_str!("../../../apps/gui/src/fixtures/protocol-v3-hidden-single.json");

#[test]
fn rust_dispatch_sequence_exactly_matches_the_committed_gui_fixture() {
    assert_eq!(COMMITTED_FIXTURE, protocol_v3_hidden_single::render());
}
