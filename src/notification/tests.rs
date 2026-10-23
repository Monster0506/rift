use super::*;
use std::thread::sleep;

#[test]
fn test_job_event_kind_eq() {
    assert_eq!(JobEventKind::Started, JobEventKind::Started);
    assert_eq!(JobEventKind::Finished, JobEventKind::Finished);
    assert_eq!(JobEventKind::Error, JobEventKind::Error);
    assert_eq!(JobEventKind::Cancelled, JobEventKind::Cancelled);
    assert_eq!(JobEventKind::Progress(50), JobEventKind::Progress(50));
    assert_ne!(JobEventKind::Progress(10), JobEventKind::Progress(20));
    assert_ne!(JobEventKind::Started, JobEventKind::Finished);
}

#[test]
fn test_job_event_kind_debug() {
    assert_eq!(format!("{:?}", JobEventKind::Started), "Started");
    assert_eq!(format!("{:?}", JobEventKind::Progress(75)), "Progress(75)");
    assert_eq!(format!("{:?}", JobEventKind::Cancelled), "Cancelled");
}

#[test]
fn test_message_entry_notification_time() {
    let before = SystemTime::now();
    let mut mgr = NotificationManager::new();
    mgr.info("hello");
    let after = SystemTime::now();

    let log = mgr.message_log();
    assert_eq!(log.len(), 1);
    let t = log[0].time();
    assert!(t >= before);
    assert!(t <= after);
}

#[test]
fn test_message_entry_job_event_time() {
    let before = SystemTime::now();
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(1, JobEventKind::Started, false, "file-save: started");
    let after = SystemTime::now();

    let log = mgr.message_log();
    assert_eq!(log.len(), 1);
    let t = log[0].time();
    assert!(t >= before);
    assert!(t <= after);
}

#[test]
fn test_log_job_event_appends_correctly() {
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(1, JobEventKind::Started, false, "fs-copy: started");
    mgr.log_job_event(1, JobEventKind::Progress(50), false, "fs-copy: halfway");
    mgr.log_job_event(1, JobEventKind::Finished, false, "fs-copy: finished");

    let log = mgr.message_log();
    assert_eq!(log.len(), 3);

    match &log[0] {
        MessageEntry::JobEvent {
            job_id,
            kind,
            silent,
            message,
            ..
        } => {
            assert_eq!(*job_id, 1);
            assert_eq!(*kind, JobEventKind::Started);
            assert!(!silent);
            assert_eq!(message, "fs-copy: started");
        }
        MessageEntry::Notification { .. } => panic!("expected JobEvent"),
    }
    match &log[2] {
        MessageEntry::JobEvent { kind, message, .. } => {
            assert_eq!(*kind, JobEventKind::Finished);
            assert_eq!(message, "fs-copy: finished");
        }
        MessageEntry::Notification { .. } => panic!("expected JobEvent"),
    }
}

#[test]
fn test_log_job_event_silent_flag() {
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(7, JobEventKind::Started, true, "cache-warming: started");

    match &mgr.message_log()[0] {
        MessageEntry::JobEvent { silent, .. } => assert!(*silent),
        MessageEntry::Notification { .. } => panic!("expected JobEvent"),
    }
}

#[test]
fn test_log_mixes_notifications_and_job_events() {
    let mut mgr = NotificationManager::new();
    mgr.info("File saved");
    mgr.log_job_event(2, JobEventKind::Started, false, "file-save: started");
    mgr.warn("Disk almost full");
    mgr.log_job_event(2, JobEventKind::Finished, false, "file-save: finished");

    let log = mgr.message_log();
    assert_eq!(log.len(), 4);
    assert!(matches!(log[0], MessageEntry::Notification { .. }));
    assert!(matches!(log[1], MessageEntry::JobEvent { .. }));
    assert!(matches!(log[2], MessageEntry::Notification { .. }));
    assert!(matches!(log[3], MessageEntry::JobEvent { .. }));
}

#[test]
fn test_notification_add_also_logs() {
    let mut mgr = NotificationManager::new();
    mgr.info("Test");
    mgr.error("Oops");

    let log = mgr.message_log();
    assert_eq!(log.len(), 2);

    match &log[0] {
        MessageEntry::Notification { kind, message, .. } => {
            assert_eq!(*kind, NotificationType::Info);
            assert_eq!(message, "Test");
        }
        MessageEntry::JobEvent { .. } => panic!("expected Notification"),
    }
    match &log[1] {
        MessageEntry::Notification { kind, message, .. } => {
            assert_eq!(*kind, NotificationType::Error);
            assert_eq!(message, "Oops");
        }
        MessageEntry::JobEvent { .. } => panic!("expected Notification"),
    }
}

#[test]
fn test_log_job_event_increments_generation() {
    let mut mgr = NotificationManager::new();
    let gen_before = mgr.generation;
    mgr.log_job_event(1, JobEventKind::Started, false, "job: started");
    assert_eq!(mgr.generation, gen_before + 1);
    mgr.log_job_event(1, JobEventKind::Finished, false, "job: finished");
    assert_eq!(mgr.generation, gen_before + 2);
}

#[test]
fn test_message_log_never_pruned_by_prune_expired() {
    let mut mgr = NotificationManager::new();
    mgr.add(
        NotificationType::Info,
        "expiring",
        Some(Duration::from_millis(1)),
    );
    mgr.log_job_event(1, JobEventKind::Started, false, "job: started");

    sleep(Duration::from_millis(10));
    mgr.prune_expired();

    // Active notifications pruned, but message log is persistent
    assert_eq!(mgr.notifications.len(), 0);
    assert_eq!(mgr.message_log().len(), 2);
}

