use super::Editor;
use crate::job_manager::jobs::plugin_process::{
    self, JobSpec, PluginJobEvent, PluginJobHandle, PluginJobPayload,
};
use crate::plugin::{PluginGeneration, PluginMutation};
use crate::term::TerminalBackend;

pub(super) struct PluginJob {
    pub(super) handle: PluginJobHandle,
    owner: Option<PluginGeneration>,
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn apply_job_mutation(
        &mut self,
        mutation: PluginMutation,
        origin: Option<PluginGeneration>,
    ) {
        match mutation {
            PluginMutation::StartJob { id, spec } => self.start_plugin_job(id, &spec, origin),
            PluginMutation::JobInput { id, data } => {
                if let Some(job) = self.plugin_jobs.get(&id) {
                    job.handle.send(data);
                }
            }
            PluginMutation::JobCloseStdin(id) => {
                if let Some(job) = self.plugin_jobs.get(&id) {
                    job.handle.close_stdin();
                }
            }
            PluginMutation::StopJob(id) => {
                if let Some(job) = self.plugin_jobs.get(&id) {
                    job.handle.stop();
                }
            }
            _ => self.report_misrouted(),
        }
    }

    fn start_plugin_job(&mut self, id: u64, spec: &JobSpec, owner: Option<PluginGeneration>) {
        match plugin_process::start(id, spec) {
            Ok((handle, job)) => {
                self.job_manager.spawn(job);
                self.plugin_jobs.insert(id, PluginJob { handle, owner });
            }
            Err(error) => {
                let event =
                    PluginJobEvent::Failed(format!("failed to start '{}': {error}", spec.command));
                self.call_plugins(|editor| editor.plugin_host.deliver_job_event(id, event));
            }
        }
    }

    pub(super) fn deliver_plugin_job_payload(&mut self, payload: PluginJobPayload) {
        let PluginJobPayload { job, event } = payload;
        let Some(entry) = self.plugin_jobs.get(&job) else {
            return;
        };
        let active = entry
            .owner
            .is_none_or(|owner| self.plugin_host.is_generation_active(owner));
        if event.is_final() {
            self.plugin_jobs.remove(&job);
        }
        if active {
            self.call_plugins(|editor| editor.plugin_host.deliver_job_event(job, event));
        }
    }

    pub(super) fn stop_generation_jobs(&mut self, generation: PluginGeneration) {
        self.plugin_jobs
            .retain(|_, job| job.owner != Some(generation));
    }
}
