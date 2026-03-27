mod harness;
use std::env;
use std::path::PathBuf;
use std::ptr::write;
use std::time::Instant;

#[cfg(feature = "tui")]
use libafl::monitors::tui::TuiMonitor;
#[cfg(not(feature = "tui"))]
use libafl::monitors::SimpleMonitor;
use libafl::{
    corpus::{InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::{ExitKind, InProcessExecutor},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::{BytesInput, HasTargetBytes},
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::ConstMapObserver,
    schedulers::QueueScheduler,
    stages::{
        InProcessRemoteWorkerQueue, RemoteWorkerCollectorStage, RemoteWorkerMutationalLauncherStage,
    },
    state::StdState,
};
use libafl_bolts::{
    current_nanos, nonnull_raw_mut, nonzero, rands::StdRand, tuples::tuple_list, AsSlice,
};
use log::info;

/// Synthetic coverage map (no compiler instrumentation).
const SIGNALS_LEN: usize = 16;
static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = std::ptr::addr_of_mut!(SIGNALS) as _;

fn signals_set(idx: usize) {
    if idx < SIGNALS_LEN {
        unsafe { write(SIGNALS_PTR.add(idx), 1) };
    }
}

fn parse_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn main() {
    env_logger::init();

    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let safe_workers_default = cpu_cores.saturating_sub(1).max(1);
    let hard_cap = (cpu_cores * 2).max(1);
    let configured_workers =
        parse_usize_env("TAR_REMOTE_WORKERS", safe_workers_default).clamp(1, hard_cap);
    let jobs_per_input = parse_usize_env("TAR_REMOTE_JOBS_PER_INPUT", configured_workers.max(2));
    let collect_batch = parse_usize_env("TAR_REMOTE_COLLECT_BATCH", configured_workers);

    info!(
        "tar_remote_worker startup: cpu_cores={cpu_cores}, safe_workers_default={safe_workers_default}, hard_cap={hard_cap}"
    );
    info!(
        "configured: workers={configured_workers}, jobs_per_input={jobs_per_input}, collect_batch={collect_batch}"
    );
    info!(
        "override with env: TAR_REMOTE_WORKERS, TAR_REMOTE_JOBS_PER_INPUT, TAR_REMOTE_COLLECT_BATCH"
    );

    let started = Instant::now();
    let mut total_execs = 0_u64;
    let mut harness_fn = |input: &BytesInput| {
        let target = input.target_bytes();
        let buf = target.as_slice();
        harness::run_from_input(buf, signals_set);
        total_execs += 1;
        if total_execs % 1000 == 0 {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let eps = (total_execs as f64 / elapsed) as u64;
            info!("heartbeat: total_execs={total_execs}, elapsed_s={elapsed:.1}, approx_execs_per_sec={eps}");
        }
        ExitKind::Ok
    };

    let observer =
        unsafe { ConstMapObserver::from_mut_ptr("signals", nonnull_raw_mut!(SIGNALS)) };

    let mut feedback = MaxMapFeedback::new(&observer);
    let mut objective = CrashFeedback::new();

    let mut state = StdState::new(
        StdRand::with_seed(current_nanos()),
        InMemoryCorpus::new(),
        OnDiskCorpus::new(PathBuf::from("./crashes")).unwrap(),
        &mut feedback,
        &mut objective,
    )
    .unwrap();

    #[cfg(not(feature = "tui"))]
    let mon = SimpleMonitor::new(|s| println!("{s}"));
    #[cfg(feature = "tui")]
    let mon = TuiMonitor::builder()
        .title("tar / in-process remote worker")
        .enhanced_graphics(false)
        .build();

    let mut mgr = SimpleEventManager::new(mon);
    let scheduler = QueueScheduler::new();
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    let mut executor = InProcessExecutor::new(
        &mut harness_fn,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )
    .expect("oops! Failed to create the Executor");

    let mut generator = RandPrintablesGenerator::new(nonzero!(64));
    state
        .generate_initial_inputs(&mut fuzzer, &mut executor, &mut generator, &mut mgr, 8)
        .expect("oops! Failed to generate the initial corpus");

    let queue = InProcessRemoteWorkerQueue::<BytesInput>::new();
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let launcher = RemoteWorkerMutationalLauncherStage::new(queue.clone(), mutator, jobs_per_input);
    let collector = RemoteWorkerCollectorStage::new(queue, collect_batch);
    let mut stages = tuple_list!(launcher, collector);

    info!("entering fuzz loop");
    fuzzer
        .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
        .expect("Error in the fuzzing loop");
}