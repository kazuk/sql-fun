#[path = "api_helpers/heavy.rs"]
pub mod heavy;

pub fn responses(output: &std::process::Output) -> Vec<::serde_json::Value> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| ::serde_json::from_str(line).expect("complete JSON response line"))
        .collect()
}
