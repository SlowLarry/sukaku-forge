use std::io;

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    sukaku_forge_core::write_all_java_topologies(&mut output)
}
