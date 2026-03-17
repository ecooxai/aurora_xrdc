use std::time::Instant;
use sysinfo::{Networks, System};

pub struct StatsSampler {
    system: System,
    networks: Networks,
    last_at: Instant,
    last_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub cpu_usage: f32,
    pub memory_used_mb: u64,
    pub net_tx_kbps: f32,
}

impl StatsSampler {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);
        let last_bytes = networks.values().map(|net| net.total_transmitted()).sum();
        Self {
            system,
            networks,
            last_at: Instant::now(),
            last_bytes,
        }
    }

    pub fn sample(&mut self) -> Sample {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        let now = Instant::now();
        let seconds = now.duration_since(self.last_at).as_secs_f32().max(0.001);
        let bytes_now: u64 = self
            .networks
            .values()
            .map(|net| net.total_transmitted())
            .sum();
        let delta = bytes_now.saturating_sub(self.last_bytes);
        self.last_at = now;
        self.last_bytes = bytes_now;
        Sample {
            cpu_usage: self.system.global_cpu_usage(),
            memory_used_mb: self.system.used_memory() / 1024 / 1024,
            net_tx_kbps: (delta as f32 * 8.0) / seconds / 1000.0,
        }
    }
}

