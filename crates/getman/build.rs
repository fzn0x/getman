//! Rebuilds the binary when the browser UI is rebuilt, so the UI built into
//! it is never stale. The UI itself is built with `npm run build` in `web/`.

fn main() {
    println!("cargo::rerun-if-changed=../../web/dist");
    println!("cargo::rerun-if-changed=build.rs");
}
