//! In-process prototype for a Remote Worker Stage.
//!
//! This is a correctness-focused first step for the GSoC Remote Worker Stage proposal:
//! - a launcher stage enqueues work packages into an in-memory queue
//! - a collector stage dequeues work packages, runs the executor, and integrates results into
//!   the main node via `ExecutionProcessor::evaluate_execution`
//!
//! The separation into work/results messages allows later replacement of the queue backend and
//! moving the worker loop into a standalone process.

use alloc::{
    borrow::Cow,
    collections::VecDeque,
    rc::Rc,
    vec::Vec,
};
use core::{cell::RefCell, marker::PhantomData};

use serde::{Deserialize, Serialize};

use libafl_bolts::Named;

use crate::{
    Error,
    executors::{Executor, ExitKind, HasObservers},
    fuzzer::{ExecutionProcessor, HasScheduler},
    inputs::Input,
    mutators::{MutationResult, Mutator},
    observers::ObserversTuple,
    schedulers::Scheduler,
    state::{
        HasCorpus, HasCurrentTestcase, HasExecutions, HasLastFoundTime, HasSolutions,
        MaybeHasClientPerfMonitor,
    },
    stages::{Restartable, Stage},
};

/// Default name for `RemoteWorkerLauncherStage`.
pub const REMOTE_WORKER_LAUNCHER_STAGE_NAME: &str = "remote_worker_launcher_inprocess";
/// Default name for `RemoteWorkerMutationalLauncherStage`.
pub const REMOTE_WORKER_MUTATIONAL_LAUNCHER_STAGE_NAME: &str =
    "remote_worker_mutational_launcher_inprocess";

/// Default name for `RemoteWorkerCollectorStage`.
pub const REMOTE_WORKER_COLLECTOR_STAGE_NAME: &str = "remote_worker_collector_inprocess";

/// Work package sent from the launcher to the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWorkerWorkPackage<I> {
    /// Serialized/transported fuzzer input for the worker to execute.
    pub input: I,
}

/// Result message sent from the worker to the collector.
///
/// For the in-process prototype we still serialize observers, to keep the message boundary close
/// to the future remote version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWorkerResult<I> {
    /// The original input corresponding to this execution result.
    pub input: I,
    /// The worker’s execution outcome.
    pub exit_kind: ExitKind,
    /// Serialized observer tuple produced during execution.
    pub observers_buf: Vec<u8>,
}

/// A shared in-memory queue for the in-process remote worker prototype.
#[derive(Debug, Clone)]
pub struct InProcessRemoteWorkerQueue<I> {
    work: Rc<RefCell<VecDeque<RemoteWorkerWorkPackage<I>>>>,
    results: Rc<RefCell<VecDeque<RemoteWorkerResult<I>>>>,
}

impl<I> InProcessRemoteWorkerQueue<I> {
    /// Create an empty in-process queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            work: Rc::new(RefCell::new(VecDeque::new())),
            results: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Enqueue a new work package.
    pub fn push_work(&self, work: RemoteWorkerWorkPackage<I>) {
        self.work.borrow_mut().push_back(work);
    }

    /// Pop one work package, if any.
    pub fn pop_work(&self) -> Option<RemoteWorkerWorkPackage<I>> {
        self.work.borrow_mut().pop_front()
    }

    /// Enqueue a worker result.
    pub fn push_result(&self, res: RemoteWorkerResult<I>) {
        self.results.borrow_mut().push_back(res);
    }

    /// Pop one worker result, if any.
    pub fn pop_result(&self) -> Option<RemoteWorkerResult<I>> {
        self.results.borrow_mut().pop_front()
    }
}

/// Stage that enqueues work packages for the remote worker (in-process prototype).
#[derive(Debug, Clone)]
pub struct RemoteWorkerLauncherStage<I> {
    name: Cow<'static, str>,
    queue: InProcessRemoteWorkerQueue<I>,
    jobs_per_input: usize,
    phantom: PhantomData<I>,
}

impl<I> RemoteWorkerLauncherStage<I> {
    /// Create a launcher stage with an associated queue.
    #[must_use]
    pub fn new(queue: InProcessRemoteWorkerQueue<I>, jobs_per_input: usize) -> Self {
        Self {
            name: Cow::Borrowed(REMOTE_WORKER_LAUNCHER_STAGE_NAME),
            queue,
            jobs_per_input: jobs_per_input.max(1),
            phantom: PhantomData,
        }
    }
}

