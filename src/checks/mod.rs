pub mod system;
pub mod services;

use crate::model::CheckResult;

pub fn run_system(results: &mut Vec<CheckResult>) {
    system::run(results);
}

pub fn run_services(results: &mut Vec<CheckResult>) {
    services::run(results);
}
