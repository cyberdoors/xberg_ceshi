#[tokio::main]
async fn main() {
    use xberg::{extract, ExtractInput, ExtractionConfig};

    let config = ExtractionConfig::default();
    let input = ExtractInput::from_uri("1.txt");
    let _ = extract(input, &config).await;
}