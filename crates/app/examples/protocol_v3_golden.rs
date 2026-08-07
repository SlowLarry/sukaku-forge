//! Print the canonical protocol-v3 hidden-single transcript to standard output.
//!
//! Regenerate the committed fixture from the repository root with:
//! `cargo run -q -p sukaku-forge-app --example protocol_v3_golden > apps/gui/src/fixtures/protocol-v3-hidden-single.json`

#[path = "../tests/support/protocol_v3_hidden_single.rs"]
mod protocol_v3_hidden_single;

fn main() {
    print!("{}", protocol_v3_hidden_single::render());
}
