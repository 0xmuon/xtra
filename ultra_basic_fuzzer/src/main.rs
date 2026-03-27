// lets start with the basics :>
mod solution;

use std::{path::PathBuf, ptr::write};

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
    stages::mutational::StdMutationalStage,
    state::StdState,
};
use libafl_bolts::{
    current_nanos, nonnull_raw_mut, nonzero, rands::StdRand, tuples::tuple_list, AsSlice,
};

/// Coverage map for the target (no compiler instrumentation)
const SIGNALS_LEN: usize = 16;
static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = std::ptr::addr_of_mut!(SIGNALS) as _;

fn signals_set(idx: usize) {
    if idx < SIGNALS_LEN {
        unsafe { write(SIGNALS_PTR.add(idx), 1) };
    }
}

fn main() {
    env_logger::init();

    let mut harness = |input: &BytesInput| {
        let target = input.target_bytes();
        let buf = target.as_slice();
        match solution::run_from_input(buf, signals_set) {
            Ok(()) => ExitKind::Ok,
            Err(()) => ExitKind::Ok, // parse error is not a crash, just skippppppppp :>
        }
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
        .title("rj / 1535b Fuzzer")
        .enhanced_graphics(false)
        .build();

    let mut mgr = SimpleEventManager::new(mon);
    let scheduler = QueueScheduler::new();
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    let mut executor = InProcessExecutor::new(
        &mut harness,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )
    .expect("Failed to create the Executor");

    let mut generator = RandPrintablesGenerator::new(nonzero!(64));
    state
        .generate_initial_inputs(&mut fuzzer, &mut executor, &mut generator, &mut mgr, 8)
        .expect("Failed to generate the initial corpus");

    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    fuzzer
        .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
        .expect("Error in the fuzzing loop");
}
// THE END :>