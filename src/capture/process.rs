//! Managed capture subprocess — full lifecycle (stderr draining,
//! bounded stop sequence) is implemented in the next task.

/// A spawned child process owned by the capture controller.
pub struct ManagedChild {
    /// The underlying child handle.
    pub child: std::process::Child,
}

impl ManagedChild {
    /// OS process id of the child.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
