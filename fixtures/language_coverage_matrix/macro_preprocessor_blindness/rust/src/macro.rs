macro_rules! make_fn { () => { fn generated() -> i32 { 1 } } }
make_fn!();
fn caller() -> i32 { generated() }
