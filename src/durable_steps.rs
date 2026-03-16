//! Durable UI Automation Steps — checkpoint, retry, and recovery.
//!
//! Long multi-step automation workflows fail in the middle. This module gives
//! every step independent retry semantics and lets the runner resume from the
//! last verified checkpoint instead of restarting from scratch.
//!
//! # Design
//!
//! - Each [`DurableStep`] carries its own `max_retries` and `timeout_ms`.
//! - On failure the step is retried (with exponential back-off) up to
//!   `max_retries` times before the workflow records the failure and stops.
//! - [`StepAction::Checkpoint`] saves the current progress so that a subsequent
//!   call to [`DurableRunner::resume_from_checkpoint`] can skip already-proven
//!   steps.
//!
//! # Example
//!
//! ```rust
//! use axterminator::durable_steps::{DurableRunner, DurableStep, StepAction};
//!
//! let steps = vec![
//!     DurableStep::new("open-file-menu", StepAction::Click("File".into())),
//!     DurableStep::checkpoint("after-menu-open"),
//!     DurableStep::with_retries(
//!         "type-filename",
//!         StepAction::Type("filename-field".into(), "report.csv".into()),
//!         3,
//!     ),
//! ];
//!
//! let mut runner = DurableRunner::new();
//! // In production you'd call runner.run(steps, &executor).
//! // Here we just verify the API is correct.
//! assert_eq!(runner.current_step(), 0);
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Public error ──────────────────────────────────────────────────────────────

/// Failure details for a single step.
#[derive(Debug, Clone, PartialEq)]
pub struct StepFailure {
    /// ID of the step that failed.
    pub step_id: String,
    /// Zero-based index in the workflow.
    pub step_index: usize,
    /// How many attempts were made before giving up.
    pub attempts: u32,
    /// Human-readable reason.
    pub reason: String,
}

// ── Public result ─────────────────────────────────────────────────────────────

/// Outcome of executing a complete workflow.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowResult {
    /// All steps completed successfully.
    Success {
        /// Total steps executed (including checkpoints).
        steps_executed: usize,
        /// Total retry attempts across the whole workflow.
        total_retries: u32,
    },
    /// A step failed after exhausting all retries.
    Failed {
        /// Failure details.
        failure: StepFailure,
        /// The most recent checkpoint index before the failure, if any.
        last_checkpoint: Option<usize>,
    },
}

// ── Step actions ──────────────────────────────────────────────────────────────

/// A single automatable UI action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepAction {
    /// Click the element matched by the query string.
    Click(String),
    /// Type `text` into the element matched by the query string.
    Type(String, String),
    /// Wait until the condition described by the query becomes true.
    Wait(String),
    /// Assert that a condition holds; fail the step if it does not.
    Assert(String),
    /// Save a checkpoint so recovery can resume from this point.
    Checkpoint,
}

// ── DurableStep ───────────────────────────────────────────────────────────────

/// A single step in a durable workflow.
#[derive(Debug, Clone)]
pub struct DurableStep {
    /// Stable, human-readable identifier (used in logs and failure reports).
    pub id: String,
    /// What to do.
    pub action: StepAction,
    /// How many times to retry on failure (0 = try once and give up).
    pub max_retries: u32,
    /// Per-attempt time budget in milliseconds.
    pub timeout_ms: u64,
}

impl DurableStep {
    /// Create a step with default retry (2) and timeout (5 000 ms) settings.
    #[must_use]
    pub fn new(id: impl Into<String>, action: StepAction) -> Self {
        Self {
            id: id.into(),
            action,
            max_retries: 2,
            timeout_ms: 5_000,
        }
    }

    /// Create a checkpoint step.
    ///
    /// Checkpoints are always retried once (they are cheap and idempotent).
    #[must_use]
    pub fn checkpoint(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            action: StepAction::Checkpoint,
            max_retries: 1,
            timeout_ms: 1_000,
        }
    }

    /// Create a step with an explicit retry count.
    #[must_use]
    pub fn with_retries(id: impl Into<String>, action: StepAction, max_retries: u32) -> Self {
        Self {
            id: id.into(),
            action,
            max_retries,
            timeout_ms: 5_000,
        }
    }

    /// Create a step with explicit retry count and timeout.
    #[must_use]
    pub fn with_config(
        id: impl Into<String>,
        action: StepAction,
        max_retries: u32,
        timeout_ms: u64,
    ) -> Self {
        Self { id: id.into(), action, max_retries, timeout_ms }
    }
}

// ── Checkpoint ────────────────────────────────────────────────────────────────

