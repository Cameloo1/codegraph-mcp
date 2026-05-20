fn duplicate(value: i32) -> i32 { value + 1 }
mod inner {
    pub fn duplicate() -> i32 { 2 }
}
fn caller() -> i32 { duplicate(1) }
