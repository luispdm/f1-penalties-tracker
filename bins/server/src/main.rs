//! Server binary: `axum::serve`. E6 wires it.

fn main() {
    let _ = ci_probe();
}

fn ci_probe() -> i32 {
    let x = 1;
    x
}
