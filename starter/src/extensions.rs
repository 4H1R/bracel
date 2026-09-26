// bracel:extension-modules

pub fn jobs(_worker: &mut bracel::jobs::Worker) {
    // bracel:extension-jobs
}

pub fn commands(_commands: &mut bracel::commands::Commands<sea_orm::DatabaseConnection>) {
    // bracel:extension-commands
}
