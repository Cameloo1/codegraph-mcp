pub fn subject() -> i32 { 1 }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn works() { assert_eq!(subject(), 1); }
}