impl<I> Named for RemoteWorkerLauncherStage<I> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<E, EM, S, Z, I> Stage<E, EM, S, Z> for RemoteWorkerLauncherStage<I>
where
    I: Input,
    S: HasCurrentTestcase<I>,
{
    fn perform(
        &mut self,
        _fuzzer: &mut Z,
        _executor: &mut E,
        state: &mut S,
        _manager: &mut EM,
    ) -> Result<(), Error> {
        let input = state.current_input_cloned()?;
        for _ in 0..self.jobs_per_input {
            self.queue.push_work(RemoteWorkerWorkPackage {
                input: input.clone(),
            });
        }
        Ok(())
    }
}

impl<S, I> Restartable<S> for RemoteWorkerLauncherStage<I> {
    fn should_restart(&mut self, _state: &mut S) -> Result<bool, Error> {
        // Launcher only enqueues work; it is safe to rerun.
        Ok(true)
    }

    fn clear_progress(&mut self, _state: &mut S) -> Result<(), Error> {
        Ok(())
    }
}

/// Stage that enqueues mutated work packages for the remote worker (in-process prototype).
#[derive(Debug, Clone)]
pub struct RemoteWorkerMutationalLauncherStage<I, M> {
    name: Cow<'static, str>,
    queue: InProcessRemoteWorkerQueue<I>,
    mutator: M,
    jobs_per_input: usize,
    phantom: PhantomData<I>,
}

impl<I, M> RemoteWorkerMutationalLauncherStage<I, M> {
    /// Create a mutational launcher stage with an associated queue.
    #[must_use]
    pub fn new(queue: InProcessRemoteWorkerQueue<I>, mutator: M, jobs_per_input: usize) -> Self {
        Self {
            name: Cow::Borrowed(REMOTE_WORKER_MUTATIONAL_LAUNCHER_STAGE_NAME),
            queue,
            mutator,
            jobs_per_input: jobs_per_input.max(1),
            phantom: PhantomData,
        }
    }
}

impl<I, M> Named for RemoteWorkerMutationalLauncherStage<I, M> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<E, EM, S, Z, I, M> Stage<E, EM, S, Z> for RemoteWorkerMutationalLauncherStage<I, M>
where
    I: Input + Clone,
    M: Mutator<I, S>,
    S: HasCurrentTestcase<I>,
{
    fn perform(
        &mut self,
        _fuzzer: &mut Z,
        _executor: &mut E,
        state: &mut S,
        _manager: &mut EM,
    ) -> Result<(), Error> {
        let base = state.current_input_cloned()?;
        for _ in 0..self.jobs_per_input {
            let mut input = base.clone();
            if self.mutator.mutate(state, &mut input)? == MutationResult::Mutated {
                self.queue.push_work(RemoteWorkerWorkPackage { input });
            }
        }
        Ok(())
    }
}

impl<S, I, M> Restartable<S> for RemoteWorkerMutationalLauncherStage<I, M> {
    fn should_restart(&mut self, _state: &mut S) -> Result<bool, Error> {
        Ok(true)
    }

    fn clear_progress(&mut self, _state: &mut S) -> Result<(), Error> {
        Ok(())
    }
}

/// Stage that dequeues work packages and integrates results into the main node.
#[derive(Debug, Clone)]
pub struct RemoteWorkerCollectorStage<I> {
    name: Cow<'static, str>,
    queue: InProcessRemoteWorkerQueue<I>,
    max_work_per_perform: usize,
    phantom: PhantomData<I>,
}

impl<I> RemoteWorkerCollectorStage<I> {
    /// Create a collector stage with an associated queue.
    #[must_use]
    pub fn new(queue: InProcessRemoteWorkerQueue<I>, max_work_per_perform: usize) -> Self {
        Self {
            name: Cow::Borrowed(REMOTE_WORKER_COLLECTOR_STAGE_NAME),
            queue,
            max_work_per_perform: max_work_per_perform.max(1),
            phantom: PhantomData,
        }
    }
}

