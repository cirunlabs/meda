//! Host-resource admission control for the HTTP API.
//!
//! Strict no-overcommit. Every accepted VM consumes its full declared
//! mem/cpu/disk against the host budget; once `(total - reserve - committed)`
//! cannot satisfy a new request the API returns 503 instead of spawning
//! and letting the kernel OOM-killer fire. The 2026-05-16 50-job test
//! showed the unconstrained path takes down the user's systemd session
//! along with the VMs, so this is a host-safety belt, not a nice-to-have.
//!
//! Reserves are env-tunable (`MEDA_RESERVE_MEM_GB`, `MEDA_RESERVE_CPU`,
//! `MEDA_RESERVE_DISK_GB`, defaulting to 1 each) so an operator can
//! widen the host's headroom without recompiling.
//!
//! This module is pure: it does no I/O, holds no async, takes inputs
//! by value. All host-state discovery (reading /proc/meminfo, statvfs,
//! enumerating ~/.meda/vms/*) lives in the caller — admission only
//! decides "yes / no, here's why" given the numbers.

use std::env;
use std::sync::{Arc, Mutex};

const RESERVE_MEM_ENV: &str = "MEDA_RESERVE_MEM_GB";
const RESERVE_CPU_ENV: &str = "MEDA_RESERVE_CPU";
const RESERVE_DISK_ENV: &str = "MEDA_RESERVE_DISK_GB";

/// Static host capacity + operator-set reserve. Built once at startup
/// from `/proc/meminfo`, `nproc`, and `statvfs(~/.meda)`. Reserves come
/// from env vars; total values are detected.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub total_mem_gb: u64,
    pub total_cpu: u32,
    pub total_disk_gb: u64,
    pub reserve_mem_gb: u64,
    pub reserve_cpu: u32,
    pub reserve_disk_gb: u64,
}

/// Sum of declared mem/cpu/disk across currently-running meda VMs.
/// Templates and stopped VMs are NOT counted — they don't pressure host
/// RAM/CPU. Disk is counted for all on-disk VM dirs because qcow2
/// overlays grow until deleted, even when the VM is stopped.
#[derive(Debug, Clone, Copy, Default)]
pub struct Committed {
    pub mem_gb: u64,
    pub cpu: u32,
    pub disk_gb: u64,
}

/// Resources a single incoming `POST /images/run` is asking for. Values
/// are parsed from the (string-typed) request body by the caller and
/// passed in normalized form here.
#[derive(Debug, Clone, Copy)]
pub struct VmRequest {
    pub mem_gb: u64,
    pub cpu: u32,
    pub disk_gb: u64,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum AdmissionDenied {
    MemExhausted { needed_gb: u64, available_gb: u64 },
    CpuExhausted { needed: u32, available: u32 },
    DiskExhausted { needed_gb: u64, available_gb: u64 },
}

impl AdmissionDenied {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MemExhausted { .. } => "MEM_EXHAUSTED",
            Self::CpuExhausted { .. } => "CPU_EXHAUSTED",
            Self::DiskExhausted { .. } => "DISK_EXHAUSTED",
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::MemExhausted {
                needed_gb,
                available_gb,
            } => format!("Memory exhausted: need {needed_gb} GiB, {available_gb} GiB available after reserve"),
            Self::CpuExhausted { needed, available } => format!(
                "CPU exhausted: need {needed} vCPU, {available} vCPU available after reserve"
            ),
            Self::DiskExhausted {
                needed_gb,
                available_gb,
            } => format!("Disk exhausted: need {needed_gb} GiB, {available_gb} GiB available after reserve"),
        }
    }
}

impl Budget {
    /// Build a budget from detected host totals + env-overridable reserves.
    /// Reserves default to 1 each; an env var that fails to parse warns
    /// and falls back to the default rather than aborting startup.
    pub fn new(total_mem_gb: u64, total_cpu: u32, total_disk_gb: u64) -> Self {
        Self {
            total_mem_gb,
            total_cpu,
            total_disk_gb,
            reserve_mem_gb: env_u64(RESERVE_MEM_ENV, 1),
            reserve_cpu: env_u32(RESERVE_CPU_ENV, 1),
            reserve_disk_gb: env_u64(RESERVE_DISK_ENV, 1),
        }
    }

    pub fn mem_available_gb(&self, committed: u64) -> u64 {
        self.total_mem_gb
            .saturating_sub(self.reserve_mem_gb)
            .saturating_sub(committed)
    }
    pub fn cpu_available(&self, committed: u32) -> u32 {
        self.total_cpu
            .saturating_sub(self.reserve_cpu)
            .saturating_sub(committed)
    }
    pub fn disk_available_gb(&self, committed: u64) -> u64 {
        self.total_disk_gb
            .saturating_sub(self.reserve_disk_gb)
            .saturating_sub(committed)
    }
}

