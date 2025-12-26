use super::*;
use std::thread::sleep;

#[test]
fn test_notification_creation() {
    let n1 = Notification::new(1, NotificationType::Info, "Info msg", None);
    assert_eq!(n1.id, 1);
    assert_eq!(n1.message, "Info msg");
    assert_eq!(n1.kind, NotificationType::Info);
    assert!(n1.ttl.is_none());

    let n2 = Notification::new(
        2,
        NotificationType::Error,
        "Error msg",
        Some(Duration::from_secs(5)),
    );
    assert_eq!(n2.kind, NotificationType::Error);
    assert_eq!(n2.ttl, Some(Duration::from_secs(5)));
}

#[test]
fn test_manager_add_convenience_methods() {
    let mut manager = NotificationManager::new();

    let id1 = manager.info("Info");
    let id2 = manager.warn("Warn");
    let id3 = manager.error("Error");
    let id4 = manager.success("Success");

    assert_eq!(manager.notifications.len(), 4);

    assert_eq!(manager.notifications[0].kind, NotificationType::Info);
    assert_eq!(manager.notifications[0].id, id1);

    assert_eq!(manager.notifications[1].kind, NotificationType::Warning);
    assert_eq!(manager.notifications[1].id, id2);

    assert_eq!(manager.notifications[2].kind, NotificationType::Error);
    assert_eq!(manager.notifications[2].id, id3);

    assert_eq!(manager.notifications[3].kind, NotificationType::Success);
    assert_eq!(manager.notifications[3].id, id4);
}

#[test]
fn test_unique_ids() {
    let mut manager = NotificationManager::new();
    let id1 = manager.add(NotificationType::Info, "First", None);
    let id2 = manager.add(NotificationType::Info, "Second", None);
    let id3 = manager.add(NotificationType::Info, "Third", None);

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    assert!(id2 > id1);
    assert!(id3 > id2);
}

#[test]
fn test_remove_notification() {
    let mut manager = NotificationManager::new();
    let id1 = manager.info("1");
    let id2 = manager.info("2");
    let id3 = manager.info("3");

    manager.remove(id2);

    assert_eq!(manager.notifications.len(), 2);
    assert!(manager.notifications.iter().any(|n| n.id == id1));
    assert!(manager.notifications.iter().any(|n| n.id == id3));
    assert!(!manager.notifications.iter().any(|n| n.id == id2));

    // Removing non-existent ID should do nothing
    manager.remove(999);
    assert_eq!(manager.notifications.len(), 2);
}

#[test]
fn test_expiration_logic() {
    let now = Instant::now();
    let ttl = Duration::from_millis(50);

    let n_expired = Notification {
        id: 1,
        message: "Expired".into(),
        kind: NotificationType::Info,
        timestamp: now - Duration::from_millis(100),
        ttl: Some(ttl),
    };

    let n_active = Notification {
        id: 2,
        message: "Active".into(),
        kind: NotificationType::Info,
        timestamp: now,
        ttl: Some(ttl),
    };

    let n_perma = Notification {
        id: 3,
        message: "Permanent".into(),
        kind: NotificationType::Info,
        timestamp: now - Duration::from_secs(100),
        ttl: None,
    };

    // Use a slightly future time to ensure the 'active' check works as expected relative to creation
    let check_time = now;

    assert!(n_expired.is_expired(check_time));
    assert!(!n_active.is_expired(check_time));
    assert!(!n_perma.is_expired(check_time));
}

#[test]
fn test_prune_expired() {
    let mut manager = NotificationManager::new();

    // Add expired notification (simulated by short TTL and sleep)
    manager.add(
        NotificationType::Info,
        "Expired",
        Some(Duration::from_millis(10)),
    );

    // Add active notification (long TTL)
    manager.add(
        NotificationType::Info,
        "Active",
        Some(Duration::from_secs(10)),
    );

    // Add permanent notification
    manager.add(NotificationType::Info, "Permanent", None);

    // Sleep to let the first one expire
    sleep(Duration::from_millis(20));

    manager.prune_expired();

    assert_eq!(manager.notifications.len(), 2);
    assert!(manager.notifications.iter().any(|n| n.message == "Active"));
    assert!(manager
        .notifications
        .iter()
        .any(|n| n.message == "Permanent"));
    assert!(!manager.notifications.iter().any(|n| n.message == "Expired"));

    // Verify explicit removal
    let active_id = manager
        .notifications
        .iter()
        .find(|n| n.message == "Active")
        .unwrap()
        .id;
    manager.remove(active_id);
    assert_eq!(manager.notifications.len(), 1);
    assert!(manager
        .notifications
        .iter()
        .any(|n| n.message == "Permanent"));
}

#[test]
fn test_iter_active() {
    let mut manager = NotificationManager::new();
    manager.info("1");
    manager.info("2");

    let messages: Vec<&str> = manager.iter_active().map(|n| n.message.as_str()).collect();
    assert_eq!(messages, vec!["1", "2"]);
}

#[test]
fn test_default_ttls() {
    let mut manager = NotificationManager::new();
    manager.info("Info");
    manager.success("Success");
    manager.warn("Warn");
    manager.error("Error");

    // Verify default TTLs relative durations
    let info_ttl = manager.notifications[0].ttl.unwrap();
    let success_ttl = manager.notifications[1].ttl.unwrap();
    let warn_ttl = manager.notifications[2].ttl.unwrap();
    let error_ttl = manager.notifications[3].ttl.unwrap();

    assert_eq!(info_ttl, Duration::from_secs(5));
    assert_eq!(success_ttl, Duration::from_secs(3));
    assert_eq!(warn_ttl, Duration::from_secs(8));
    assert_eq!(error_ttl, Duration::from_secs(10));
}
