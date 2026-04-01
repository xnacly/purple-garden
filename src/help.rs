pub fn print_help_by_topic(topic: Option<&str>) -> &str {
    if let Some(topic) = topic {
        match topic {
            "types" => include_str!("../help/types.txt"),
            _ => "unknown topic",
        }
    } else {
        include_str!("../help/intro.txt")
    }
}
