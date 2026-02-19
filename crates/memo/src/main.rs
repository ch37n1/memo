// memo: CLI client. Thin clap app over memo-client.
// No direct filesystem or DB access.

fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn main_executes() {
        super::main();
    }
}
