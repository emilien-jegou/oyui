pub enum CommandMode {
    Normal,
    Active(String),
    ConfirmMerge,
}

#[derive(PartialEq, Eq)]
pub enum ExitAction {
    KeepRunning,
    QuitAndMerge,
    QuitWithAbort,
    QuitWithReason(String),
}
