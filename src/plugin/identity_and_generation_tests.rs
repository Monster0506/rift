use super::*;

#[test]
fn plugin_id_monotonic_and_nonzero() {
    let id1 = PluginId::allocate();
    let id2 = PluginId::allocate();
    assert!(id1.get() > 0);
    assert!(id2.get() > id1.get());
    assert_eq!(id1.as_nonzero().get(), id1.get());
}

#[test]
fn plugin_generation_monotonic_and_distinct() {
    let pid1 = PluginId::allocate();
    let pid2 = PluginId::allocate();
    let gen1 = PluginGeneration::allocate(pid1);
    let gen2 = PluginGeneration::allocate(pid1);
    let gen3 = PluginGeneration::allocate(pid2);

    assert_eq!(gen1.plugin_id(), pid1);
    assert_eq!(gen2.plugin_id(), pid1);
    assert_eq!(gen3.plugin_id(), pid2);

    assert!(gen2.generation() > gen1.generation());
    assert!(gen3.generation() > gen2.generation());
    assert_ne!(gen1, gen2);
    assert_ne!(gen2, gen3);

    let (raw_pid, raw_gen) = gen1.raw();
    assert_eq!(raw_pid, pid1.get());
    assert_eq!(raw_gen, gen1.generation().get());
}

#[test]
fn host_plugin_registration_and_lookup() {
    let mut host = PluginHost::new(1);
    let id1 = host.register_plugin("git-blame");
    let id2 = host.register_plugin("git-blame");
    let id3 = host.register_plugin("markdown");

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
    assert_eq!(host.plugin_id_for_name("git-blame"), Some(id1));
    assert_eq!(host.plugin_name(id1), Some("git-blame"));
    assert_eq!(host.plugin_name(id3), Some("markdown"));
}

#[test]
fn host_generation_lifecycle_and_mutation_filtering() {
    let mut host = PluginHost::new(1);
    let pid = host.register_plugin("test-plugin");
    let gen = host.new_generation(pid);

    assert!(host.is_generation_active(gen));
    assert_eq!(host.generation_status(gen), Some(GenerationStatus::Active));

    // Native mutation is always queued
    host.queue_mutation(PluginMutation::CloseFloat);

    // Active generation mutation is accepted
    assert!(
        host.queue_mutation_with_origin(PluginMutation::InsertAtCursor("active".to_string()), gen,)
    );
    assert_eq!(host.queued_mutation_count(), 2);

    // Mark retiring
    assert!(host.mark_generation_retiring(gen));
    assert!(host.is_generation_retiring(gen));

    // New mutation from retiring generation is rejected
    assert!(!host
        .queue_mutation_with_origin(PluginMutation::InsertAtCursor("rejected".to_string()), gen,));
    assert_eq!(host.queued_mutation_count(), 2);

    // Drain discards the retiring generation's mutation and preserves native
    let drained: Vec<PluginMutation> = host.drain_mutations().collect();
    assert_eq!(drained.len(), 1);
    assert!(matches!(drained[0], PluginMutation::CloseFloat));
}

#[test]
fn discard_mutations_for_generation_explicitly() {
    let mut host = PluginHost::new(1);
    let pid1 = host.register_plugin("p1");
    let pid2 = host.register_plugin("p2");
    let gen1 = host.new_generation(pid1);
    let gen2 = host.new_generation(pid2);

    host.queue_mutation_with_origin(PluginMutation::CloseFloat, gen1);
    host.queue_mutation_with_origin(PluginMutation::SaveBuffer, gen2);
    host.queue_mutation(PluginMutation::SwapWindows);

    assert_eq!(host.queued_mutation_count(), 3);
    let discarded = host.discard_mutations_for_generation(gen1);
    assert_eq!(discarded, 1);
    assert_eq!(host.queued_mutation_count(), 2);

    let drained: Vec<PluginMutation> = host.drain_mutations().collect();
    assert_eq!(drained.len(), 2);
    assert!(matches!(drained[0], PluginMutation::SaveBuffer));
    assert!(matches!(drained[1], PluginMutation::SwapWindows));
}

#[test]
fn retiring_generation_unregisters_handlers() {
    let mut host = PluginHost::new(1);
    let pid = host.register_plugin("p");
    let gen = host.new_generation(pid);

    host.register_command_with_generation("cmd", gen, |_| vec![PluginMutation::SwapWindows]);
    host.register_action_with_generation("act", gen, || vec![PluginMutation::SwapWindows]);

    assert!(host.has_command("cmd"));
    assert!(host.execute_command("cmd", &[]));
    assert!(host.execute_action("act"));
    assert_eq!(host.queued_mutation_count(), 2);

    // Retire the generation
    host.retire_generation(gen);
    assert!(host.is_generation_retired(gen));

    // Handlers are unregistered
    assert!(!host.has_command("cmd"));
    assert!(!host.execute_command("cmd", &[]));
    assert!(!host.execute_action("act"));
}
