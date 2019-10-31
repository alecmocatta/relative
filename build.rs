// See also: https://github.com/rayon-rs/rayon/blob/fc69e50f298b2f5fa2ce9be27827a0850f3bc8f2/rayon-core/build.rs
//
// We need a build script to use `links = "relative"`.  But we're not
// *actually* linking to anything, just making sure that we're the only
// relative in use.
fn main() {
	// we don't need to rebuild for anything else
	println!("cargo:rerun-if-changed=build.rs");
}
