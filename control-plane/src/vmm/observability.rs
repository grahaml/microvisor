use std::io;
use std::path::Path;
use std::time::Instant;
use std::os::unix::fs::MetadataExt;
use tracing::{warn, Span};
use rand::Rng;
use tokio::task::JoinHandle;

pub fn generate_session_id() -> u64 {
    rand::rng().random()
}

pub struct SyscallAuditor;

impl SyscallAuditor {
    pub fn audit<F, R>(name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        
        if duration.as_millis() >= 1 {
            warn!(
                syscall = name,
                duration_ms = duration.as_millis(),
                "Blocking syscall exceeded 1ms threshold"
            );
        }
        
        result
    }

    /// Spawns a blocking task on the tokio thread pool and audits its execution time.
    /// If it exceeds 1ms, a warning is logged.
    pub async fn spawn_blocking<F, R>(name: &'static str, f: F) -> io::Result<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = Span::current();
        let handle: JoinHandle<R> = tokio::task::spawn_blocking(move || {
            let _enter = span.enter();
            Self::audit(name, f)
        });

        handle.await.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_generate_session_id() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_spawn_blocking_audit() {
        let result = SyscallAuditor::spawn_blocking("test_task", || {
            thread::sleep(Duration::from_millis(2));
            42
        }).await.unwrap();
        
        assert_eq!(result, 42);
    }
}

pub struct MetricsCollector {
    cgroup_id: u64,
}

impl MetricsCollector {
    pub fn from_cgroup_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            cgroup_id: metadata.ino(),
        })
    }

    pub fn get_kvm_exit_count(&self) -> io::Result<u64> {
        // In a real implementation, this would read from a BPF map
        // populated by a program attached to tracepoint:kvm:kvm_exit
        // which filters by current->cgroups->...->id == self.cgroup_id
        tracing::info!("Collecting KVM exits for cgroup ID {}", self.cgroup_id);
        Ok(0)
    }

    pub fn link_cgroup_to_session(cgroup_id: u64, session_id: u64) {
        tracing::info!(cgroup_id, session_id, "Linking cgroup to session in BPF map");
        // In a real implementation, this would update a BPF map:
        // bpf_map_update_elem(vm_session_map, &cgroup_id, &session_id, BPF_ANY);
    }
}
