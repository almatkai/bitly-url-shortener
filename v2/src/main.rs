#[tokio::main(worker_threads = 2)]
async fn main() {
    match tezbar_bitly_url_shortener::Adapter::production() {
        Ok(adapter) => {
            if tezbar_extension_sdk::serve(adapter).await.is_err() {
                eprintln!("Bitly extension protocol ended with an error");
                std::process::exit(1);
            }
        }
        Err(_) => {
            eprintln!("Bitly extension could not initialize its secure HTTP client");
            std::process::exit(1);
        }
    }
}
