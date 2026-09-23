use super::*;
use crate::job_manager::CancellationSignal;

fn fresh_signal() -> CancellationSignal {
    CancellationSignal::new_uncancelled()
}

#[test]
fn copy_recursive_pub_errors_when_destination_is_inside_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mydir");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), "hello").unwrap();

    // A destination inside the source directory must be rejected.
    let destination = source.join("sub");
    let result = FsCopyJob::copy_recursive_pub(&source, &destination);
    assert!(
        result.is_err(),
        "copy_recursive_pub must error when destination is inside source"
    );
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn copy_recursive_pub_copies_file_successfully() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.txt");
    std::fs::write(&src, "world").unwrap();
    let dst = dir.path().join("hello_copy.txt");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "world");
}

#[test]
fn copy_recursive_pub_copies_directory_successfully() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("srcdir");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("a.txt"), "aaa").unwrap();
    let dst = dir.path().join("dstdir");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "aaa");
}

#[test]
fn copy_recursive_pub_copies_nested_directory() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("outer");
    std::fs::create_dir_all(src.join("inner")).unwrap();
    std::fs::write(src.join("inner").join("deep.txt"), "deep").unwrap();
    let dst = dir.path().join("copy");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(
        std::fs::read_to_string(dst.join("inner").join("deep.txt")).unwrap(),
        "deep"
    );
}

#[test]
fn copy_recursive_pub_copies_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("empty");
    std::fs::create_dir(&src).unwrap();
    let dst = dir.path().join("empty_copy");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert!(
        dst.is_dir(),
        "empty directory must be created at destination"
    );
}

#[test]
fn copy_recursive_pub_errors_on_nonexistent_source() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("does_not_exist");
    let dst = dir.path().join("dst");
    let result = FsCopyJob::copy_recursive_pub(&src, &dst);
    // Source doesn't exist: is_dir() returns false, falls into the file branch,
    // then fs::copy fails because the source path doesn't exist.
    assert!(result.is_err(), "missing source must return an error");
}

#[test]
fn copy_recursive_pub_file_to_existing_destination_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    std::fs::write(&src, "new content").unwrap();
    std::fs::write(&dst, "old content").unwrap();
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new content");
}

#[test]
fn copy_recursive_pub_preserves_file_contents_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("binary.bin");
    let data: Vec<u8> = (0u8..=255).collect();
    std::fs::write(&src, &data).unwrap();
    let dst = dir.path().join("binary_copy.bin");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), data);
}

#[test]
fn copy_recursive_pub_destination_prefix_not_confused_with_inside_source() {
    // "src_extra" shares the prefix but is outside "src", so allow it.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("f.txt"), "hi").unwrap();
    let dst = dir.path().join("src_extra");
    FsCopyJob::copy_recursive_pub(&src, &dst).unwrap();
    assert_eq!(std::fs::read_to_string(dst.join("f.txt")).unwrap(), "hi");
}

#[cfg(unix)]
fn make_self_referential_symlink(src: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, src.join("self_link"))
}

#[cfg(windows)]
fn make_self_referential_symlink(src: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(src, src.join("self_link"))
}

#[test]
fn copy_recursive_does_not_loop_on_self_referential_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("looped");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("real.txt"), "data").unwrap();
    // Symlink inside src pointing back at src itself.
    if make_self_referential_symlink(&src).is_err() {
        // No symlink privilege on this machine (common on Windows CI); skip.
        return;
    }

    let dst = dir.path().join("looped_copy");
    let signal = fresh_signal();
    let mut visited = HashSet::new();
    let result = FsCopyJob::copy_recursive(&src, &dst, &signal, &mut visited);

    assert!(
        result.is_ok(),
        "copy must terminate without error on a self-referential symlink: {:?}",
        result.err()
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("real.txt")).unwrap(),
        "data"
    );
    let link_meta = std::fs::symlink_metadata(dst.join("self_link")).unwrap();
    assert!(
        link_meta.file_type().is_symlink(),
        "self_link must be recreated as a symlink, not recursed into"
    );
}
