use std::time::Instant;
use sysinfo::{Networks, System};

pub struct StatsSampler {
    system: System,
    networks: Networks,
    last_at: Instant,
    last_tx_bytes: u64,
    last_rx_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub cpu_usage: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub swap_used_mb: u64,
    pub swap_total_mb: u64,
    pub net_tx_kbps: f32,
    pub net_rx_kbps: f32,
}

impl StatsSampler {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);
        let last_tx_bytes = networks.values().map(|net| net.total_transmitted()).sum();
        let last_rx_bytes = networks.values().map(|net| net.total_received()).sum();
        Self {
            system,
            networks,
            last_at: Instant::now(),
            last_tx_bytes,
            last_rx_bytes,
        }
    }

    pub fn sample(&mut self) -> Sample {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        let now = Instant::now();
        let seconds = now.duration_since(self.last_at).as_secs_f32().max(0.001);
        let tx_bytes_now: u64 = self
            .networks
            .values()
            .map(|net| net.total_transmitted())
            .sum();
        let rx_bytes_now: u64 = self.networks.values().map(|net| net.total_received()).sum();
        let tx_delta = tx_bytes_now.saturating_sub(self.last_tx_bytes);
        let rx_delta = rx_bytes_now.saturating_sub(self.last_rx_bytes);
        self.last_at = now;
        self.last_tx_bytes = tx_bytes_now;
        self.last_rx_bytes = rx_bytes_now;
        Sample {
            cpu_usage: self.system.global_cpu_usage(),
            memory_used_mb: self.system.used_memory() / 1024 / 1024,
            memory_total_mb: self.system.total_memory() / 1024 / 1024,
            swap_used_mb: self.system.used_swap() / 1024 / 1024,
            swap_total_mb: self.system.total_swap() / 1024 / 1024,
            net_tx_kbps: (tx_delta as f32 * 8.0) / seconds / 1000.0,
            net_rx_kbps: (rx_delta as f32 * 8.0) / seconds / 1000.0,
        }
    }
}
