//! Tests for Rift error handling system

use super::*;
use std::io;

#[test]
fn test_error_severity_display() {
    assert_eq!(format!("{}", ErrorSeverity::Info), "INFO");
    assert_eq!(format!("{}", ErrorSeverity::Warning), "WARN");
    assert_eq!(format!("{}", ErrorSeverity::Error), "ERROR");
    assert_eq!(format!("{}", ErrorSeverity::Critical), "CRITICAL");
}

#[test]
fn test_error_severity_ordering() {
    assert!(ErrorSeverity::Info < ErrorSeverity::Warning);
    assert!(ErrorSeverity::Warning < ErrorSeverity::Error);
    assert!(ErrorSeverity::Error < ErrorSeverity::Critical);
    assert!(ErrorSeverity::Critical > ErrorSeverity::Info);
}

#[test]
fn test_error_type_display() {
    assert_eq!(format!("{}", ErrorType::Io), "IO");
    assert_eq!(format!("{}", ErrorType::Parse), "Parse");
    assert_eq!(format!("{}", ErrorType::Settings), "Settings");
    assert_eq!(format!("{}", ErrorType::Execution), "Execution");
    assert_eq!(format!("{}", ErrorType::Renderer), "Renderer");
    assert_eq!(format!("{}", ErrorType::Internal), "Internal");
    assert_eq!(format!("{}", ErrorType::Other), "Other");
}

#[test]
fn test_rift_error_new() {
    let err = RiftError::new(ErrorType::Io, "E001", "test msg");
    assert_eq!(err.severity, ErrorSeverity::Error);
    assert_eq!(err.kind, ErrorType::Io);
    assert_eq!(err.code, "E001");
    assert_eq!(err.message, "test msg");
}

#[test]
fn test_rift_error_critical() {
    let err = RiftError::critical(ErrorType::Internal, "PANIC", "system crash");
    assert_eq!(err.severity, ErrorSeverity::Critical);
    assert_eq!(err.kind, ErrorType::Internal);
    assert_eq!(err.code, "PANIC");
    assert_eq!(err.message, "system crash");
}

#[test]
fn test_rift_error_warning() {
    let err = RiftError::warning(ErrorType::Settings, "W001", "low memory");
    assert_eq!(err.severity, ErrorSeverity::Warning);
    assert_eq!(err.kind, ErrorType::Settings);
    assert_eq!(err.code, "W001");
    assert_eq!(err.message, "low memory");
}

#[test]
fn test_rift_error_display() {
    let err = RiftError::new(ErrorType::Io, "E001", "test msg");
    assert_eq!(format!("{}", err), "[ERROR] IO(E001): test msg");
}

#[test]
fn test_rift_error_contains_msg() {
    let err = RiftError::new(ErrorType::Other, "E", "the quick brown fox");
    assert!(err.contains_msg("quick"));
    assert!(err.contains_msg("brown"));
    assert!(!err.contains_msg("lazy"));
}

#[test]
fn test_contains_msg_edge_cases() {
    let err = RiftError::new(ErrorType::Other, "E", "exact");
    assert!(err.contains_msg("exact")); // Exact match
    assert!(err.contains_msg("")); // All strings contain empty string
    assert!(!err.contains_msg("ext")); // Subsequence but not substring
}

#[test]
fn test_result_alias() {
    fn produce_error() -> Result<()> {
        Err(RiftError::new(ErrorType::Other, "FAIL", "reason"))
    }

    let res = produce_error();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "FAIL");
}

#[test]
fn test_from_conversions() {
    let err_string: RiftError = "string error".to_string().into();
    assert_eq!(err_string.code, "GENERIC_ERROR");
    assert_eq!(err_string.message, "string error");

    let err_str: RiftError = "str error".into();
    assert_eq!(err_str.message, "str error");

    let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
    let err_io: RiftError = io_err.into();
    assert_eq!(err_io.kind, ErrorType::Io);
    assert_eq!(err_io.code, "IO_ERROR");
}

#[test]
fn test_from_io_error_kinds() {
    let kinds = vec![
        (io::ErrorKind::NotFound, "not found"),
        (io::ErrorKind::PermissionDenied, "denied"),
        (io::ErrorKind::AlreadyExists, "exists"),
    ];

    for (kind, msg) in kinds {
        let io_err = io::Error::new(kind, msg);
        let err: RiftError = io_err.into();
        assert_eq!(err.kind, ErrorType::Io);
        assert_eq!(err.code, "IO_ERROR");
        assert!(err.message.contains(msg));
    }
}

#[test]
fn test_rift_error_traits() {
    let err1 = RiftError::new(ErrorType::Io, "E1", "msg");
    let err2 = RiftError::new(ErrorType::Io, "E1", "msg");
    let err3 = RiftError::new(ErrorType::Io, "E2", "msg");

    // PartialEq
    assert_eq!(err1, err2);
    assert_ne!(err1, err3);

    // std::error::Error
    let std_err: &dyn std::error::Error = &err1;
    assert_eq!(format!("{}", std_err), "[ERROR] IO(E1): msg");
}

#[test]
fn test_from_str_conversion() {
    let err: RiftError = "literal error".into();
    assert_eq!(err.severity, ErrorSeverity::Error);
    assert_eq!(err.kind, ErrorType::Other);
    assert_eq!(err.code, "GENERIC_ERROR");
    assert_eq!(err.message, "literal error");
}

#[test]
fn test_error_severity_extremes() {
    let severities = vec![
        ErrorSeverity::Info,
        ErrorSeverity::Warning,
        ErrorSeverity::Error,
        ErrorSeverity::Critical,
    ];

    for s in severities {
        assert_eq!(s, s.clone());
    }
}
#[test]
fn test_error_manager_handle_sets_ttl() {
    let mut manager = manager::ErrorManager::new();
    let err = RiftError::new(ErrorType::Io, "E1", "io error");

    manager.handle(err);

    let notifications: Vec<_> = manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 1);
    // Standard error should have 10s TTL
    assert!(notifications[0].ttl.is_some());
    assert_eq!(notifications[0].ttl.unwrap().as_secs(), 10);

    // Warning should have 8s TTL
    let warn = RiftError::warning(ErrorType::Settings, "W1", "warn");
    manager.handle(warn);
    let notifications: Vec<_> = manager.notifications().iter_active().collect();
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[1].ttl.unwrap().as_secs(), 8);
}