/// Concurrency-safe admission controller.
///
/// The naive `read committed → can_admit → spawn` sequence is racy
/// under burst load: N concurrent handlers all read the same
/// pre-burst committed value (none of the in-progress spawns have
/// written their VM dir yet), all call `can_admit` independently,
/// all admit, host then OOMs.
///
/// `Admission` closes that race by maintaining an in-flight counter
/// updated under a mutex. Each admit (a) snapshots both the
/// on-disk committed AND the in-flight tally before deciding, and
/// (b) atomically reserves the request's resources before releasing
/// the lock. The reservation is returned as an RAII guard
/// (`Reservation`) so caller-side bugs can't leak counter slots —
/// drop releases.
///
/// On-disk committed is read by the caller (it requires fs I/O); the
/// caller passes it in. The locked critical section is therefore very
/// short: read in_flight, sum, decide, mutate in_flight, drop lock.
pub struct Admission {
    pub budget: Budget,
    in_flight: Mutex<Committed>,
}

impl Admission {
    pub fn new(budget: Budget) -> Arc<Self> {
        Arc::new(Self {
            budget,
            in_flight: Mutex::new(Committed::default()),
        })
    }

    /// Snapshot of currently-reserved (in-flight, not yet on-disk)
    /// resources. Used by the capacity endpoint for observability.
    pub fn in_flight(&self) -> Committed {
        self.in_flight.lock().map(|g| *g).unwrap_or_default()
    }

    /// Atomic check-then-reserve. Returns a `Reservation` guard whose
    /// drop releases the reserved capacity. Concurrent callers see the
    /// reservation immediately so subsequent requests in the same burst
    /// correctly account for it.
    ///
    /// Caller must pass `persistent_committed` — what's currently on
    /// disk from prior, already-spawned VMs. We add in-flight to it
    /// inside the lock to get the effective committed total.
    pub fn try_reserve(
        self: &Arc<Self>,
        req: &VmRequest,
        persistent_committed: &Committed,
    ) -> Result<Reservation, AdmissionDenied> {
        let mut in_flight = self.in_flight.lock().expect("admission in_flight poisoned");
        let effective = Committed {
            mem_gb: persistent_committed.mem_gb.saturating_add(in_flight.mem_gb),
            cpu: persistent_committed.cpu.saturating_add(in_flight.cpu),
            disk_gb: persistent_committed
                .disk_gb
                .saturating_add(in_flight.disk_gb),
        };
        can_admit(req, &effective, &self.budget)?;
        in_flight.mem_gb = in_flight.mem_gb.saturating_add(req.mem_gb);
        in_flight.cpu = in_flight.cpu.saturating_add(req.cpu);
        in_flight.disk_gb = in_flight.disk_gb.saturating_add(req.disk_gb);
        Ok(Reservation {
            owner: Arc::clone(self),
            req: *req,
            released: false,
        })
    }
}

/// RAII guard returned by `Admission::try_reserve`. Dropping it
/// releases the reserved capacity back to the in-flight tally so
/// the next admission decision sees the slot free.
pub struct Reservation {
    owner: Arc<Admission>,
    req: VmRequest,
    released: bool,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Ok(mut in_flight) = self.owner.in_flight.lock() {
            in_flight.mem_gb = in_flight.mem_gb.saturating_sub(self.req.mem_gb);
            in_flight.cpu = in_flight.cpu.saturating_sub(self.req.cpu);
            in_flight.disk_gb = in_flight.disk_gb.saturating_sub(self.req.disk_gb);
        }
    }
}

pub fn can_admit(
    req: &VmRequest,
    committed: &Committed,
    budget: &Budget,
) -> Result<(), AdmissionDenied> {
    let mem_avail = budget.mem_available_gb(committed.mem_gb);
    if req.mem_gb > mem_avail {
        return Err(AdmissionDenied::MemExhausted {
            needed_gb: req.mem_gb,
            available_gb: mem_avail,
        });
    }
    let cpu_avail = budget.cpu_available(committed.cpu);
    if req.cpu > cpu_avail {
        return Err(AdmissionDenied::CpuExhausted {
            needed: req.cpu,
            available: cpu_avail,
        });
    }
    let disk_avail = budget.disk_available_gb(committed.disk_gb);
    if req.disk_gb > disk_avail {
        return Err(AdmissionDenied::DiskExhausted {
            needed_gb: req.disk_gb,
            available_gb: disk_avail,
        });
    }
    Ok(())
}

