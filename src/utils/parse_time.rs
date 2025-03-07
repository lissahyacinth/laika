use crate::errors::{LaikaError, LaikaResult};
use time::Duration;

pub fn parse_time_str(time_str: &str) -> LaikaResult<Duration> {
    let (value, unit) = time_str.split_at(
        time_str
            .chars()
            .position(|c| c.is_alphabetic())
            .ok_or(LaikaError::Generic("missing unit".to_string()))?,
    );

    let amount: i64 = value
        .parse()
        .map_err(|_| LaikaError::Generic("invalid number".to_string()))?;

    match unit {
        "ms" => Ok(Duration::milliseconds(amount)),
        "s" => Ok(Duration::seconds(amount)),
        "m" => Ok(Duration::minutes(amount)),
        "h" => Ok(Duration::hours(amount)),
        "d" => Ok(Duration::days(amount)),
        _ => Err(LaikaError::Generic(format!("unknown unit: {}", unit))),
    }
}

#[test]
fn test_parse_time_str() {
    assert_eq!(parse_time_str("30m").unwrap(), Duration::seconds(1800));
    assert_eq!(parse_time_str("1h").unwrap(), Duration::seconds(3600));
    assert_eq!(parse_time_str("24h").unwrap(), Duration::seconds(86400));
    assert!(parse_time_str("invalid").is_err());
}
