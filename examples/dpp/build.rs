fn main() {
    let needs_esp_idf = std::env::var_os("CARGO_FEATURE_ESP32S3").is_some()
        || std::env::var_os("CARGO_FEATURE_ESP32C6").is_some();

    if needs_esp_idf {
        embuild::build::CfgArgs::output_propagated("ESP_IDF")
            .expect("failed to propagate cfg args");
        embuild::build::LinkArgs::output_propagated("ESP_IDF")
            .expect("failed to propagate link args");
    }
}
