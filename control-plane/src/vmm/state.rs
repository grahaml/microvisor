use std::fmt;
use tracing::{info, instrument};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Pending,
    ProvisioningStorage,
    ConfiguringNetwork,
    InitializingCgroup,
    LaunchingVMM,
    Running,
    Terminating,
    Destroyed,
}

impl fmt::Display for VmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct VmStateMachine {
    state: VmState,
    session_id: u64,
}

impl VmStateMachine {
    pub fn new(session_id: u64) -> Self {
        Self {
            state: VmState::Pending,
            session_id,
        }
    }

    pub fn current_state(&self) -> VmState {
        self.state
    }

    #[instrument(skip(self), fields(session_id = %self.session_id, from = %self.state, to = %next_state))]
    pub fn transition_to(&mut self, next_state: VmState) -> Result<(), String> {
        if self.is_valid_transition(next_state) {
            info!(session_id = %self.session_id, "Transitioning state: {} -> {}", self.state, next_state);
            self.state = next_state;
            Ok(())
        } else {
            let err = format!("Invalid state transition: {} -> {}", self.state, next_state);
            tracing::error!(session_id = %self.session_id, "{}", err);
            Err(err)
        }
    }

    fn is_valid_transition(&self, next_state: VmState) -> bool {
        match (self.state, next_state) {
            (VmState::Pending, VmState::ProvisioningStorage) => true,
            (VmState::ProvisioningStorage, VmState::ConfiguringNetwork) => true,
            (VmState::ConfiguringNetwork, VmState::InitializingCgroup) => true,
            (VmState::InitializingCgroup, VmState::LaunchingVMM) => true,
            (VmState::LaunchingVMM, VmState::Running) => true,
            (VmState::Running, VmState::Terminating) => true,
            (VmState::Terminating, VmState::Destroyed) => true,
            // Allow transitions to Terminating from almost anywhere for emergency teardown
            (VmState::ProvisioningStorage, VmState::Terminating) => true,
            (VmState::ConfiguringNetwork, VmState::Terminating) => true,
            (VmState::InitializingCgroup, VmState::Terminating) => true,
            (VmState::LaunchingVMM, VmState::Terminating) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct OrchestratorError {
    pub session_id: u64,
    pub state: VmState,
    pub message: String,
}

impl std::error::Error for OrchestratorError {}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Session {}][State {}] Error: {}", self.session_id, self.state, self.message)
    }
}

impl OrchestratorError {
    pub fn new(session_id: u64, state: VmState, message: String) -> Self {
        Self { session_id, state, message }
    }
}
