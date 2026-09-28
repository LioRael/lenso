fn main() -> std::io::Result<()> {
    lenso_capability_http_endpoint::process::serve(local_starter::Notes::default())
}
