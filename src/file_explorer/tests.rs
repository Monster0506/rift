use super::*;
use crate::component::EventResult;
use crate::key::Key;
use std::path::PathBuf;

#[test]
fn test_new() {
    let path = PathBuf::from("./");
    let explorer = FileExplorer::new(path.clone());
    assert_eq!(explorer.current_path, path);
    assert!(!explorer.show_hidden);
}

#[test]
fn test_create_list_job() {
    let path = PathBuf::from("./");
    let explorer = FileExplorer::new(path);
    let job = explorer.create_list_job();
    // Cannot easily inspect Job, but we can check if it compiles and runs without panic
    assert!(format!("{:?}", job).contains("DirectoryListJob"));
}

#[test]
fn test_with_colors() {
    let path = PathBuf::from("./");
    let explorer = FileExplorer::new(path).with_colors(
        Some(crate::color::Color::Red),
        Some(crate::color::Color::Blue),
    );
    assert_eq!(explorer.fg, Some(crate::color::Color::Red));
    assert_eq!(explorer.bg, Some(crate::color::Color::Blue));
}

#[test]
fn test_handle_input_esc() {
    let mut explorer = FileExplorer::new(PathBuf::from("./"));
    let res = explorer.handle_input(Key::Escape);
    match res {
        EventResult::Action(action) => {
            // Use as_any() to downcast
            if action.as_any().is::<ExplorerAction>() {
                // Verified it's an ExplorerAction
                assert!(true);
            } else {
                panic!("Expected ExplorerAction");
            }
        }
        _ => panic!("Expected Action"),
    }
}
