//! The migrations are embedded in the binary when it is compiled, so a migration added or changed compiles the
//! package again.

fn main() {
  println!("cargo:rerun-if-changed=migrations");
}
