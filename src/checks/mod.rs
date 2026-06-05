pub mod system;
pub mod services;
pub mod packages;
pub mod desktop;

use crate::model::CheckResult;

pub fn run_system(results: &mut Vec<CheckResult>) {
    system::run(results);
}

pub fn run_services(results: &mut Vec<CheckResult>) {
    services::run(results);
}

pub fn run_packages(results: &mut Vec<CheckResult>) {
    packages::run(results);
}
