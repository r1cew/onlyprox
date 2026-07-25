pub async fn fetch_subscription(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.text().await
}