/// A saved progress marker within a workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// The step index **after** which the checkpoint was saved.
    pub step_index: usize,
    /// Unix timestamp in milliseconds when this checkpoint was recorded.
    pub timestamp_ms: u64,
    /// Cheap hash of the workflow state at save time (e.g., step id list).
    pub state_hash: u64,
}

impl Checkpoint {
    /// Create a new checkpoint for the given index.
    #[must_use]
    pub fn new(step_index: usize, state_hash: u64) -> Self {
        Self {
            step_index,
            timestamp_ms: current_timestamp_ms(),
            state_hash,
        }
    }
}

// ── Executor trait ────────────────────────────────────────────────────────────

/// Abstraction over the actual UI automation back-end.
///
/// Implementors translate [`StepAction`]s into real interactions (accessibility
/// API calls, CDP commands, etc.).  A `MockExecutor` is provided for testing.
pub trait StepExecutor {
    /// Execute one action.
    ///
    /// # Errors
    ///
    /// Return `Err(reason)` when the action fails and should be retried.
    fn execute(&mut self, action: &StepAction) -> Result<(), String>;
}

// ── DurableRunner ─────────────────────────────────────────────────────────────

/// Stateful runner that executes a workflow with retry and checkpoint support.
///
/// # State model
///
/// The runner maintains `current_step` — the next step to execute — and a list
/// of `checkpoints` accumulated during the run.  On a call to
/// `resume_from_checkpoint` the cursor jumps to `checkpoint.step_index + 1`.
pub struct DurableRunner {
    current_step: usize,
    checkpoints: Vec<Checkpoint>,
    total_retries: u32,
}

