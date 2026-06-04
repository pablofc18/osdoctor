pub mod system;

use crate::model::CheckResult;

pub fn run_system(results: &mut Vec<CheckResult>) {
    system::run(results);
}