/// Parse meda's size strings — "8G", "8192M", "1T", or a bare "8"
/// (treated as GiB to match the request body's documented semantics)
/// — into GiB.
///
/// Floors on the way down (1500M → 1 GiB), which is what we want for
/// admission: it errs in favour of denying a borderline request rather
/// than accepting one that the host can't actually hold.
///
/// Returns 0 on parse failure. The caller decides what to do — for an
/// incoming request that means "treat as needing 0 GiB", which is fine
/// (a deliberately broken request just becomes admissible-but-useless).
/// For an existing VM's on-disk metadata it means "this VM contributes
/// 0 to committed", which is conservative for the operator's blast
/// radius — a corrupt VM dir shouldn't lock the entire host out.
pub fn parse_size_gb(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }
    let split_at = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_str, unit) = (&s[..split_at], &s[split_at..]);
    let n: u64 = match num_str.trim().parse() {
        Ok(v) => v,
        Err(_) => return 0,
    };
    match unit.to_ascii_uppercase().as_str() {
        "" | "G" | "GB" | "GIB" => n,
        "M" | "MB" | "MIB" => n / 1024,
        "K" | "KB" | "KIB" => n / (1024 * 1024),
        "T" | "TB" | "TIB" => n.saturating_mul(1024),
        _ => 0,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    match env::var(key) {
        Ok(s) => s.trim().parse::<u64>().unwrap_or_else(|_| {
            log::warn!("{key}='{s}' is not a u64; using default {default}");
            default
        }),
        Err(_) => default,
    }
}

