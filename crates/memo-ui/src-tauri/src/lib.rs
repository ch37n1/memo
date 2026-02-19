// memo-ui: Tauri v2 native desktop app backend crate.

#[must_use]
pub fn app_name() -> &'static str {
    "memo-ui"
}

#[cfg(test)]
mod tests {
    use super::app_name;

    #[test]
    fn app_name_is_stable() {
        assert_eq!(app_name(), "memo-ui");
    }
}