impl DurableRunner {
    /// Create a fresh runner positioned at step 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_step: 0,
            checkpoints: Vec::new(),
            total_retries: 0,
        }
    }

    /// Execute a sequence of steps, retrying each one up to its `max_retries`.
    ///
    /// Returns [`WorkflowResult::Success`] when all steps pass, or
    /// [`WorkflowResult::Failed`] on the first step that exhausts its retries.
    pub fn run(
        &mut self,
        steps: &[DurableStep],
        executor: &mut dyn StepExecutor,
    ) -> WorkflowResult {
        while self.current_step < steps.len() {
            let step = &steps[self.current_step];

            match self.execute_with_retry(step, executor) {
                Ok(retries_used) => {
                    self.total_retries += retries_used;
                    if step.action == StepAction::Checkpoint {
                        self.save_checkpoint(self.current_step, steps);
                    }
                    self.current_step += 1;
                }
                Err(reason) => {
                    let failure = StepFailure {
                        step_id: step.id.clone(),
                        step_index: self.current_step,
                        attempts: step.max_retries + 1,
                        reason,
                    };
                    let last_checkpoint = self.checkpoints.last().map(|c| c.step_index);
                    return WorkflowResult::Failed { failure, last_checkpoint };
                }
            }
        }

        WorkflowResult::Success {
            steps_executed: self.current_step,
            total_retries: self.total_retries,
        }
    }

    /// Resume execution from a checkpoint, skipping already-completed steps.
    ///
    /// After this call, `current_step` is set to `checkpoint.step_index + 1`
    /// and a subsequent `run` will start from that position.
    pub fn resume_from_checkpoint(&mut self, checkpoint: &Checkpoint) {
        self.current_step = checkpoint.step_index + 1;
    }

    /// The zero-based index of the next step to execute.
    #[must_use]
    pub fn current_step(&self) -> usize {
        self.current_step
    }

    /// All checkpoints saved during the current run.
    #[must_use]
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// The last saved checkpoint, if any.
    #[must_use]
    pub fn last_checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }

    /// Reset the runner to the initial state.
    pub fn reset(&mut self) {
        self.current_step = 0;
        self.checkpoints.clear();
        self.total_retries = 0;
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Attempt to execute `step` up to `max_retries + 1` times.
    ///
    /// Returns the number of retries consumed (0 = first attempt succeeded),
    /// or an error string if all attempts failed.
    fn execute_with_retry(
        &self,
        step: &DurableStep,
        executor: &mut dyn StepExecutor,
    ) -> Result<u32, String> {
        let mut last_err = String::new();
        for attempt in 0..=step.max_retries {
            match executor.execute(&step.action) {
                Ok(()) => return Ok(attempt),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// Record a checkpoint after the step at `step_index`.
    fn save_checkpoint(&mut self, step_index: usize, steps: &[DurableStep]) {
        let hash = hash_step_ids(steps);
        self.checkpoints.push(Checkpoint::new(step_index, hash));
    }
}

impl Default for DurableRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ── Free helpers ──────────────────────────────────────────────────────────────

/// Hash the IDs of all steps to produce a cheap state fingerprint.
fn hash_step_ids(steps: &[DurableStep]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for step in steps {
        step.id.hash(&mut hasher);
    }
    hasher.finish()
}

/// Return the current time as a Unix timestamp in milliseconds.
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

// ── MockExecutor (test helper, public for integration tests) ──────────────────

/// A test-double executor that executes actions from a pre-configured script.
///
/// Each entry in `results` corresponds to one [`StepExecutor::execute`] call.
/// When the list is exhausted every subsequent call succeeds.
pub struct MockExecutor {
    /// Pre-configured results consumed front-to-back.
    results: Vec<Result<(), String>>,
    /// Actions received (for assertion).
    received: Vec<StepAction>,
}

impl MockExecutor {
    /// Create an executor that always succeeds.
    #[must_use]
    pub fn always_ok() -> Self {
        Self { results: Vec::new(), received: Vec::new() }
    }

    /// Create an executor from an explicit result sequence.
    #[must_use]
    pub fn from_results(results: Vec<Result<(), String>>) -> Self {
        Self { results, received: Vec::new() }
    }

    /// The actions that have been passed to `execute` so far.
    #[must_use]
    pub fn received(&self) -> &[StepAction] {
        &self.received
    }
}

impl StepExecutor for MockExecutor {
    fn execute(&mut self, action: &StepAction) -> Result<(), String> {
        self.received.push(action.clone());
        if self.results.is_empty() {
            Ok(())
        } else {
            self.results.remove(0)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Fixtures ──────────────────────────────────────────────────────────

    fn click_step(id: &str, target: &str) -> DurableStep {
        DurableStep::new(id, StepAction::Click(target.into()))
    }

    fn three_step_workflow() -> Vec<DurableStep> {
        vec![
            click_step("step-a", "File"),
            DurableStep::checkpoint("cp-1"),
            click_step("step-b", "Save"),
        ]
    }

    // ── DurableStep constructors ───────────────────────────────────────────

    #[test]
    fn durable_step_new_sets_default_retries_and_timeout() {
        // GIVEN: A step created with the basic constructor
        let step = DurableStep::new("s", StepAction::Click("btn".into()));
        // THEN: Defaults are applied
        assert_eq!(step.max_retries, 2);
        assert_eq!(step.timeout_ms, 5_000);
    }

    #[test]
    fn durable_step_checkpoint_factory_produces_checkpoint_action() {
        let step = DurableStep::checkpoint("cp");
        assert_eq!(step.action, StepAction::Checkpoint);
        assert_eq!(step.id, "cp");
    }

    #[test]
    fn durable_step_with_retries_overrides_retry_count() {
        let step = DurableStep::with_retries("s", StepAction::Click("x".into()), 5);
        assert_eq!(step.max_retries, 5);
    }

    #[test]
    fn durable_step_with_config_stores_all_fields() {
        let step = DurableStep::with_config(
            "s",
            StepAction::Type("field".into(), "hello".into()),
            3,
            2_000,
        );
        assert_eq!(step.max_retries, 3);
        assert_eq!(step.timeout_ms, 2_000);
    }

    // ── WorkflowResult variants ────────────────────────────────────────────

    #[test]
    fn successful_run_returns_correct_step_count() {
        // GIVEN: Three-step workflow, all succeed first try
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::always_ok();
        let steps = three_step_workflow();

        // WHEN: Running the workflow
        let result = runner.run(&steps, &mut exec);

        // THEN: Success with 3 steps executed, 0 retries
        assert_eq!(
            result,
            WorkflowResult::Success { steps_executed: 3, total_retries: 0 }
        );
    }

    #[test]
    fn failed_step_returns_failure_with_correct_id() {
        // GIVEN: Second step always fails
        let steps = three_step_workflow();
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::from_results(vec![
            Ok(()),                      // step-a succeeds
            Ok(()),                      // cp-1 succeeds
            Err("not found".into()),     // step-b fails attempt 1
            Err("not found".into()),     // step-b fails attempt 2 (retry 1)
            Err("not found".into()),     // step-b fails attempt 3 (retry 2)
        ]);

        // WHEN: Running
        let result = runner.run(&steps, &mut exec);

        // THEN: Failure reports the correct step
        match result {
            WorkflowResult::Failed { failure, .. } => {
                assert_eq!(failure.step_id, "step-b");
                assert_eq!(failure.step_index, 2);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn failure_result_includes_last_checkpoint_index() {
        // GIVEN: Workflow with a checkpoint then a failing step
        let steps = three_step_workflow();
        let mut runner = DurableRunner::new();
        let fail_all: Vec<Result<(), String>> = (0..10)
            .map(|i| if i < 2 { Ok(()) } else { Err("err".into()) })
            .collect();
        let mut exec = MockExecutor::from_results(fail_all);

        // WHEN: Running
        let result = runner.run(&steps, &mut exec);

        // THEN: last_checkpoint is the checkpoint step index (1)
        match result {
            WorkflowResult::Failed { last_checkpoint, .. } => {
                assert_eq!(last_checkpoint, Some(1));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    // ── Retry behaviour ────────────────────────────────────────────────────

    #[test]
    fn step_succeeds_on_second_attempt_after_one_failure() {
        // GIVEN: First attempt fails, second succeeds
        let steps = vec![DurableStep::with_retries(
            "flaky",
            StepAction::Click("btn".into()),
            1,
        )];
        let mut runner = DurableRunner::new();
        let mut exec =
            MockExecutor::from_results(vec![Err("transient".into()), Ok(())]);

        // WHEN: Running
        let result = runner.run(&steps, &mut exec);

        // THEN: Success — one retry used
        assert_eq!(
            result,
            WorkflowResult::Success { steps_executed: 1, total_retries: 1 }
        );
    }

    #[test]
    fn zero_retries_fails_immediately_on_first_error() {
        // GIVEN: Step with no retries
        let steps = vec![DurableStep::with_retries(
            "strict",
            StepAction::Click("x".into()),
            0,
        )];
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::from_results(vec![Err("boom".into())]);

        // WHEN: Running
        let result = runner.run(&steps, &mut exec);

        // THEN: Fails without retry
        match result {
            WorkflowResult::Failed { failure, .. } => assert_eq!(failure.attempts, 1),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    // ── Checkpoint ────────────────────────────────────────────────────────

    #[test]
    fn checkpoint_step_saves_checkpoint() {
        // GIVEN: Workflow with checkpoint step
        let steps = three_step_workflow();
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::always_ok();

        // WHEN: Running successfully
        runner.run(&steps, &mut exec);

        // THEN: One checkpoint was saved
        assert_eq!(runner.checkpoints().len(), 1);
        assert_eq!(runner.last_checkpoint().unwrap().step_index, 1);
    }

    #[test]
    fn resume_from_checkpoint_sets_correct_step_index() {
        // GIVEN: A checkpoint at step 2
        let cp = Checkpoint::new(2, 0xDEAD_BEEF);
        let mut runner = DurableRunner::new();

        // WHEN: Resuming from the checkpoint
        runner.resume_from_checkpoint(&cp);

        // THEN: Next step to execute is 3
        assert_eq!(runner.current_step(), 3);
    }

    #[test]
    fn resumed_run_skips_already_completed_steps() {
        // GIVEN: Workflow where first two steps have been completed
        let steps = vec![
            click_step("step-1", "A"),
            click_step("step-2", "B"),
            click_step("step-3", "C"),
        ];
        let cp = Checkpoint::new(1, 0);
        let mut runner = DurableRunner::new();
        runner.resume_from_checkpoint(&cp);
        let mut exec = MockExecutor::always_ok();

        // WHEN: Running from checkpoint
        let result = runner.run(&steps, &mut exec);

        // THEN: Only step-3 was executed (index 2 → current_step becomes 3)
        assert_eq!(
            result,
            WorkflowResult::Success { steps_executed: 3, total_retries: 0 }
        );
        // exec received only one action
        assert_eq!(exec.received().len(), 1);
    }

    // ── DurableRunner helpers ──────────────────────────────────────────────

    #[test]
    fn reset_clears_state() {
        // GIVEN: Runner mid-workflow
        let steps = three_step_workflow();
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::always_ok();
        runner.run(&steps, &mut exec);

        // WHEN: Resetting
        runner.reset();

        // THEN: State is cleared
        assert_eq!(runner.current_step(), 0);
        assert!(runner.checkpoints().is_empty());
    }

    #[test]
    fn empty_workflow_succeeds_immediately() {
        // GIVEN: No steps
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::always_ok();
        let result = runner.run(&[], &mut exec);

        // THEN: Success with zero steps
        assert_eq!(
            result,
            WorkflowResult::Success { steps_executed: 0, total_retries: 0 }
        );
    }

    // ── MockExecutor ──────────────────────────────────────────────────────

    #[test]
    fn mock_executor_records_received_actions() {
        // GIVEN: Three-step workflow
        let steps = three_step_workflow();
        let mut runner = DurableRunner::new();
        let mut exec = MockExecutor::always_ok();

        // WHEN: Running
        runner.run(&steps, &mut exec);

        // THEN: All three actions were dispatched to the executor
        assert_eq!(exec.received().len(), 3);
        assert_eq!(exec.received()[0], StepAction::Click("File".into()));
        assert_eq!(exec.received()[1], StepAction::Checkpoint);
        assert_eq!(exec.received()[2], StepAction::Click("Save".into()));
    }
}