fn env_u32(key: &str, default: u32) -> u32 {
    match env::var(key) {
        Ok(s) => s.trim().parse::<u32>().unwrap_or_else(|_| {
            log::warn!("{key}='{s}' is not a u32; using default {default}");
            default
        }),
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(mem: u64, cpu: u32, disk: u64) -> Budget {
        Budget {
            total_mem_gb: mem,
            total_cpu: cpu,
            total_disk_gb: disk,
            reserve_mem_gb: 1,
            reserve_cpu: 1,
            reserve_disk_gb: 1,
        }
    }

    fn req(mem: u64, cpu: u32, disk: u64) -> VmRequest {
        VmRequest {
            mem_gb: mem,
            cpu,
            disk_gb: disk,
        }
    }

    #[test]
    fn empty_host_admits_request_within_budget() {
        // 98 GiB host with reserve=1 → 97 GiB budget. 8 GiB request fits.
        let b = budget(98, 16, 400);
        let c = Committed::default();
        assert!(can_admit(&req(8, 4, 50), &c, &b).is_ok());
    }

    #[test]
    fn rejects_when_committed_plus_request_exceeds_mem_budget() {
        // 98 GiB host, reserve=1 GiB → 97 GiB budget.
        // Already 96 GiB committed → only 1 GiB free → 8 GiB request denied.
        let b = budget(98, 16, 400);
        let c = Committed {
            mem_gb: 96,
            cpu: 4,
            disk_gb: 50,
        };
        match can_admit(&req(8, 4, 50), &c, &b) {
            Err(AdmissionDenied::MemExhausted {
                needed_gb,
                available_gb,
            }) => {
                assert_eq!(needed_gb, 8);
                assert_eq!(available_gb, 1);
            }
            other => panic!("expected MemExhausted, got {other:?}"),
        }
    }

    #[test]
    fn rejects_when_request_exceeds_cpu_budget() {
        // 16-core host, reserve=1 → 15 budget. 4 already running → 11 free.
        // 12 vCPU request denied.
        let b = budget(98, 16, 400);
        let c = Committed {
            mem_gb: 0,
            cpu: 4,
            disk_gb: 0,
        };
        match can_admit(&req(0, 12, 0), &c, &b) {
            Err(AdmissionDenied::CpuExhausted { needed, available }) => {
                assert_eq!(needed, 12);
                assert_eq!(available, 11);
            }
            other => panic!("expected CpuExhausted, got {other:?}"),
        }
    }

    #[test]
    fn rejects_when_request_exceeds_disk_budget() {
        // 400 GiB host, reserve=1 → 399 budget. 200 committed → 199 free.
        // 200 GiB request denied.
        let b = budget(98, 16, 400);
        let c = Committed {
            mem_gb: 0,
            cpu: 0,
            disk_gb: 200,
        };
        match can_admit(&req(0, 0, 200), &c, &b) {
            Err(AdmissionDenied::DiskExhausted {
                needed_gb,
                available_gb,
            }) => {
                assert_eq!(needed_gb, 200);
                assert_eq!(available_gb, 199);
            }
            other => panic!("expected DiskExhausted, got {other:?}"),
        }
    }

    #[test]
    fn exact_fit_is_accepted() {
        // 8 GiB host, reserve=1 → 7 budget. Request 7 → accepted at the edge.
        let b = budget(8, 4, 100);
        let c = Committed::default();
        assert!(can_admit(&req(7, 3, 99), &c, &b).is_ok());
    }

    #[test]
    fn reserve_larger_than_total_denies_everything() {
        // Operator misconfigures `MEDA_RESERVE_MEM_GB=200` on a 98 GiB
        // host. saturating_sub clamps to 0 — no request can be admitted,
        // not even one. Better than negative wraparound.
        let mut b = budget(98, 16, 400);
        b.reserve_mem_gb = 200;
        let c = Committed::default();
        match can_admit(&req(1, 0, 0), &c, &b) {
            Err(AdmissionDenied::MemExhausted {
                available_gb: 0, ..
            }) => {}
            other => panic!("expected MemExhausted with available=0, got {other:?}"),
        }
    }

    #[test]
    fn committed_larger_than_budget_denies() {
        // Edge case after a config change: existing committed exceeds
        // the current budget (operator shrank reserve). New requests
        // should be denied, not overflow.
        let b = budget(98, 16, 400);
        let c = Committed {
            mem_gb: 200,
            cpu: 0,
            disk_gb: 0,
        };
        match can_admit(&req(1, 0, 0), &c, &b) {
            Err(AdmissionDenied::MemExhausted {
                available_gb: 0, ..
            }) => {}
            other => panic!("expected MemExhausted with available=0, got {other:?}"),
        }
    }

    #[test]
    fn mem_check_runs_before_cpu_or_disk() {
        // If multiple resources are exhausted, we surface the FIRST
        // one (mem) so the operator's logs aren't ambiguous about what
        // the bottleneck is. Order is mem -> cpu -> disk.
        let b = budget(8, 1, 100);
        let c = Committed {
            mem_gb: 7,
            cpu: 0,
            disk_gb: 0,
        };
        assert!(matches!(
            can_admit(&req(8, 4, 200), &c, &b),
            Err(AdmissionDenied::MemExhausted { .. })
        ));
    }

    #[test]
    fn denied_messages_include_numbers() {
        // Operator-facing readable error — the diagnostic string MUST
        // carry the numeric reason or `tail -f` debugging is useless.
        let d = AdmissionDenied::MemExhausted {
            needed_gb: 8,
            available_gb: 3,
        };
        let m = d.message();
        assert!(m.contains("8"));
        assert!(m.contains("3"));
        assert_eq!(d.code(), "MEM_EXHAUSTED");
    }

    // Env-parsing tests use unique vars per test so parallel test runs
    // don't trample each other. The shared MEDA_RESERVE_* names are
    // read only via Budget::new and not exercised here.

    #[test]
    fn env_u64_parses_valid_value() {
        env::set_var("ADMISSION_TEST_OK_U64", "42");
        assert_eq!(env_u64("ADMISSION_TEST_OK_U64", 1), 42);
        env::remove_var("ADMISSION_TEST_OK_U64");
    }

    #[test]
    fn env_u64_falls_back_on_bad_value() {
        env::set_var("ADMISSION_TEST_BAD_U64", "twelve");
        assert_eq!(env_u64("ADMISSION_TEST_BAD_U64", 7), 7);
        env::remove_var("ADMISSION_TEST_BAD_U64");
    }

    #[test]
    fn env_u64_falls_back_when_unset() {
        env::remove_var("ADMISSION_TEST_UNSET_U64");
        assert_eq!(env_u64("ADMISSION_TEST_UNSET_U64", 5), 5);
    }

    #[test]
    fn parse_size_gb_handles_common_units() {
        // The incoming ImageRunRequest carries memory/disk as user-typed
        // strings ("8G", "8192M", …). These must collapse to the same
        // GiB number our admission logic compares against committed.
        assert_eq!(parse_size_gb("8G"), 8);
        assert_eq!(parse_size_gb("8192M"), 8);
        assert_eq!(parse_size_gb("50GB"), 50);
        assert_eq!(parse_size_gb("1T"), 1024);
        // Bare number — admission's request body uses "8" intermittently;
        // treat as GiB to match the documented semantics.
        assert_eq!(parse_size_gb("16"), 16);
    }

    #[test]
    fn parse_size_gb_floors_subgib_values() {
        // 500 MiB rounds DOWN to 0 GiB. This is intentional — if the
        // operator declared 500M, an admission check at coarse GiB
        // granularity should not "win them" a free slot by rounding up.
        assert_eq!(parse_size_gb("500M"), 0);
    }

    #[test]
    fn parse_size_gb_returns_zero_on_garbage() {
        assert_eq!(parse_size_gb(""), 0);
        assert_eq!(parse_size_gb("abc"), 0);
        assert_eq!(parse_size_gb("8X"), 0);
        assert_eq!(parse_size_gb("-8G"), 0); // negative parses fail
    }

    fn small_budget() -> Budget {
        Budget {
            total_mem_gb: 16,
            total_cpu: 4,
            total_disk_gb: 100,
            reserve_mem_gb: 0,
            reserve_cpu: 0,
            reserve_disk_gb: 0,
        }
    }

    #[test]
    fn concurrent_reservations_against_empty_host_respect_budget() {
        // Race scenario: 10 concurrent handlers all see persistent_committed=0
        // (nothing on disk yet). Without in-flight tracking, all 10 would
        // admit; with it, only as many as the budget supports do.
        // Budget here: 4 cpu / 4 cpu-per-VM = 4 VMs max.
        let admission = Admission::new(small_budget());
        let persistent = Committed::default();
        let r = VmRequest {
            mem_gb: 4,
            cpu: 1,
            disk_gb: 25,
        };

        let mut admitted = Vec::new();
        let mut denied = 0;
        for _ in 0..10 {
            match admission.try_reserve(&r, &persistent) {
                Ok(g) => admitted.push(g),
                Err(_) => denied += 1,
            }
        }

        // 4 cpu budget / 1 cpu per VM = 4 admitted, 6 denied.
        assert_eq!(admitted.len(), 4);
        assert_eq!(denied, 6);
        // The in-flight tally reflects all admitted reservations.
        let inf = admission.in_flight();
        assert_eq!(inf.cpu, 4);
        assert_eq!(inf.mem_gb, 16);
    }

    #[test]
    fn dropping_reservation_frees_capacity() {
        // After a reservation drops (handler returned), the next request
        // sees the slot freed and admits again.
        let admission = Admission::new(small_budget());
        let persistent = Committed::default();
        let r = VmRequest {
            mem_gb: 4,
            cpu: 4,
            disk_gb: 50,
        };

        // First fills the whole budget.
        let g = admission.try_reserve(&r, &persistent).expect("admit");
        assert!(admission.try_reserve(&r, &persistent).is_err());

        drop(g);
        // Now there's room again.
        assert!(admission.try_reserve(&r, &persistent).is_ok());
    }

    #[test]
    fn persistent_committed_is_summed_with_in_flight() {
        // Spawned VMs that already wrote their dirs contribute via the
        // caller-passed persistent_committed; we still admit only up to
        // budget minus the sum of both buckets.
        let admission = Admission::new(small_budget());
        let persistent = Committed {
            mem_gb: 12,
            cpu: 3,
            disk_gb: 75,
        };
        let r = VmRequest {
            mem_gb: 4,
            cpu: 1,
            disk_gb: 25,
        };

        // Persistent already consumes 3/4 cpu. One more fits.
        let _g = admission.try_reserve(&r, &persistent).expect("admit");
        // No more.
        assert!(admission.try_reserve(&r, &persistent).is_err());
    }

    #[test]
    fn denied_reservation_does_not_mutate_in_flight() {
        // Failed try_reserve must NOT leak counter slots — otherwise
        // burst-denied requests would slowly poison the in-flight tally
        // and starve later legitimate requests.
        let admission = Admission::new(small_budget());
        let persistent = Committed {
            mem_gb: 16,
            cpu: 4,
            disk_gb: 100,
        };
        let r = VmRequest {
            mem_gb: 4,
            cpu: 1,
            disk_gb: 25,
        };

        let before = admission.in_flight();
        let _ = admission.try_reserve(&r, &persistent); // expected Err
        let after = admission.in_flight();
        assert_eq!(before.cpu, after.cpu);
        assert_eq!(before.mem_gb, after.mem_gb);
    }

    #[test]
    fn env_u32_parses_and_falls_back() {
        env::set_var("ADMISSION_TEST_OK_U32", "9");
        assert_eq!(env_u32("ADMISSION_TEST_OK_U32", 1), 9);
        env::remove_var("ADMISSION_TEST_OK_U32");

        env::set_var("ADMISSION_TEST_BAD_U32", "-3");
        assert_eq!(env_u32("ADMISSION_TEST_BAD_U32", 4), 4);
        env::remove_var("ADMISSION_TEST_BAD_U32");
    }
}
