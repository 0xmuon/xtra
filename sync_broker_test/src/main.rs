//! Minimal fuzzer to test SyncFromBrokerStage and corpus_size reporting.
//!
//! Run with broker + clients (e.g. `--cores 0,1`). Clients use SyncFromBrokerStage
//! to send their corpus to the broker; the broker should display non-zero corpus
//! for each client when the fix (state.corpus().count()) is in place.

#![cfg(feature = "std")]

use std::path::PathBuf;

use clap::Parser;
use libafl::{
    corpus::{InMemoryCorpus, OnDiskCorpus},
    events::{launcher::Launcher, llmp::LlmpEventConverter, EventConfig, LlmpRestartingEventManager},
    executors::{inprocess::InProcessExecutor, ExitKind},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::{
        BytesInput, BytesInputConverter, FromBytesInputConverter, HasTargetBytes,
        ToBytesInputConverter,
    },
    monitors::SimpleMonitor,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::ConstMapObserver,
    schedulers::QueueScheduler,
    stages::{mutational::StdMutationalStage, sync::SyncFromBrokerStage},
    state::StdState,
    Error,
};
use libafl_bolts::{
    core_affinity::Cores,
    nonnull_raw_mut,
    rands::StdRand,
    shmem::{ShMemProvider, StdShMemProvider},
    tuples::tuple_list,
};

/// State type matching launcher's generic parameters (corpus, input, rand, solutions).
type SyncBrokerState =
    StdState<InMemoryCorpus<BytesInput>, BytesInput, StdRand, OnDiskCorpus<BytesInput>>;

/// Coverage map (no instrumentation; for testing only)
const SIGNALS_LEN: usize = 16;
static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = core::ptr::addr_of_mut!(SIGNALS).cast::<u8>();

fn signals_set(idx: usize) {
    unsafe { std::ptr::write(SIGNALS_PTR.add(idx), 1) };
}

#[derive(Debug, Parser)]
#[command(
    name = "sync_broker_test",
    about = "Minimal fuzzer to test SyncFromBrokerStage and corpus_size reporting to the broker"
)]
struct Opt {
    #[arg(
        short,
        long,
        value_parser = Cores::from_cmdline,
        help = "Cores for broker (0) and clients (e.g. 0,1 for broker+1 client)",
        default_value = "0"
    )]
    cores: Cores,

    #[arg(short = 'p', long, default_value = "1337", help = "Broker TCP port")]
    broker_port: u16,

    #[arg(short, long, default_value = "./out", help = "Output directory")]
    output: PathBuf,
}

fn main() -> Result<(), Error> {
    env_logger::init();
    let opt = Opt::parse();

    let shmem_provider = StdShMemProvider::new().expect("Failed to init shared memory");
    let provider_for_builder = shmem_provider.clone();
    let monitor = SimpleMonitor::new(|s| println!("{s}"));
    let broker_port = opt.broker_port;
    let cores = opt.cores;

    let mut run_client = move |state: Option<SyncBrokerState>,
                               mut mgr: LlmpRestartingEventManager<
        (),
        BytesInput,
        SyncBrokerState,
        libafl_bolts::shmem::StdShMem,
        StdShMemProvider,
    >,
                               _client_description: libafl::events::launcher::ClientDescription| {
        let mut harness = |input: &BytesInput| {
            let buf: &[u8] = &*input.target_bytes();
            signals_set(0);
            if !buf.is_empty() && buf[0] == b'a' {
                signals_set(1);
            }
            ExitKind::Ok
        };

        let observer = unsafe {
            ConstMapObserver::from_mut_ptr(
                "signals",
                nonnull_raw_mut!(SIGNALS),
            )
        };

        let mut feedback = MaxMapFeedback::new(&observer);
        let mut objective = CrashFeedback::new();

        let mut state = state.unwrap_or_else(|| {
            SyncBrokerState::new(
                StdRand::new(),
                InMemoryCorpus::new(),
                OnDiskCorpus::new(PathBuf::from("./crashes")).unwrap(),
                &mut feedback,
                &mut objective,
            )
            .unwrap()
        });

        let scheduler = QueueScheduler::new();
        let mut fuzzer = StdFuzzer::builder()
            .scheduler(scheduler)
            .feedback(feedback)
            .objective(objective)
            .target_bytes_converter::<BytesInput, _>(BytesInputConverter::new())
            .build();

        let mut executor = InProcessExecutor::new(
            &mut harness,
            tuple_list!(observer),
            &mut fuzzer,
            &mut state,
            &mut mgr,
        )
        .expect("Failed to create executor");

        let mut generator = RandPrintablesGenerator::new(libafl_bolts::nonzero!(32_usize));
        state
            .generate_initial_inputs_forced(&mut fuzzer, &mut executor, &mut generator, &mut mgr, 8)
            .expect("Failed to generate initial corpus");

        let mutator = HavocScheduledMutator::new(havoc_mutations());

        // Build LlmpEventConverter (second connection to same broker) for SyncFromBrokerStage
        let conv = LlmpEventConverter::builder().build_on_port(
            shmem_provider.clone(),
            broker_port,
            Some(ToBytesInputConverter::new(BytesInputConverter::new())),
            Some(FromBytesInputConverter::new(BytesInputConverter::new())),
        )?;

        let mut stages = tuple_list!(
            StdMutationalStage::new(mutator),
            SyncFromBrokerStage::new(conv)
        );

        fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
        Ok(())
    };

    match Launcher::builder()
        .shmem_provider(provider_for_builder)
        .configuration(EventConfig::from_name("sync_broker_test"))
        .monitor(monitor)
        .run_client(&mut run_client)
        .cores(&cores)
        .broker_port(broker_port)
        .build()
        .launch()
    {
        Ok(()) => {}
        Err(Error::ShuttingDown) => println!("Fuzzing stopped. Good bye."),
        Err(e) => return Err(e),
    }
    Ok(())
}
