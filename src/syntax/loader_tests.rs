use super::*;

#[cfg(windows)]
const ALWAYS_LOADED_LIB: &str = "kernel32.dll";
#[cfg(all(unix, not(target_os = "macos")))]
const ALWAYS_LOADED_LIB: &str = "libc.so.6";
#[cfg(target_os = "macos")]
const ALWAYS_LOADED_LIB: &str = "libSystem.B.dylib";

#[test]
fn arc_rawlib_outlives_original_storage_slot() {
    let lib = unsafe { RawLib::open(ALWAYS_LOADED_LIB) }.expect("open a system library");
    let lib = Arc::new(lib);

    let mut loaded_libs: Vec<Arc<RawLib>> = vec![lib.clone()];
    let handed_out: Arc<RawLib> = lib.clone();
    drop(lib);

    assert_eq!(Arc::strong_count(&handed_out), 2);

    loaded_libs.clear();
    drop(loaded_libs);

    assert_eq!(
        Arc::strong_count(&handed_out),
        1,
        "the library must still be alive via the outstanding Arc clone"
    );
}
