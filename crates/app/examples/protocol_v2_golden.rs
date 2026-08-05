//! Print the canonical protocol-v2 hidden-single transcript to standard output.
//!
//! Regenerate the committed fixture from the repository root with:
//! `cargo run -q -p sukaku-forge-app --example protocol_v2_golden > apps/gui/src/fixtures/protocol-v2-hidden-single.json`

#[path = "../tests/support/protocol_v2_hidden_single.rs"]
mod protocol_v2_hidden_single;

fn main() {
    print!("{}", protocol_v2_hidden_single::render());
}
