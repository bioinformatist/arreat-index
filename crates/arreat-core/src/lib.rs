pub const APP_NAME: &str = "Arreat Index";

#[cfg(test)]
mod tests {
    use super::APP_NAME;

    #[test]
    fn app_name_is_stable() {
        assert_eq!(APP_NAME, "Arreat Index");
    }
}
