use super::{
    helper::PreviewManager, installation::collect_installation, PreviewDiagnosticSnapshot,
    PreviewState,
};

impl PreviewState {
    pub(crate) fn diagnostic_snapshot(&self) -> Result<PreviewDiagnosticSnapshot, String> {
        self.with_manager(|manager| {
            Ok(PreviewDiagnosticSnapshot {
                status: collect_installation(manager, false),
                entries: manager.diagnostics(),
            })
        })
    }

    pub(super) fn with_manager<T>(
        &self,
        operation: impl FnOnce(&mut PreviewManager) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut manager = self
            .0
            .lock()
            .map_err(|_| "preview lock is poisoned".to_owned())?;
        operation(&mut manager)
    }
}
