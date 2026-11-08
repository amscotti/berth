//! A short ring of samples, so the overview can show what has been happening
//! rather than only what is happening now.
//!
//! Fixed capacity and a shift on overflow: 120 floats is 480 bytes, and moving
//! them once every few seconds is nothing next to ring-index arithmetic on
//! every read.

/// How many five-second passes the graphs remember: ten minutes.
pub const CAPACITY: usize = 120;

/// Which series a graph wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Cpu,
    Mem,
    Running,
    Disk,
    HostCpu,
    HostMem,
}

/// One point in the ring.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub cpu: f32,
    /// False when the engine did not measure CPU this pass. The graph holds
    /// the last known value instead of drawing a fake zero.
    pub cpu_known: bool,
    pub mem: f32,
    /// False when any up container had no stats. The memory graph holds the
    /// last complete sum instead of drawing a partial one as if it were total.
    pub mem_known: bool,
    pub running: f32,
    pub disk: f32,
    pub host_cpu: f32,
    pub host_mem: f32,
}

/// A fixed ring of samples, oldest first.
#[derive(Clone, Debug)]
pub struct History {
    cpu: [f32; CAPACITY],
    cpu_known: [bool; CAPACITY],
    mem: [f32; CAPACITY],
    mem_known: [bool; CAPACITY],
    running: [f32; CAPACITY],
    disk: [f32; CAPACITY],
    host_cpu: [f32; CAPACITY],
    host_mem: [f32; CAPACITY],
    count: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            cpu: [0.0; CAPACITY],
            cpu_known: [true; CAPACITY],
            mem: [0.0; CAPACITY],
            mem_known: [true; CAPACITY],
            running: [0.0; CAPACITY],
            disk: [0.0; CAPACITY],
            host_cpu: [0.0; CAPACITY],
            host_mem: [0.0; CAPACITY],
            count: 0,
        }
    }
}

impl History {
    pub fn push(&mut self, sample: Sample) {
        if self.count == CAPACITY {
            Self::shift(&mut self.cpu);
            Self::shift(&mut self.cpu_known);
            Self::shift(&mut self.mem);
            Self::shift(&mut self.mem_known);
            Self::shift(&mut self.running);
            Self::shift(&mut self.disk);
            Self::shift(&mut self.host_cpu);
            Self::shift(&mut self.host_mem);
            self.count = CAPACITY - 1;
        }

        let previous_cpu = if self.count > 0 {
            self.cpu[self.count - 1]
        } else {
            0.0
        };
        let previous_mem = if self.count > 0 {
            self.mem[self.count - 1]
        } else {
            0.0
        };
        self.cpu[self.count] = if sample.cpu_known {
            sample.cpu
        } else {
            previous_cpu
        };
        self.cpu_known[self.count] = sample.cpu_known;
        self.mem[self.count] = if sample.mem_known {
            sample.mem
        } else {
            previous_mem
        };
        self.mem_known[self.count] = sample.mem_known;
        self.running[self.count] = sample.running;
        self.disk[self.count] = sample.disk;
        self.host_cpu[self.count] = sample.host_cpu;
        self.host_mem[self.count] = sample.host_mem;
        self.count += 1;
    }

