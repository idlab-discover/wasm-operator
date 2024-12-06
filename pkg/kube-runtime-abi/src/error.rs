#[derive(Debug)]
pub enum SpawnerError {
    SpawnerNotInitialized,
}

impl std::fmt::Display for SpawnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnerError::SpawnerNotInitialized => write!(f, "The spawner is not initialized."),
        }
    }
}

impl std::error::Error for SpawnerError {}
