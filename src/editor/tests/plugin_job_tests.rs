use super::common::*;
use super::*;
use crate::job_manager::jobs::plugin_process::ExitProbe;
use crate::test_utils::MockTerminal;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(10);
const CHILD_ENV: &str = "RIFT_PLUGIN_JOB_CHILD";

fn lua_editor() -> Editor<MockTerminal> {
    let editor = create_editor();
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    editor
}

fn run_lua(editor: &mut Editor<MockTerminal>, code: &str) {
    let err = editor.plugin_host.lua_exec(code);
    assert!(err.is_none(), "{err:?}");
    editor.apply_plugin_mutations();
}

fn lua_holds(editor: &Editor<MockTerminal>, condition: &str) -> bool {
    editor
        .plugin_host
        .lua_exec(&format!("assert({condition})"))
        .is_none()
}

fn tick_until(editor: &mut Editor<MockTerminal>, condition: &str) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        editor.tick().unwrap();
        if lua_holds(editor, condition) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {condition}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_exit(probe: &ExitProbe) {
    let deadline = Instant::now() + DEADLINE;
    while !probe.has_exited() {
        assert!(
            Instant::now() < deadline,
            "plugin job process was not killed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn only_probe(editor: &Editor<MockTerminal>) -> ExitProbe {
    assert_eq!(editor.plugin_jobs.len(), 1);
    editor
        .plugin_jobs
        .values()
        .next()
        .unwrap()
        .handle
        .exit_probe()
}

fn notification_count(editor: &Editor<MockTerminal>, needle: &str) -> usize {
    editor
        .state
        .error_manager
        .notifications()
        .iter_active()
        .filter(|n| n.message.contains(needle))
        .count()
}

fn echo_lines(lines: &[&str]) -> String {
    let commands: Vec<String> = lines.iter().map(|line| format!("echo {line}")).collect();
    commands.join(if cfg!(windows) { "&" } else { "; " })
}

fn long_running() -> &'static str {
    if cfg!(windows) {
        "ping -n 60 127.0.0.1"
    } else {
        "sleep 60; echo done"
    }
}

fn write_plugin(dir: &std::path::Path, file: &str, src: &str) -> std::path::PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, src).unwrap();
    path
}

#[test]
fn stdin_echo_child() {
    use std::io::{BufRead, Write};
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let mut stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        writeln!(stdout, "echo:{}", line.unwrap()).unwrap();
        stdout.flush().unwrap();
    }
}

#[test]
fn output_arrives_as_ordered_lines_then_exit_zero() {
    let mut editor = lua_editor();
    run_lua(
        &mut editor,
        &format!(
            "_G.out = {{}} \
             rift.job.shell({:?}, {{ \
               on_stdout = function(lines) for _, l in ipairs(lines) do table.insert(_G.out, l) end end, \
               on_exit = function(code) _G.code = code _G.seen_at_exit = #_G.out end }})",
            echo_lines(&["one", "two", "three"])
        ),
    );
    tick_until(&mut editor, "_G.code ~= nil");
    assert!(lua_holds(
        &editor,
        "_G.code == 0 and _G.seen_at_exit == 3 and table.concat(_G.out, ',') == 'one,two,three'"
    ));
    assert!(editor.plugin_jobs.is_empty());
}

#[test]
fn non_zero_exit_reports_its_code() {
    let mut editor = lua_editor();
    run_lua(
        &mut editor,
        "rift.job.shell('exit 3', { on_exit = function(code) _G.code = code end })",
    );
    tick_until(&mut editor, "_G.code ~= nil");
    assert!(lua_holds(&editor, "_G.code == 3"));
}

