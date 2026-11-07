//! Scripted completions for tests and local dry runs.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use harbor_core::{Completion, CompletionRequest, InferError};

use crate::Inference;

/// Returns queued results and counts how many times it was called.
#[derive(Debug)]
pub struct ScriptedInference {
    responses: Mutex<VecDeque<Result<Completion, InferError>>>,
    calls: AtomicUsize,
}

impl ScriptedInference {
    /// Queues `responses` in order. An empty queue yields [`InferError::NoProvider`].
    #[must_use]
    pub fn new(responses: Vec<Result<Completion, InferError>>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
            calls: AtomicUsize::new(0),
        }
    }

    /// How many times [`Inference::complete`] has been entered.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Inference for ScriptedInference {
    async fn complete(&self, _request: &CompletionRequest) -> Result<Completion, InferError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut queue = self
            .responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.pop_front().unwrap_or(Err(InferError::NoProvider))
    }
}
