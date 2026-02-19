// memo-core: shared types, error types, and scope parsing.
// No I/O. All other crates depend on this.

/// Returns the canonical service name. Used as a smoke-test for the build pipeline.
pub fn hello_world() -> &'static str {
    "memo"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_world_returns_service_name() {
        assert_eq!(hello_world(), "memo");
    }
}