#[test]
fn run_loop_delivers_output_without_other_plugin_calls() {
    let mut editor = lua_editor();
    run_lua(
        &mut editor,
        &format!(
            "rift.job.shell({:?}, {{ on_stdout = function(lines) rift.notify('info', 'job said ' .. lines[1]) end }})",
            echo_lines(&["ping"])
        ),
    );
    let deadline = Instant::now() + DEADLINE;
    while notification_count(&editor, "job said ping") == 0 {
        assert!(
            Instant::now() < deadline,
            "job output never reached the plugin"
        );
        editor.tick().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn send_reaches_stdin_and_close_stdin_ends_the_child() {
    let mut editor = lua_editor();
    let exe = std::env::current_exe().unwrap();
    let test_name = format!(
        "{}::stdin_echo_child",
        module_path!().split_once("::").unwrap().1
    );
    run_lua(
        &mut editor,
        &format!(
            "_G.out = {{}} \
             _G.id = rift.job.start({{ cmd = {exe:?}, args = {{ {test_name:?}, '--exact', '--nocapture', '-q' }}, \
               env = {{ {CHILD_ENV} = '1' }}, \
               on_stdout = function(lines) for _, l in ipairs(lines) do \
                 if l:sub(1, 5) == 'echo:' then table.insert(_G.out, l) end end end, \
               on_exit = function(code) _G.code = code end }}) \
             assert(rift.job.send(_G.id, 'hello\\n'))"
        ),
    );
    tick_until(&mut editor, "#_G.out == 1");
    assert!(lua_holds(
        &editor,
        "_G.out[1] == 'echo:hello' and _G.code == nil"
    ));

    run_lua(&mut editor, "assert(rift.job.close_stdin(_G.id))");
    tick_until(&mut editor, "_G.code ~= nil");
    assert!(lua_holds(&editor, "_G.code == 0"));
}

#[test]
fn stop_kills_a_long_running_child_tree() {
    let mut editor = lua_editor();
    run_lua(
        &mut editor,
        &format!(
            "_G.id = rift.job.shell({:?}, {{ on_exit = function(code) _G.code = code end }})",
            long_running()
        ),
    );
    let probe = only_probe(&editor);
    run_lua(&mut editor, "assert(rift.job.stop(_G.id))");
    tick_until(&mut editor, "_G.code ~= nil");
    assert!(lua_holds(&editor, "_G.code == 137"));
    assert!(probe.has_exited());
}

#[test]
fn unload_stops_the_plugins_jobs_without_calling_back() {
    let plugins = tempfile::tempdir().unwrap();
    let path = write_plugin(
        plugins.path(),
        "watcher.lua",
        &format!(
            "rift.job.shell({:?}, {{ on_exit = function() _G.called = true end }})",
            long_running()
        ),
    );
    let mut editor = lua_editor();
    assert!(editor.plugin_host.lua_load_file(&path).is_none());
    editor.apply_plugin_mutations();
    let probe = only_probe(&editor);

    let name = path.to_string_lossy().replace('\\', "/");
    run_lua(&mut editor, &format!("rift.plugins.unload({name:?})"));
    wait_for_exit(&probe);
    let deadline = Instant::now() + DEADLINE;
    while !editor.plugin_jobs.is_empty() {
        assert!(Instant::now() < deadline, "exit was never drained");
        editor.tick().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(lua_holds(&editor, "_G.called == nil"));
}

#[test]
fn spawn_failure_reports_the_plugin_and_exits() {
    let plugins = tempfile::tempdir().unwrap();
    let path = write_plugin(
        plugins.path(),
        "missing.lua",
        "rift.job.start({ cmd = 'rift-no-such-program-for-jobs', on_exit = function(code) _G.code = code end })",
    );
    let mut editor = lua_editor();
    assert!(editor.plugin_host.lua_load_file(&path).is_none());
    editor.apply_plugin_mutations();

    assert!(lua_holds(&editor, "_G.code == -1"));
    assert_eq!(
        notification_count(
            &editor,
            "[missing:job] failed to start 'rift-no-such-program-for-jobs'"
        ),
        1
    );
    assert!(editor.plugin_jobs.is_empty());
}

#[test]
fn reload_kills_jobs_of_the_retired_generation() {
    let plugins = tempfile::tempdir().unwrap();
    write_plugin(
        plugins.path(),
        "watcher.lua",
        &format!("rift.job.shell({:?})", long_running()),
    );
    let init = plugins.path().join("missing_init.lua");
    let mut editor = create_editor();
    assert!(editor
        .load_plugins_from(&[plugins.path().to_path_buf()], &init)
        .is_empty());
    let probe = only_probe(&editor);

    write_plugin(plugins.path(), "watcher.lua", "");
    assert!(editor
        .load_plugins_from(&[plugins.path().to_path_buf()], &init)
        .is_empty());

    assert!(editor.plugin_jobs.is_empty());
    wait_for_exit(&probe);
}

#[test]
fn output_from_a_retired_generation_is_dropped() {
    let plugins = tempfile::tempdir().unwrap();
    write_plugin(
        plugins.path(),
        "chatty.lua",
        &format!(
            "rift.job.shell({:?}, {{ \
               on_stdout = function() rift.notify('info', 'stale output') end, \
               on_exit = function() rift.notify('info', 'stale exit') end }})",
            echo_lines(&["late"])
        ),
    );
    let init = plugins.path().join("missing_init.lua");
    let mut editor = create_editor();
    assert!(editor
        .load_plugins_from(&[plugins.path().to_path_buf()], &init)
        .is_empty());
    let deadline = Instant::now() + DEADLINE;
    while editor.job_manager.any_job_thread_alive() {
        assert!(Instant::now() < deadline, "job never finished");
        std::thread::sleep(Duration::from_millis(5));
    }

    write_plugin(plugins.path(), "chatty.lua", "");
    assert!(editor
        .load_plugins_from(&[plugins.path().to_path_buf()], &init)
        .is_empty());
    for _ in 0..5 {
        editor.tick().unwrap();
    }

    assert_eq!(notification_count(&editor, "stale"), 0);
}

#[test]
fn dropping_the_editor_kills_running_jobs() {
    let mut editor = lua_editor();
    run_lua(
        &mut editor,
        &format!("rift.job.shell({:?})", long_running()),
    );
    let probe = only_probe(&editor);

    drop(editor);

    wait_for_exit(&probe);
}

#[test]
fn job_callback_reads_state_changed_after_the_start() {
    let mut editor = lua_editor();
    load_text(&mut editor, "one\ntwo\n");
    run_lua(
        &mut editor,
        "rift.job.shell('exit 0', { on_exit = function() _G.seen = rift.get_cursor() end })",
    );
    editor.active_document().buffer_mut().set_cursor(4).unwrap();

    tick_until(&mut editor, "_G.seen ~= nil");
    assert!(lua_holds(&editor, "_G.seen == 2"));
}
