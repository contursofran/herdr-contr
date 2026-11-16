// Sets the build stamp (HERDR_CONTR_BUILD) for `--version`.
include!("../build-stamp.rs");

fn main() {
    build_stamp();
}