impl<I> Named for RemoteWorkerCollectorStage<I> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<E, EM, S, Z, I> Stage<E, EM, S, Z> for RemoteWorkerCollectorStage<I>
where
    I: Input,
    E: Executor<EM, I, S, Z> + HasObservers,
    E::Observers: ObserversTuple<I, S> + Serialize + for<'de> Deserialize<'de>,
    EM: crate::events::EventFirer<I, S>,
    S: HasCorpus<I>
        + HasSolutions<I>
        + HasCurrentTestcase<I>
        + HasExecutions
        + HasLastFoundTime
        + MaybeHasClientPerfMonitor,
    Z: HasScheduler<I, S> + ExecutionProcessor<EM, I, E::Observers, S>,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error> {
        // Worker part (in-process):
        for _ in 0..self.max_work_per_perform {
            let Some(work) = self.queue.pop_work() else {
                break;
            };

            let exit_kind = match executor.run_target(fuzzer, state, manager, &work.input) {
                Ok(exit_kind) => exit_kind,
                Err(e) => {
                    // Prototype behavior: surface execution failures to the caller.
                    // In the remote version we may want retry/backoff and skipping.
                    return Err(e);
                }
            };

            let observers = executor.observers();
            let observers_buf = postcard::to_allocvec(&*observers)?;

            self.queue.push_result(RemoteWorkerResult {
                input: work.input,
                exit_kind,
                observers_buf,
            });
        }

        // Collector part:
        for _ in 0..self.max_work_per_perform {
            let Some(res) = self.queue.pop_result() else {
                break;
            };

            // For this in-process prototype, evaluate with the executor's live observers.
            // Deserialized observers do not preserve runtime observer handles required by map feedbacks.
            let _ = postcard::from_bytes::<E::Observers>(&res.observers_buf)?;
            let observers = executor.observers();

            // Keep scheduler accounting consistent with `Evaluator::evaluate_input_with_observers`.
            fuzzer
                .scheduler_mut()
                .on_evaluation(state, &res.input, &*observers)?;

            let _ = fuzzer.evaluate_execution(
                state,
                manager,
                &res.input,
                &*observers,
                &res.exit_kind,
                true, // send events as in the standard evaluation path
            )?;
        }

        Ok(())
    }
}

impl<S, I> Restartable<S> for RemoteWorkerCollectorStage<I> {
    fn should_restart(&mut self, _state: &mut S) -> Result<bool, Error> {
        // Prototype behavior: always run.
        // A production version should include robust restart semantics.
        Ok(true)
    }

    fn clear_progress(&mut self, _state: &mut S) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        corpus::{Corpus, HasCurrentCorpusId, InMemoryCorpus},
        executors::nop::ConstantExecutor,
        events::NopEventManager,
        feedbacks::CrashFeedback,
        fuzzer::StdFuzzer,
        inputs::NopInput,
        schedulers::RandScheduler,
        state::StdState,
    };
    use libafl_bolts::rands::StdRand;

    // Basic correctness: enqueue input, execute it, integrate as a solution on crash.
    #[test]
    fn inprocess_remote_worker_happy_path() {
        let queue = InProcessRemoteWorkerQueue::<NopInput>::new();

        let launcher = RemoteWorkerLauncherStage::new(queue.clone(), 1);
        let mut collector = RemoteWorkerCollectorStage::new(queue.clone(), 1);

        let mut corpus = InMemoryCorpus::new();
        let solutions = InMemoryCorpus::new();

        let id = corpus.add(NopInput {}.into()).unwrap();

        // Configure fuzzer to treat crashes as solutions.
        type TestState = StdState<InMemoryCorpus<NopInput>, NopInput, StdRand, InMemoryCorpus<NopInput>>;
        let scheduler: RandScheduler<TestState> = RandScheduler::new();
        let mut feedback = ();
        let mut objective = CrashFeedback::new();

        let mut state = StdState::new(
            StdRand::with_seed(0),
            corpus,
            solutions,
            &mut feedback,
            &mut objective,
        )
        .unwrap();

        let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

        // The executor will return Crash; keep current ids aligned.
        state.set_corpus_id(id).unwrap();
        *state.corpus_mut().current_mut() = Some(id);

        let mut executor = ConstantExecutor::crash();
        let mut manager: NopEventManager = NopEventManager::new();

        // Enqueue work for the current input.
        let mut launcher = launcher;
        launcher
            .perform(&mut fuzzer, &mut executor, &mut state, &mut manager)
            .unwrap();

        // Dequeue work, "run" it, integrate results.
        collector
            .perform(&mut fuzzer, &mut executor, &mut state, &mut manager)
            .unwrap();

        assert_eq!(state.solutions().count(), 1);
    }
}