    fn shift<T: Copy>(values: &mut [T; CAPACITY]) {
        values.copy_within(1.., 0);
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The visible slice for one series. This borrows; it does not allocate.
    pub fn values(&self, which: Which) -> &[f32] {
        match which {
            Which::Cpu => &self.cpu[..self.count],
            Which::Mem => &self.mem[..self.count],
            Which::Running => &self.running[..self.count],
            Which::Disk => &self.disk[..self.count],
            Which::HostCpu => &self.host_cpu[..self.count],
            Which::HostMem => &self.host_mem[..self.count],
        }
    }

    /// Whether every visible CPU point is a real measurement.
    pub fn cpu_all_known(&self) -> bool {
        self.cpu_known[..self.count].iter().all(|known| *known)
    }

    /// Whether every visible memory point is a complete sum.
    pub fn mem_all_known(&self) -> bool {
        self.mem_known[..self.count].iter().all(|known| *known)
    }

    pub fn latest_cpu(&self) -> Option<f32> {
        (self.count > 0).then(|| self.cpu[self.count - 1])
    }

    pub fn peak(&self, which: Which) -> f32 {
        self.values(which).iter().copied().fold(0.0_f32, f32::max)
    }

    /// A rounded 1/2/5 × 10^n ceiling for a graph. A fixed ceiling keeps a flat
    /// series at one height while its oldest samples leave the ring.
    pub fn ceiling(peak: f32) -> f32 {
        if !peak.is_finite() || peak <= 0.0 {
            return 1.0;
        }
        let magnitude = 10.0_f32.powf(peak.log10().floor());
        let n = peak / magnitude;
        let step = if n <= 1.0 {
            1.0
        } else if n <= 2.0 {
            2.0
        } else if n <= 5.0 {
            5.0
        } else {
            10.0
        };
        (step * magnitude).max(1.0)
    }

    /// Drop engine points when the engine changes; host points are still valid.
    pub fn clear_engine(&mut self) {
        self.cpu[..self.count].fill(0.0);
        self.mem[..self.count].fill(0.0);
        self.running[..self.count].fill(0.0);
        self.disk[..self.count].fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: f32, mem: f32) -> Sample {
        Sample {
            cpu,
            cpu_known: true,
            mem,
            mem_known: true,
            running: 1.0,
            disk: 0.0,
            host_cpu: 0.0,
            host_mem: 0.0,
        }
    }

    #[test]
    fn push_appends_and_reports_length() {
        let mut history = History::default();
        assert!(history.is_empty());
        history.push(sample(1.0, 2.0));
        assert_eq!(history.len(), 1);
        assert_eq!(history.values(Which::Cpu), &[1.0]);
        assert_eq!(history.values(Which::Mem), &[2.0]);
    }

    #[test]
    fn unknown_cpu_holds_the_previous_height() {
        let mut history = History::default();
        history.push(sample(10.0, 20.0));
        history.push(Sample {
            cpu_known: false,
            ..sample(0.0, 20.0)
        });
        assert_eq!(history.values(Which::Cpu), &[10.0, 10.0]);
        assert!(!history.cpu_all_known());
    }

    #[test]
    fn unknown_memory_holds_the_previous_sum() {
        let mut history = History::default();
        history.push(sample(1.0, 100.0));
        history.push(Sample {
            mem_known: false,
            ..sample(1.0, 0.0)
        });
        assert_eq!(history.values(Which::Mem), &[100.0, 100.0]);
        assert!(!history.mem_all_known());
    }

    #[test]
    fn the_ring_keeps_the_newest_capacity() {
        let mut history = History::default();
        for i in 0..(CAPACITY + 10) {
            history.push(sample(i as f32, i as f32));
        }
        assert_eq!(history.len(), CAPACITY);
        assert_eq!(history.values(Which::Cpu)[0], 10.0);
        assert_eq!(
            *history.values(Which::Cpu).last().unwrap(),
            (CAPACITY + 9) as f32
        );
    }

    #[test]
    fn ceilings_are_1_2_5() {
        assert_eq!(History::ceiling(0.0), 1.0);
        assert_eq!(History::ceiling(1.0), 1.0);
        assert_eq!(History::ceiling(1.5), 2.0);
        assert_eq!(History::ceiling(3.0), 5.0);
        assert_eq!(History::ceiling(7.0), 10.0);
        assert_eq!(History::ceiling(120.0), 200.0);
    }

    #[test]
    fn peak_and_latest_are_independent_of_known_flags() {
        let mut history = History::default();
        history.push(sample(4.0, 8.0));
        history.push(sample(2.0, 3.0));
        assert_eq!(history.peak(Which::Cpu), 4.0);
        assert_eq!(history.latest_cpu(), Some(2.0));
    }
}
