//! The engine calls this between a remote call and the manifest write.
//! A stop is a crash: the in-memory engine is discarded and the next open reads the disk.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    BeforeRemoteCall,
    AfterRemoteCall,
    BeforeManifestWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Control {
    Continue,
    Stop,
}

pub(super) trait Hook: Send {
    fn on_step(&mut self, step: Step, path: &str) -> Control;
}

impl<F> Hook for F
where
    F: FnMut(Step, &str) -> Control + Send,
{
    fn on_step(&mut self, step: Step, path: &str) -> Control {
        self(step, path)
    }
}

pub(super) fn gate(hook: &mut dyn Hook, step: Step, path: &str) -> bool {
    match hook.on_step(step, path) {
        Control::Continue => true,
        Control::Stop => false,
    }
}
