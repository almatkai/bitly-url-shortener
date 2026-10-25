#[tokio::main(worker_threads = 2)]
async fn main() {
    let origin = std::env::var("BITLY_QA_ORIGIN").expect("QA loopback origin required");
    let certificate = std::env::var("BITLY_QA_CERT")
        .ok()
        .map(|p| std::fs::read(p).unwrap());
    let api = tezbar_bitly_url_shortener::http::Api::fixture(&origin, certificate.as_deref());
    tezbar_extension_sdk::serve(tezbar_bitly_url_shortener::Adapter::with_api(api))
        .await
        .unwrap();
}
