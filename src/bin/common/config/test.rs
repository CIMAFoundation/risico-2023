    #[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn parse_line_fails_for_malformed_line() {
        let line = "test/path/to/foo.txt";
        let result = parse_line(&line);
        assert!(result.is_err());
    }

    #[test]
    fn parse_line_ok() {
        let line = "test/path/to/202205060000_grid_variable.txt";
        let result = parse_line(&line);
        assert!(result.is_ok());
        let (grid, variable, date) = result.expect("should unwrap");
        assert_eq!(grid, "grid");
        assert_eq!(variable, "variable");
        assert_eq!(date, DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2022, 5, 6).and_hms(0, 0, 0), Utc));
    }
}
