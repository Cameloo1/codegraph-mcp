pub fn target() -> i32 { 1 }
use crate::target as aliased_target;
fn run() -> i32 { aliased_target() }