#[test]
fn test_message_log_clear_all_does_not_affect_log() {
    let mut mgr = NotificationManager::new();
    mgr.info("msg1");
    mgr.info("msg2");
    mgr.clear_all();

    assert_eq!(mgr.notifications.len(), 0);
    assert_eq!(mgr.message_log().len(), 2); // log is persistent
}

#[test]
fn test_message_log_bounded_under_heavy_progress_ticks() {
    let mut mgr = NotificationManager::new();
    let tick_count = 5000;
    for i in 0..tick_count {
        mgr.log_job_event(1, JobEventKind::Progress(i % 100), false, "tick");
    }

    assert!(
        mgr.message_log().len() < tick_count as usize,
        "message_log grew unbounded: {} entries for {} ticks",
        mgr.message_log().len(),
        tick_count
    );
}

#[test]
fn test_message_log_coalesces_progress_per_job() {
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(1, JobEventKind::Started, false, "job: started");
    for pct in 0..50 {
        mgr.log_job_event(1, JobEventKind::Progress(pct), false, "job: progress");
    }

    let log = mgr.message_log();
    let progress_entries: Vec<_> = log
        .iter()
        .filter(|e| {
            matches!(
                e,
                MessageEntry::JobEvent {
                    job_id: 1,
                    kind: JobEventKind::Progress(_),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(progress_entries.len(), 1);
    match progress_entries[0] {
        MessageEntry::JobEvent { kind, .. } => assert_eq!(*kind, JobEventKind::Progress(49)),
        MessageEntry::Notification { .. } => panic!("expected JobEvent"),
    }
}

#[test]
fn test_progress_job_event_stores_percentage() {
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(3, JobEventKind::Progress(42), false, "fs-copy: halfway");

    match &mgr.message_log()[0] {
        MessageEntry::JobEvent { kind, .. } => {
            assert_eq!(*kind, JobEventKind::Progress(42));
        }
        MessageEntry::Notification { .. } => panic!("expected JobEvent"),
    }
}

#[test]
fn test_multiple_jobs_tracked_independently() {
    let mut mgr = NotificationManager::new();
    mgr.log_job_event(1, JobEventKind::Started, true, "syntax-parse: started");
    mgr.log_job_event(2, JobEventKind::Started, false, "fs-copy: started");
    mgr.log_job_event(1, JobEventKind::Finished, true, "syntax-parse: finished");
    mgr.log_job_event(2, JobEventKind::Finished, false, "fs-copy: finished");

    let log = mgr.message_log();
    let job1_count = log
        .iter()
        .filter(|e| matches!(e, MessageEntry::JobEvent { job_id: 1, .. }))
        .count();
    let job2_count = log
        .iter()
        .filter(|e| matches!(e, MessageEntry::JobEvent { job_id: 2, .. }))
        .count();
    assert_eq!(job1_count, 2);
    assert_eq!(job2_count, 2);
}

#[test]
fn test_notification_creation() {
    let n1 = Notification::new(NotificationType::Info, "Info msg", None);
    assert_eq!(n1.message, "Info msg");
    assert_eq!(n1.kind, NotificationType::Info);
    assert!(n1.ttl.is_none());

    let n2 = Notification::new(
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

    manager.info("Info");
    manager.warn("Warn");
    manager.error("Error");
    manager.success("Success");

    assert_eq!(manager.notifications.len(), 4);

    assert_eq!(manager.notifications[0].kind, NotificationType::Info);

    assert_eq!(manager.notifications[1].kind, NotificationType::Warning);

    assert_eq!(manager.notifications[2].kind, NotificationType::Error);

    assert_eq!(manager.notifications[3].kind, NotificationType::Success);
}

#[test]
fn test_expiration_logic() {
    let now = Instant::now();
    let ttl = Duration::from_millis(50);

    let n_expired = Notification {
        message: "Expired".into(),
        kind: NotificationType::Info,
        timestamp: now.checked_sub(Duration::from_millis(100)).unwrap(),
        ttl: Some(ttl),
    };

    let n_active = Notification {
        message: "Active".into(),
        kind: NotificationType::Info,
        timestamp: now,
        ttl: Some(ttl),
    };

    let n_perma = Notification {
        message: "Permanent".into(),
        kind: NotificationType::Info,
        timestamp: now.checked_sub(Duration::from_secs(100)).unwrap(),
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

#[test]
fn test_clear_last() {
    let mut manager = NotificationManager::new();
    manager.info("1");
    manager.info("2");
    manager.info("3");

    manager.clear_last();
    assert_eq!(manager.notifications.len(), 2);
    assert_eq!(manager.notifications.last().unwrap().message, "2");

    manager.clear_last();
    assert_eq!(manager.notifications.len(), 1);
    assert_eq!(manager.notifications.last().unwrap().message, "1");

    manager.clear_last();
    assert!(manager.notifications.is_empty());

    // Should not panic on empty
    manager.clear_last();
    assert!(manager.notifications.is_empty());
}

#[test]
fn test_clear_all() {
    let mut manager = NotificationManager::new();
    manager.info("1");
    manager.info("2");
    manager.info("3");

    manager.clear_all();
    assert!(manager.notifications.is_empty());

    // Should not panic on empty
    manager.clear_all();
    assert!(manager.notifications.is_empty());
}
